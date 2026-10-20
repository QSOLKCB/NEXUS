// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Centre-transit path: ingress, exact nexus, egress.
//!
//! The path is written in a single signed-progress form. With
//! `s = 2p - 1 in [-1, 1]`, a strand with bearing `theta` is at
//!
//! ```text
//! psi(s) = -sign(s) * 2 pi * twist * (1 - |s|)
//! P(s)   = s * R * (cos(theta + psi), sin(theta + psi))
//! ```
//!
//! `s = -1` gives the ingress, `s = 0` gives the exact origin because the
//! radial factor is literally zero, and `s = +1` gives the egress. No branch
//! on the half of the path is needed, and antipodality of the endpoints
//! follows from the sign of `s` alone.

use std::f64::consts::TAU;

use crate::error::{Error, Result};
use crate::field::{Field, Strand};

/// A planar position.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Pos {
    /// Horizontal coordinate.
    pub x: f64,
    /// Vertical coordinate.
    pub y: f64,
}

impl Pos {
    /// The exact origin.
    pub const ORIGIN: Self = Self { x: 0.0, y: 0.0 };

    /// Euclidean norm.
    pub fn norm(self) -> f64 {
        self.x.hypot(self.y)
    }
}

/// Validate progress and convert it to signed progress `s = 2p - 1`.
pub fn signed(progress: f64) -> Result<f64> {
    if progress.is_finite() && (0.0..=1.0).contains(&progress) {
        Ok(2.0 * progress - 1.0)
    } else {
        Err(Error::Progress(progress))
    }
}

/// Eversion sign: negative inbound, exactly zero at the nexus, positive
/// outbound. This is the signed progress itself.
pub fn eversion(progress: f64) -> Result<f64> {
    signed(progress)
}

fn sign(value: f64) -> f64 {
    if value > 0.0 {
        1.0
    } else if value < 0.0 {
        -1.0
    } else {
        0.0
    }
}

/// Position of a strand at a given progress.
pub fn position(field: &Field, strand: &Strand, progress: f64) -> Result<Pos> {
    let s = signed(progress)?;
    Ok(position_signed(field, strand, s))
}

/// Position for an already-validated signed progress.
pub fn position_signed(field: &Field, strand: &Strand, s: f64) -> Pos {
    let twist_angle = -sign(s) * TAU * field.twist() * (1.0 - s.abs());
    let angle = strand.bearing + twist_angle;
    let radial = s * field.radius();
    let x = radial * angle.cos();
    let y = radial * angle.sin();
    // Fold negative zero into positive zero so the nexus compares equal to
    // `Pos::ORIGIN` bit for bit.
    Pos { x: x + 0.0, y: y + 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::{FieldSpec, Ordering};

    #[test]
    fn nexus_is_the_exact_origin() {
        let field = FieldSpec::default().build().unwrap();
        for strand in field.iter(Ordering::Uniform) {
            assert_eq!(position(&field, &strand, 0.5).unwrap(), Pos::ORIGIN);
        }
    }

    #[test]
    fn endpoints_match_strand_geometry_bit_for_bit() {
        let field = FieldSpec {
            twist: 2.25,
            phase: 0.3,
            ..FieldSpec::default()
        }
        .build()
        .unwrap();
        for strand in field.iter(Ordering::Golden) {
            let start = position(&field, &strand, 0.0).unwrap();
            let end = position(&field, &strand, 1.0).unwrap();
            assert_eq!(start.x, strand.ingress.x + 0.0);
            assert_eq!(start.y, strand.ingress.y + 0.0);
            assert_eq!(end, strand.egress);
        }
    }

    #[test]
    fn radius_shrinks_then_grows_monotonically() {
        let field = FieldSpec::default().build().unwrap();
        let strand = field.strand(17, Ordering::Uniform).unwrap();
        let mut last = f64::INFINITY;
        for step in 0..=500 {
            let r = position(&field, &strand, f64::from(step) / 1000.0).unwrap().norm();
            assert!(r <= last);
            last = r;
        }
        for step in 500..=1000 {
            let r = position(&field, &strand, f64::from(step) / 1000.0).unwrap().norm();
            assert!(r >= last);
            last = r;
        }
    }

    #[test]
    fn eversion_sign_flips_once() {
        assert!(eversion(0.25).unwrap() < 0.0);
        assert_eq!(eversion(0.5).unwrap(), 0.0);
        assert!(eversion(0.75).unwrap() > 0.0);
        assert!(eversion(1.5).is_err());
        assert!(eversion(f64::NAN).is_err());
    }
}
