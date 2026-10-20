// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Symbolic ternary classification over continuous geometry.
//!
//! A [`Trit`] names which leg of the transit a progress value lies on. A
//! [`Lane`] deterministically partitions strands into three execution lanes.
//! Both are metadata: they organise work and evidence, they do not change the
//! geometry, and they do not by themselves create parallelism.

use crate::error::Result;
use crate::transit::signed;

/// Which leg of the transit a progress value belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Trit {
    /// `0 <= p < 1/2`, value `-1`.
    Inbound,
    /// `p == 1/2`, value `0`.
    Nexus,
    /// `1/2 < p <= 1`, value `+1`.
    Outbound,
}

impl Trit {
    /// Classify a validated progress value.
    pub fn classify(progress: f64) -> Result<Self> {
        let s = signed(progress)?;
        Ok(Self::from_signed(s))
    }

    /// Classify an already-signed progress value.
    pub fn from_signed(s: f64) -> Self {
        if s < 0.0 {
            Self::Inbound
        } else if s > 0.0 {
            Self::Outbound
        } else {
            Self::Nexus
        }
    }

    /// Balanced-ternary value.
    pub const fn value(self) -> i8 {
        match self {
            Self::Inbound => -1,
            Self::Nexus => 0,
            Self::Outbound => 1,
        }
    }

    /// Stable lowercase label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Inbound => "inbound",
            Self::Nexus => "nexus",
            Self::Outbound => "outbound",
        }
    }
}

/// Deterministic execution lane: `ordinal mod 3`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Lane(u8);

impl Lane {
    /// Number of lanes.
    pub const COUNT: usize = 3;

    /// All lanes in order.
    pub const ALL: [Self; 3] = [Self(0), Self(1), Self(2)];

    /// Lane for a strand ordinal.
    pub const fn of(ordinal: u32) -> Self {
        Self((ordinal % 3) as u8)
    }

    /// Lane index in `0..3`.
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// Stable label such as `lane-0`.
    pub const fn label(self) -> &'static str {
        match self.0 {
            0 => "lane-0",
            1 => "lane-1",
            _ => "lane-2",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trit_is_exact_at_the_nexus() {
        assert_eq!(Trit::classify(0.0).unwrap(), Trit::Inbound);
        assert_eq!(Trit::classify(0.499_999_999).unwrap(), Trit::Inbound);
        assert_eq!(Trit::classify(0.5).unwrap(), Trit::Nexus);
        assert_eq!(Trit::classify(0.500_000_001).unwrap(), Trit::Outbound);
        assert_eq!(Trit::classify(1.0).unwrap(), Trit::Outbound);
        assert!(Trit::classify(-0.1).is_err());
    }

    #[test]
    fn lanes_cycle() {
        assert_eq!(Lane::of(0), Lane::ALL[0]);
        assert_eq!(Lane::of(1), Lane::ALL[1]);
        assert_eq!(Lane::of(2), Lane::ALL[2]);
        assert_eq!(Lane::of(3), Lane::ALL[0]);
        assert_eq!(Lane::of(u32::MAX).index(), (u32::MAX % 3) as usize);
    }
}
