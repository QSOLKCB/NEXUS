// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Cuboctahedral (Vector Equilibrium) combinatorics and the two classical
//! engines built on them: a twelve-direction state atlas and an exactly
//! reversible coupled lattice over `Z_4` registers.
//!
//! Everything here is deterministic classical computation. Nothing claims
//! quantum hardware, a Hilbert space, or a physical lattice.

pub mod atlas;
pub mod graph;
pub mod lattice;

/// 32-bit integer mixer shared by the lattice and its GPU mirror.
///
/// This is a bijection on `u32` built from xor-shifts and odd multipliers.
pub const fn stir(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x21f0_aaad);
    x ^= x >> 15;
    x = x.wrapping_mul(0x735a_2d97);
    x ^= x >> 15;
    x
}

/// Coefficient of variation of a histogram, zero for perfectly even bins.
pub fn coefficient_of_variation(bins: &[u64]) -> f64 {
    if bins.is_empty() {
        return 0.0;
    }
    let n = bins.len() as f64;
    let mean = bins.iter().map(|&b| b as f64).sum::<f64>() / n;
    if mean == 0.0 {
        return 0.0;
    }
    let variance = bins.iter().map(|&b| (b as f64 - mean).powi(2)).sum::<f64>() / n;
    variance.sqrt() / mean
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stir_is_injective_on_a_sample() {
        let mut seen = std::collections::HashSet::new();
        for x in 0..100_000_u32 {
            assert!(seen.insert(stir(x)));
        }
        assert_ne!(stir(1), 1);
    }

    #[test]
    fn cv_is_zero_for_flat_histograms() {
        assert_eq!(coefficient_of_variation(&[5, 5, 5, 5]), 0.0);
        assert!(coefficient_of_variation(&[1, 9]) > 0.5);
        assert_eq!(coefficient_of_variation(&[]), 0.0);
    }
}
