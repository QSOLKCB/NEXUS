// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Invariant audit over every strand of a field.
//!
//! The audit reports worst-case deviations rather than pass/fail booleans so
//! that an external protocol can apply its own thresholds. The sequential and
//! parallel audits must agree exactly; the parallel path shards strands over
//! workers and reduces maxima.

use crate::error::Result;
use crate::exec::effective_workers;
use crate::field::{Field, Ordering};
use crate::transit::{eversion, position};

/// Worst-case invariant deviations across a field.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Audit {
    /// `max_i | ||egress_i|| - R |`.
    pub radius_error: f64,
    /// `max_i || ingress_i + egress_i ||`.
    pub antipodal_error: f64,
    /// `max_i || P_i(1/2) ||`.
    pub nexus_error: f64,
    /// `max_i || P_i(0) - ingress_i ||` and the egress counterpart, combined.
    pub endpoint_error: f64,
    /// Spread of consecutive `|logical_i+1 - logical_i|` distances minus one;
    /// zero means an exact floor partition. Large under the golden ordering,
    /// which is not a partition.
    pub gap_error: u64,
    /// Eversion at `p = 1/4`, `1/2`, `3/4`.
    pub eversion_before: f64,
    /// Eversion at the nexus.
    pub eversion_at: f64,
    /// Eversion after the nexus.
    pub eversion_after: f64,
    /// Strand of the first tracer.
    pub tracer_first: u32,
    /// Strand of the last tracer.
    pub tracer_last: u32,
    /// Strands examined.
    pub strands: u32,
}

impl Audit {
    /// True when the eversion sign sequence is negative, zero, positive.
    pub fn eversion_sign_ok(&self) -> bool {
        self.eversion_before < 0.0 && self.eversion_at == 0.0 && self.eversion_after > 0.0
    }

    /// Fraction of the strand range covered by tracers.
    pub fn tracer_coverage(&self) -> f64 {
        if self.strands <= 1 {
            1.0
        } else {
            f64::from(self.tracer_last) / f64::from(self.strands - 1)
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Partial {
    radius: f64,
    antipodal: f64,
    nexus: f64,
    endpoint: f64,
    gap_min: u64,
    gap_max: u64,
    first_logical: u64,
    last_logical: u64,
    count: u32,
}

impl Partial {
    const EMPTY: Self = Self {
        radius: 0.0,
        antipodal: 0.0,
        nexus: 0.0,
        endpoint: 0.0,
        gap_min: u64::MAX,
        gap_max: 0,
        first_logical: 0,
        last_logical: 0,
        count: 0,
    };

    fn merge(self, other: Self) -> Self {
        if other.count == 0 {
            return self;
        }
        if self.count == 0 {
            return other;
        }
        let boundary = other.first_logical.abs_diff(self.last_logical);
        Self {
            radius: self.radius.max(other.radius),
            antipodal: self.antipodal.max(other.antipodal),
            nexus: self.nexus.max(other.nexus),
            endpoint: self.endpoint.max(other.endpoint),
            gap_min: self.gap_min.min(other.gap_min).min(boundary),
            gap_max: self.gap_max.max(other.gap_max).max(boundary),
            first_logical: self.first_logical,
            last_logical: other.last_logical,
            count: self.count + other.count,
        }
    }
}

fn audit_range(field: &Field, ordering: Ordering, start: u32, end: u32) -> Result<Partial> {
    let mut partial = Partial::EMPTY;
    let mut previous: Option<u64> = None;
    for ordinal in start..end {
        let strand = field.strand(ordinal, ordering)?;
        let radius = (strand.egress.norm() - field.radius()).abs();
        let antipodal = (strand.ingress.x + strand.egress.x).hypot(strand.ingress.y + strand.egress.y);
        let nexus = position(field, &strand, 0.5)?.norm();
        let start_pos = position(field, &strand, 0.0)?;
        let end_pos = position(field, &strand, 1.0)?;
        let endpoint = (start_pos.x - strand.ingress.x)
            .hypot(start_pos.y - strand.ingress.y)
            .max((end_pos.x - strand.egress.x).hypot(end_pos.y - strand.egress.y));
        if let Some(prev) = previous {
            let gap = strand.logical.abs_diff(prev);
            partial.gap_min = partial.gap_min.min(gap);
            partial.gap_max = partial.gap_max.max(gap);
        } else {
            partial.first_logical = strand.logical;
        }
        previous = Some(strand.logical);
        partial.last_logical = strand.logical;
        partial.radius = partial.radius.max(radius);
        partial.antipodal = partial.antipodal.max(antipodal);
        partial.nexus = partial.nexus.max(nexus);
        partial.endpoint = partial.endpoint.max(endpoint);
        partial.count += 1;
    }
    Ok(partial)
}

fn finish(field: &Field, partial: Partial) -> Result<Audit> {
    let gap_error = if partial.count < 2 || partial.gap_min == u64::MAX {
        0
    } else {
        (partial.gap_max - partial.gap_min).saturating_sub(1)
    };
    Ok(Audit {
        radius_error: partial.radius,
        antipodal_error: partial.antipodal,
        nexus_error: partial.nexus,
        endpoint_error: partial.endpoint,
        gap_error,
        eversion_before: eversion(0.25)?,
        eversion_at: eversion(0.5)?,
        eversion_after: eversion(0.75)?,
        tracer_first: field.tracer_strand(0)?,
        tracer_last: field.tracer_strand(field.tracers() - 1)?,
        strands: field.strands(),
    })
}

/// Sequential audit of every strand.
pub fn audit(field: &Field, ordering: Ordering) -> Result<Audit> {
    let partial = audit_range(field, ordering, 0, field.strands())?;
    finish(field, partial)
}

/// Parallel audit with up to `workers` threads. Results equal [`audit`].
pub fn audit_parallel(field: &Field, ordering: Ordering, workers: usize) -> Result<Audit> {
    let strands = field.strands() as usize;
    let workers = effective_workers(strands, workers)?;
    if workers == 1 {
        return audit(field, ordering);
    }
    let shard = strands.div_ceil(workers);
    let partials: Vec<Result<Partial>> = shards(field, ordering, shard, workers);
    let mut merged = Partial::EMPTY;
    for partial in partials {
        merged = merged.merge(partial?);
    }
    finish(field, merged)
}

#[cfg(not(target_family = "wasm"))]
fn shards(field: &Field, ordering: Ordering, shard: usize, workers: usize) -> Vec<Result<Partial>> {
    let total = field.strands() as usize;
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|w| {
                let start = (w * shard).min(total);
                let end = ((w + 1) * shard).min(total);
                scope.spawn(move || audit_range(field, ordering, start as u32, end as u32))
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap_or(Err(crate::error::Error::Worker)))
            .collect()
    })
}

#[cfg(target_family = "wasm")]
fn shards(field: &Field, ordering: Ordering, _shard: usize, _workers: usize) -> Vec<Result<Partial>> {
    vec![audit_range(field, ordering, 0, field.strands())]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::FieldSpec;

    #[test]
    fn default_field_has_exact_invariants() {
        let field = FieldSpec::default().build().unwrap();
        let report = audit(&field, Ordering::Uniform).unwrap();
        assert_eq!(report.nexus_error, 0.0);
        assert_eq!(report.antipodal_error, 0.0);
        assert_eq!(report.endpoint_error, 0.0);
        assert_eq!(report.gap_error, 0);
        assert!(report.radius_error < 1e-15);
        assert!(report.eversion_sign_ok());
        assert_eq!(report.tracer_first, 0);
    }

    #[test]
    fn parallel_audit_equals_sequential_audit() {
        for strands in [1_u32, 2, 3, 1_000, 4_096] {
            let field = FieldSpec {
                cardinality: 1_000_003,
                strands,
                ..FieldSpec::default()
            }
            .build()
            .unwrap();
            let sequential = audit(&field, Ordering::Uniform).unwrap();
            for workers in [2, 3, 8, 17] {
                assert_eq!(audit_parallel(&field, Ordering::Uniform, workers).unwrap(), sequential);
            }
        }
    }

    #[test]
    fn golden_ordering_keeps_geometry_but_not_the_floor_partition() {
        let field = FieldSpec::default().build().unwrap();
        let report = audit(&field, Ordering::Golden).unwrap();
        assert_eq!(report.nexus_error, 0.0);
        assert_eq!(report.antipodal_error, 0.0);
        assert!(report.gap_error > 0);
    }
}
