// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Field specification, validation, and exact strand selection.
//!
//! A *field* has a logical cardinality `L` that may be far larger than
//! anything materialised in memory. A bounded number of *strands* `N`
//! represent it. Each strand is tied to one logical index chosen by an exact
//! integer rule, so no `f64` rounding enters the selection step.

use std::f64::consts::TAU;

use crate::error::{Error, Result};
use crate::transit::Pos;

/// Fixed-point `2^64 / phi`, the golden-ratio Kronecker step.
const GOLDEN_STEP: u64 = 0x9E37_79B9_7F4A_7C15;

/// Unvalidated field parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldSpec {
    /// Logical cardinality `L`. Never allocated.
    pub cardinality: u64,
    /// Number of representative strands `N` (`1 <= N <= L`).
    pub strands: u32,
    /// Bounded tracer budget, independent of `L` and `N`.
    pub tracers: u32,
    /// Receiver radius `R` (finite, positive).
    pub radius: f64,
    /// Angular phase offset in radians (finite).
    pub phase: f64,
    /// Twist per half-transit in whole turns (finite).
    pub twist: f64,
}

impl Default for FieldSpec {
    fn default() -> Self {
        Self {
            cardinality: 1 << 24,
            strands: 1_024,
            tracers: 512,
            radius: 0.56,
            phase: 0.0,
            twist: 1.5,
        }
    }
}

impl FieldSpec {
    /// Validate into a [`Field`].
    pub fn build(self) -> Result<Field> {
        Field::new(self)
    }
}

/// How strand ordinals are mapped onto logical indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Ordering {
    /// Canonical exact floor partition `floor(i L / N)`.
    #[default]
    Uniform,
    /// Golden-ratio Kronecker ordering computed in 64.64 fixed point.
    Golden,
}

impl Ordering {
    /// Stable lowercase label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Uniform => "uniform",
            Self::Golden => "golden",
        }
    }

    /// Stable single-byte code for canonical encodings.
    pub const fn code(self) -> u8 {
        match self {
            Self::Uniform => 0,
            Self::Golden => 1,
        }
    }
}

/// A validated field. Construct with [`FieldSpec::build`] or [`Field::new`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Field {
    spec: FieldSpec,
}

/// One representative strand with its exact logical index and endpoints.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Strand {
    /// Position in the representative set (`0 <= ordinal < N`).
    pub ordinal: u32,
    /// Exact logical index this strand represents.
    pub logical: u64,
    /// Bearing angle of the egress point in radians.
    pub bearing: f64,
    /// Egress point on the receiver circle.
    pub egress: Pos,
    /// Ingress point, exactly antipodal to the egress.
    pub ingress: Pos,
}

impl Field {
    /// Validate a specification.
    pub fn new(spec: FieldSpec) -> Result<Self> {
        if spec.cardinality == 0 {
            return Err(Error::Spec {
                name: "cardinality",
                requirement: "must be greater than zero",
            });
        }
        if spec.strands == 0 {
            return Err(Error::Spec {
                name: "strands",
                requirement: "must be greater than zero",
            });
        }
        if u64::from(spec.strands) > spec.cardinality {
            return Err(Error::Spec {
                name: "strands",
                requirement: "must not exceed cardinality",
            });
        }
        if spec.tracers == 0 {
            return Err(Error::Spec {
                name: "tracers",
                requirement: "must be greater than zero",
            });
        }
        if !(spec.radius.is_finite() && spec.radius > 0.0) {
            return Err(Error::Spec {
                name: "radius",
                requirement: "must be finite and positive",
            });
        }
        if !spec.phase.is_finite() {
            return Err(Error::Spec {
                name: "phase",
                requirement: "must be finite",
            });
        }
        if !spec.twist.is_finite() {
            return Err(Error::Spec {
                name: "twist",
                requirement: "must be finite",
            });
        }
        if !(spec.phase.abs() + TAU + TAU * spec.twist.abs()).is_finite() {
            return Err(Error::Spec {
                name: "twist",
                requirement: "and phase must keep the transit angle finite",
            });
        }
        Ok(Self { spec })
    }

    /// The validated specification.
    pub const fn spec(&self) -> &FieldSpec {
        &self.spec
    }

    /// Logical cardinality `L`.
    pub const fn cardinality(&self) -> u64 {
        self.spec.cardinality
    }

    /// Strand count `N`.
    pub const fn strands(&self) -> u32 {
        self.spec.strands
    }

    /// Tracer budget.
    pub const fn tracers(&self) -> u32 {
        self.spec.tracers
    }

    /// Receiver radius `R`.
    pub const fn radius(&self) -> f64 {
        self.spec.radius
    }

    /// Angular phase.
    pub const fn phase(&self) -> f64 {
        self.spec.phase
    }

    /// Twist per half-transit in turns.
    pub const fn twist(&self) -> f64 {
        self.spec.twist
    }

    fn check_ordinal(&self, ordinal: u32) -> Result<()> {
        if ordinal < self.spec.strands {
            Ok(())
        } else {
            Err(Error::Ordinal {
                kind: "strand",
                value: u64::from(ordinal),
                bound: u64::from(self.spec.strands),
            })
        }
    }

    /// Exact logical index for one ordinal under the given ordering.
    pub fn logical(&self, ordinal: u32, ordering: Ordering) -> Result<u64> {
        self.check_ordinal(ordinal)?;
        Ok(match ordering {
            Ordering::Uniform => uniform_logical(self.spec.cardinality, self.spec.strands, ordinal),
            Ordering::Golden => golden_logical(self.spec.cardinality, ordinal),
        })
    }

    /// Build a strand from an already-known logical index.
    pub fn strand_at(&self, ordinal: u32, logical: u64) -> Strand {
        let fraction = logical as f64 / self.spec.cardinality as f64;
        let bearing = self.spec.phase + TAU * fraction;
        let egress = Pos {
            x: self.spec.radius * bearing.cos(),
            y: self.spec.radius * bearing.sin(),
        };
        Strand {
            ordinal,
            logical,
            bearing,
            egress,
            ingress: Pos {
                x: -egress.x,
                y: -egress.y,
            },
        }
    }

    /// Strand for one ordinal.
    pub fn strand(&self, ordinal: u32, ordering: Ordering) -> Result<Strand> {
        let logical = self.logical(ordinal, ordering)?;
        Ok(self.strand_at(ordinal, logical))
    }

    /// Iterate every strand in ordinal order using carry arithmetic only.
    pub fn iter(&self, ordering: Ordering) -> StrandIter<'_> {
        StrandIter {
            field: self,
            ordering,
            next: 0,
            cursor: Cursor::start(self.spec.cardinality, self.spec.strands),
        }
    }

    /// Strand ordinal a tracer is attached to: `floor(j N / P)`.
    pub fn tracer_strand(&self, tracer: u32) -> Result<u32> {
        if tracer >= self.spec.tracers {
            return Err(Error::Ordinal {
                kind: "tracer",
                value: u64::from(tracer),
                bound: u64::from(self.spec.tracers),
            });
        }
        let value = u64::from(tracer) * u64::from(self.spec.strands) / u64::from(self.spec.tracers);
        Ok(value as u32)
    }
}

/// Random-access exact floor map with a 128-bit intermediate.
pub fn uniform_logical(cardinality: u64, strands: u32, ordinal: u32) -> u64 {
    let product = u128::from(cardinality) * u128::from(ordinal);
    (product / u128::from(strands)) as u64
}

/// Golden ordering: `floor(fract(i / phi) * L)` with the fraction held exactly
/// in 64-bit fixed point. The result is always below `cardinality`.
pub fn golden_logical(cardinality: u64, ordinal: u32) -> u64 {
    let fraction = u64::from(ordinal).wrapping_mul(GOLDEN_STEP);
    ((u128::from(fraction) * u128::from(cardinality)) >> 64) as u64
}

/// Incremental floor-map cursor: advances by `L / N` plus a carried remainder.
#[derive(Debug, Clone, Copy)]
struct Cursor {
    strands: u64,
    quotient: u64,
    step_remainder: u64,
    logical: u64,
    remainder: u64,
}

impl Cursor {
    fn start(cardinality: u64, strands: u32) -> Self {
        let strands = u64::from(strands);
        Self {
            strands,
            quotient: cardinality / strands,
            step_remainder: cardinality % strands,
            logical: 0,
            remainder: 0,
        }
    }

    fn advance(&mut self) {
        self.logical += self.quotient;
        self.remainder += self.step_remainder;
        if self.remainder >= self.strands {
            self.remainder -= self.strands;
            self.logical += 1;
        }
    }
}

/// Iterator returned by [`Field::iter`].
#[derive(Debug, Clone)]
pub struct StrandIter<'a> {
    field: &'a Field,
    ordering: Ordering,
    next: u32,
    cursor: Cursor,
}

impl Iterator for StrandIter<'_> {
    type Item = Strand;

    fn next(&mut self) -> Option<Strand> {
        if self.next >= self.field.strands() {
            return None;
        }
        let ordinal = self.next;
        let logical = match self.ordering {
            Ordering::Uniform => {
                let value = self.cursor.logical;
                self.cursor.advance();
                value
            }
            Ordering::Golden => golden_logical(self.field.cardinality(), ordinal),
        };
        self.next += 1;
        Some(self.field.strand_at(ordinal, logical))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = (self.field.strands() - self.next) as usize;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for StrandIter<'_> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incremental_cursor_matches_random_access_floor_map() {
        for (cardinality, strands) in [(1_u64 << 24, 1_024_u32), (1_000_003, 777), (97, 97), (10, 3)] {
            let field = FieldSpec {
                cardinality,
                strands,
                ..FieldSpec::default()
            }
            .build()
            .unwrap();
            for (ordinal, strand) in field.iter(Ordering::Uniform).enumerate() {
                assert_eq!(strand.logical, uniform_logical(cardinality, strands, ordinal as u32));
            }
        }
    }

    #[test]
    fn floor_map_gaps_differ_by_at_most_one() {
        let field = FieldSpec {
            cardinality: 1_000_003,
            strands: 4_096,
            ..FieldSpec::default()
        }
        .build()
        .unwrap();
        let logical: Vec<u64> = field.iter(Ordering::Uniform).map(|s| s.logical).collect();
        let gaps: Vec<u64> = logical.windows(2).map(|w| w[1] - w[0]).collect();
        let min = *gaps.iter().min().unwrap();
        let max = *gaps.iter().max().unwrap();
        assert!(max - min <= 1);
        assert_eq!(logical[0], 0);
    }

    #[test]
    fn golden_ordering_stays_in_range_and_is_distinct_for_small_counts() {
        let cardinality = 1_u64 << 32;
        let mut seen = std::collections::HashSet::new();
        for ordinal in 0..10_000_u32 {
            let value = golden_logical(cardinality, ordinal);
            assert!(value < cardinality);
            assert!(seen.insert(value), "collision at ordinal {ordinal}");
        }
        // Roughly uniform coverage: the ordering fills the field in fractions.
        assert_eq!(golden_logical(cardinality, 0), 0);
    }

    #[test]
    fn endpoints_are_exactly_antipodal() {
        let field = FieldSpec::default().build().unwrap();
        for strand in field.iter(Ordering::Uniform) {
            assert_eq!(strand.ingress.x, -strand.egress.x);
            assert_eq!(strand.ingress.y, -strand.egress.y);
        }
    }

    #[test]
    fn spec_errors_name_the_field() {
        let bad = FieldSpec {
            strands: 0,
            ..FieldSpec::default()
        };
        assert!(matches!(bad.build(), Err(Error::Spec { name: "strands", .. })));
        let bad = FieldSpec {
            cardinality: 10,
            strands: 11,
            ..FieldSpec::default()
        };
        assert!(matches!(bad.build(), Err(Error::Spec { name: "strands", .. })));
        let bad = FieldSpec {
            radius: f64::NAN,
            ..FieldSpec::default()
        };
        assert!(matches!(bad.build(), Err(Error::Spec { name: "radius", .. })));
    }

    #[test]
    fn tracers_cover_the_strand_range() {
        let field = FieldSpec::default().build().unwrap();
        assert_eq!(field.tracer_strand(0).unwrap(), 0);
        let last = field.tracer_strand(field.tracers() - 1).unwrap();
        assert!(last < field.strands());
        assert!(field.tracer_strand(field.tracers()).is_err());
    }
}
