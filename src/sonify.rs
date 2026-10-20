// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Deterministic sonification controls derived from samples.
//!
//! The mapping is configurable. The default [`EventMap`] is the 432 Hz map:
//! lanes add 0, 4, or 7 semitones; inbound, nexus, and outbound add -2, 0, or
//! +2 semitones; gain follows normalised radius with a full-scale impulse at
//! the exact nexus; pan follows normalised `x`. These are mappings for an
//! audio layer, not properties of the geometry.

use crate::error::{Error, Result};
use crate::exec::Sample;
use crate::field::Field;
use crate::ternary::Trit;

/// Semitone and gain tables for event generation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EventMap {
    /// Base frequency in hertz.
    pub base_hz: f64,
    /// Semitone offset per lane index.
    pub lane_semitones: [f64; 3],
    /// Semitone offset for inbound, nexus, outbound.
    pub trit_semitones: [f64; 3],
    /// Gain at the receiver edge is 1; gain approaches this floor near the
    /// centre, except at the exact nexus which emits a full-scale impulse.
    pub gain_floor: f64,
}

impl Default for EventMap {
    fn default() -> Self {
        Self::at(432.0)
    }
}

impl EventMap {
    /// The default tables at a chosen base frequency.
    pub const fn at(base_hz: f64) -> Self {
        Self {
            base_hz,
            lane_semitones: [0.0, 4.0, 7.0],
            trit_semitones: [-2.0, 0.0, 2.0],
            gain_floor: 0.2,
        }
    }

    fn validate(&self) -> Result<()> {
        if !self.base_hz.is_finite() || self.base_hz <= 0.0 {
            return Err(Error::Frequency(self.base_hz));
        }
        if !self.gain_floor.is_finite() || !(0.0..=1.0).contains(&self.gain_floor) {
            return Err(Error::Spec {
                name: "gain_floor",
                requirement: "must be finite and in [0, 1]",
            });
        }
        if self
            .lane_semitones
            .iter()
            .chain(&self.trit_semitones)
            .any(|v| !v.is_finite())
        {
            return Err(Error::Spec {
                name: "semitones",
                requirement: "must all be finite",
            });
        }
        Ok(())
    }
}

/// One sonification control event.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Event {
    /// Frequency in hertz.
    pub hz: f64,
    /// Gain in `[0, 1]`.
    pub gain: f64,
    /// Stereo pan in `[-1, 1]`.
    pub pan: f64,
}

/// Map a sample to an event.
pub fn event(field: &Field, map: &EventMap, sample: &Sample) -> Result<Event> {
    map.validate()?;
    let trit_slot = (sample.trit.value() + 1) as usize;
    let semitones = map.lane_semitones[sample.lane.index()] + map.trit_semitones[trit_slot];
    let hz = map.base_hz * (semitones / 12.0).exp2();
    if !hz.is_finite() || hz <= 0.0 {
        return Err(Error::Frequency(hz));
    }
    let normalised = (sample.pos.norm() / field.radius()).clamp(0.0, 1.0);
    let gain = if sample.trit == Trit::Nexus {
        1.0
    } else {
        map.gain_floor + (1.0 - map.gain_floor) * normalised
    };
    let pan = (sample.pos.x / field.radius()).clamp(-1.0, 1.0);
    Ok(Event { hz, gain, pan })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::{sample, Job};
    use crate::field::{FieldSpec, Ordering};

    #[test]
    fn nexus_impulse_is_full_scale_and_centred() {
        let field = FieldSpec::default().build().unwrap();
        let map = EventMap::default();
        let centre = sample(
            &field,
            Ordering::Uniform,
            Job {
                ordinal: 4,
                progress: 0.5,
            },
        )
        .unwrap();
        let e = event(&field, &map, &centre).unwrap();
        assert_eq!(e.gain, 1.0);
        assert_eq!(e.pan, 0.0);
        assert!((e.hz - 432.0 * (4.0_f64 / 12.0).exp2()).abs() < 1e-9);
    }

    #[test]
    fn direction_shifts_pitch_symmetrically() {
        let field = FieldSpec::default().build().unwrap();
        let map = EventMap::default();
        let inbound = sample(
            &field,
            Ordering::Uniform,
            Job {
                ordinal: 0,
                progress: 0.1,
            },
        )
        .unwrap();
        let outbound = sample(
            &field,
            Ordering::Uniform,
            Job {
                ordinal: 0,
                progress: 0.9,
            },
        )
        .unwrap();
        let lo = event(&field, &map, &inbound).unwrap();
        let hi = event(&field, &map, &outbound).unwrap();
        assert!(lo.hz < 432.0 && hi.hz > 432.0);
        assert!((lo.hz * hi.hz - 432.0 * 432.0).abs() < 1e-6);
        assert!(lo.gain < 1.0 && lo.gain >= 0.2);
    }

    #[test]
    fn bad_base_frequency_is_rejected() {
        let field = FieldSpec::default().build().unwrap();
        let s = sample(
            &field,
            Ordering::Uniform,
            Job {
                ordinal: 2,
                progress: 0.9,
            },
        )
        .unwrap();
        assert!(event(&field, &EventMap::at(0.0), &s).is_err());
        assert!(event(&field, &EventMap::at(f64::NAN), &s).is_err());
        // Lane 2 outbound shifts upward by nine semitones, so f64::MAX overflows.
        assert!(event(&field, &EventMap::at(f64::MAX), &s).is_err());
    }
}
