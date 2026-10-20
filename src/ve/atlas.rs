// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Twelve-direction state atlas over transit samples.
//!
//! The cuboctahedron's twelve vertices split into three great-square families
//! (`XY`, `XZ`, `YZ`) of four signed states each. A normalised sample vector
//! `(x / R, y / R, eversion)` is classified by dropping its smallest
//! component; the two signs that remain select one of four quadrant states.
//! The exact nexus is kept separate. A deterministic walk on `Z_3 x Z_4` then
//! advances the family and quadrant registers in opposite senses for inbound
//! and outbound samples of the same lane, so the walk is reversible by
//! direction. These are classical labels, not a physical ququart.

use crate::error::{Error, Result};
use crate::exec::{canonical_job, sample_strand, Sample, StrandTable};
use crate::field::{Field, Ordering};
use crate::ternary::{Lane, Trit};
use crate::ve::coefficient_of_variation;

/// A classified direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Direction {
    /// Great-square family: 0 = `XY`, 1 = `XZ`, 2 = `YZ`.
    pub family: u8,
    /// Signed quadrant in `0..4`.
    pub quadrant: u8,
}

impl Direction {
    /// Flat index in `0..12`.
    pub const fn index(self) -> usize {
        self.family as usize * 4 + self.quadrant as usize
    }

    /// Family label.
    pub const fn family_label(self) -> &'static str {
        match self.family {
            0 => "xy",
            1 => "xz",
            _ => "yz",
        }
    }
}

/// Classify a sample. Returns `None` at the exact nexus.
pub fn classify(field: &Field, sample: &Sample) -> Option<Direction> {
    let v = [
        sample.pos.x / field.radius(),
        sample.pos.y / field.radius(),
        sample.eversion,
    ];
    if v.iter().all(|c| *c == 0.0) {
        return None;
    }
    let magnitudes = [v[0].abs(), v[1].abs(), v[2].abs()];
    // Drop the smallest component; ties resolve to the lowest index.
    let mut dropped = 0;
    for i in 1..3 {
        if magnitudes[i] < magnitudes[dropped] {
            dropped = i;
        }
    }
    let (family, first, second) = match dropped {
        2 => (0, v[0], v[1]),
        1 => (1, v[0], v[2]),
        _ => (2, v[1], v[2]),
    };
    let quadrant = u8::from(first < 0.0) | (u8::from(second < 0.0) << 1);
    Some(Direction { family, quadrant })
}

/// Advance a direction `rounds` times along the lane-and-direction walk.
pub fn walk(start: Direction, lane: Lane, trit: Trit, rounds: u32) -> Direction {
    let outbound = trit == Trit::Outbound;
    let quadrant_step: u8 = if outbound { 1 } else { 3 };
    let family_out = ((lane.index() + 1) % 3) as u8;
    let family_step = if outbound { family_out } else { (3 - family_out) % 3 };
    let r = rounds % 12;
    Direction {
        family: ((u32::from(start.family) + u32::from(family_step) * r) % 3) as u8,
        quadrant: ((u32::from(start.quadrant) + u32::from(quadrant_step) * r) % 4) as u8,
    }
}

/// Aggregate atlas census over a job stream.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AtlasCensus {
    /// Twelve-bin histogram of initial directions.
    pub initial: [u64; 12],
    /// Twelve-bin histogram after the walk.
    pub final_state: [u64; 12],
    /// Final family occupancy.
    pub families: [u64; 3],
    /// Final quadrant occupancy.
    pub quadrants: [u64; 4],
    /// Samples at the exact nexus (not classified).
    pub nexus: u64,
    /// Samples whose initial family equals their lane index.
    pub coherence: u64,
    /// Samples absorbed.
    pub count: u64,
}

impl AtlasCensus {
    /// Absorb one sample.
    pub fn absorb(&mut self, field: &Field, sample: &Sample, rounds: u32) {
        self.count += 1;
        let Some(start) = classify(field, sample) else {
            self.nexus += 1;
            return;
        };
        self.initial[start.index()] += 1;
        if usize::from(start.family) == sample.lane.index() {
            self.coherence += 1;
        }
        let end = walk(start, sample.lane, sample.trit, rounds);
        self.final_state[end.index()] += 1;
        self.families[end.family as usize] += 1;
        self.quadrants[end.quadrant as usize] += 1;
    }

    /// Structural validity.
    pub fn validate(&self) -> Result<()> {
        let classified = self
            .count
            .checked_sub(self.nexus)
            .ok_or(Error::Lattice("nexus count exceeds sample count"))?;
        let sum = |bins: &[u64]| bins.iter().map(|&v| u128::from(v)).sum::<u128>();
        if sum(&self.initial) != u128::from(classified)
            || sum(&self.final_state) != u128::from(classified)
            || sum(&self.families) != u128::from(classified)
            || sum(&self.quadrants) != u128::from(classified)
            || self.coherence > classified
        {
            return Err(Error::Lattice("atlas census totals disagree"));
        }
        for family in 0..3 {
            if sum(&self.final_state[family * 4..family * 4 + 4]) != u128::from(self.families[family]) {
                return Err(Error::Lattice("atlas family histogram disagrees with directions"));
            }
        }
        for quadrant in 0..4 {
            let total: u128 = (0..3).map(|f| u128::from(self.final_state[f * 4 + quadrant])).sum();
            if total != u128::from(self.quadrants[quadrant]) {
                return Err(Error::Lattice("atlas quadrant histogram disagrees with directions"));
            }
        }
        Ok(())
    }

    /// Coherence as a fraction of classified samples.
    pub fn coherence_ratio(&self) -> f64 {
        let classified = self.count.saturating_sub(self.nexus);
        if classified == 0 {
            0.0
        } else {
            self.coherence as f64 / classified as f64
        }
    }

    /// Balance statistics: (initial 12-bin CV, final 12-bin CV, family CV, quadrant CV).
    pub fn balance(&self) -> (f64, f64, f64, f64) {
        (
            coefficient_of_variation(&self.initial),
            coefficient_of_variation(&self.final_state),
            coefficient_of_variation(&self.families),
            coefficient_of_variation(&self.quadrants),
        )
    }
}

/// Census of the first `count` canonical jobs with `rounds` walk steps each.
pub fn canonical_census(field: &Field, ordering: Ordering, count: u64, rounds: u32) -> Result<AtlasCensus> {
    if count == 0 {
        return Err(Error::Zero("samples"));
    }
    let table = StrandTable::new(*field, ordering);
    let mut census = AtlasCensus::default();
    for index in 0..count {
        let job = canonical_job(field, index);
        let strand = table.strand(job.ordinal)?;
        let sample = sample_strand(field, strand, ordering, job.progress)?;
        census.absorb(field, &sample, rounds);
    }
    Ok(census)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::{sample, Job};
    use crate::field::FieldSpec;

    #[test]
    fn nexus_is_unclassified_and_everything_else_lands_in_twelve_bins() {
        let field = FieldSpec::default().build().unwrap();
        let centre = sample(
            &field,
            Ordering::Uniform,
            Job {
                ordinal: 0,
                progress: 0.5,
            },
        )
        .unwrap();
        assert_eq!(classify(&field, &centre), None);
        for ordinal in 0..64 {
            for step in [0.0, 0.1, 0.4, 0.6, 0.9, 1.0] {
                let s = sample(
                    &field,
                    Ordering::Uniform,
                    Job {
                        ordinal,
                        progress: step,
                    },
                )
                .unwrap();
                let d = classify(&field, &s).unwrap();
                assert!(d.index() < 12);
            }
        }
    }

    #[test]
    fn walk_is_reversible_by_direction() {
        for lane in Lane::ALL {
            for family in 0..3 {
                for quadrant in 0..4 {
                    let start = Direction { family, quadrant };
                    for rounds in [1, 5, 16, 128] {
                        let out = walk(start, lane, Trit::Outbound, rounds);
                        let back = walk(out, lane, Trit::Inbound, rounds);
                        assert_eq!(back, start);
                    }
                }
            }
        }
    }

    #[test]
    fn census_totals_are_consistent() {
        let field = FieldSpec::default().build().unwrap();
        let census = canonical_census(&field, Ordering::Uniform, 10_010, 16).unwrap();
        census.validate().unwrap();
        assert!(census.nexus > 0);
        assert_eq!(census.count, 10_010);
        let (a, b, c, d) = census.balance();
        assert!(a.is_finite() && b.is_finite() && c.is_finite() && d.is_finite());
    }
}
