// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Exactly reversible coupled lattice over `Z_4` registers.
//!
//! Each cell holds one `Z_4` register per cuboctahedron vertex. A round
//! applies six collision-free controlled increments along one perfect
//! matching, then swaps the three registers of one triangular face with a
//! neighbouring cell. Increments are invertible by subtraction and swaps are
//! involutions, so running the rounds backwards restores the seeded state
//! exactly. This is the CPU reference; the optional GPU runner mirrors it.

use std::time::{Duration, Instant};

use crate::error::{Error, Result};
use crate::ve::graph::{Cuboctahedron, EDGES, TRIANGLES, VERTICES};
use crate::ve::{coefficient_of_variation, stir};

/// Largest round count accepted.
pub const MAX_ROUNDS: u32 = 1 << 16;
/// Default deterministic seed (`"NEXS"` as ASCII).
pub const DEFAULT_SEED: u32 = 0x4e45_5853;
/// GPU-model resident bytes per cell: twelve `u32` registers in two buffers.
pub const GPU_RESIDENT_BYTES_PER_CELL: u64 = 96;

/// Validate a cell/round request shared by CPU and GPU runners.
pub fn validate_request(cells: u32, rounds: u32) -> Result<()> {
    if cells < 2 {
        return Err(Error::Lattice("cells must be at least two"));
    }
    if cells % 2 != 0 {
        return Err(Error::Lattice("cells must be even so every cell has a partner"));
    }
    if rounds == 0 {
        return Err(Error::Lattice("rounds must be at least one"));
    }
    if rounds > MAX_ROUNDS {
        return Err(Error::Lattice("rounds exceed the supported maximum"));
    }
    Ok(())
}

/// The deterministic choices for one round.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoundPlan {
    /// Active matching colour in `0..4`.
    pub matching: u8,
    /// When set, each edge's control and target endpoints are exchanged.
    pub flip: bool,
    /// Triangular face whose registers are exchanged with the partner cell.
    pub face: u8,
}

/// Plan for a round under a seed.
pub fn plan(seed: u32, round: u32) -> RoundPlan {
    let h = stir(seed.rotate_left(7) ^ round.wrapping_mul(0x2545_f491) ^ 0x734c_4154);
    RoundPlan {
        matching: (h & 3) as u8,
        flip: (h >> 2) & 1 == 1,
        face: ((h >> 3) & 7) as u8,
    }
}

/// Seeded register value for one cell and vertex.
pub fn seed_register(seed: u32, cell: u32, vertex: u32) -> u8 {
    let h = stir(seed ^ 0x6e65_7875)
        .wrapping_add(cell.wrapping_mul(0x9e37_79b1))
        .wrapping_add(vertex.wrapping_mul(0x7f4a_7c15));
    (stir(h) & 3) as u8
}

/// Control word for one cell and face in one round, in `0..3`.
pub fn control(seed: u32, round: u32, cell: u32, face: u32) -> u32 {
    let h = seed
        .wrapping_add(round.wrapping_mul(0x27d4_eb2f))
        .wrapping_add(cell.wrapping_mul(0x1656_67b1))
        .wrapping_add(face.wrapping_mul(0x9e37_79b1));
    stir(h) % 3
}

/// Increment applied to the target register of an edge, in `{1, 2, 3}`.
pub fn increment(control_value: u8, control_word: u32, edge: u32, cell: u32) -> u8 {
    (1 + (u32::from(control_value) + control_word + edge + cell) % 3) as u8
}

/// Partner of a cell in a given round.
pub fn partner(cells: u32, round: u32, cell: u32) -> u32 {
    let offset = round & 1;
    if (cell + offset) % 2 == 0 {
        (cell + 1) % cells
    } else {
        (cell + cells - 1) % cells
    }
}

/// Compact census of a lattice state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Census {
    /// `vertex * 4 + value` occupancy.
    pub vertex_state: [u64; VERTICES * 4],
    /// Aggregate occupancy of each `Z_4` value.
    pub totals: [u64; 4],
    /// Cells whose three face registers do not sum to zero modulo four.
    pub excited: [u64; TRIANGLES],
    /// Edges whose endpoint registers differ.
    pub disagreement: u64,
    /// Order-independent four-word fold of the whole state.
    pub fold: [u32; 4],
}

impl Census {
    /// Structural validity against a cell count.
    pub fn validate(&self, cells: u32) -> Result<()> {
        let cells = u64::from(cells);
        for vertex in 0..VERTICES {
            let total: u64 = self.vertex_state[vertex * 4..vertex * 4 + 4].iter().sum();
            if total != cells {
                return Err(Error::Lattice("vertex histogram does not sum to the cell count"));
            }
        }
        if self.totals.iter().sum::<u64>() != cells * VERTICES as u64 {
            return Err(Error::Lattice("aggregate histogram does not sum to the register count"));
        }
        if self.disagreement > cells * EDGES as u64 {
            return Err(Error::Lattice("edge disagreement exceeds the edge population"));
        }
        if self.excited.iter().any(|&e| e > cells) {
            return Err(Error::Lattice("face excitation exceeds the cell count"));
        }
        Ok(())
    }

    /// Hex rendering of the fold.
    pub fn fold_hex(&self) -> String {
        format!(
            "{:08x}{:08x}{:08x}{:08x}",
            self.fold[0], self.fold[1], self.fold[2], self.fold[3]
        )
    }

    /// Coefficient of variation across the four aggregate totals.
    pub fn total_cv(&self) -> f64 {
        coefficient_of_variation(&self.totals)
    }

    /// Coefficient of variation across the 48 vertex/state bins.
    pub fn vertex_state_cv(&self) -> f64 {
        coefficient_of_variation(&self.vertex_state)
    }

    /// Disagreement as a fraction of all edges.
    pub fn disagreement_ratio(&self, cells: u32) -> f64 {
        self.disagreement as f64 / (u64::from(cells) * EDGES as u64) as f64
    }

    /// Largest absolute difference between two censuses, and the number of
    /// differing fold words.
    pub fn compare(&self, other: &Self) -> (u64, u64, u64) {
        let mut max = 0u64;
        let mut l1 = 0u64;
        let mut tally = |a: u64, b: u64| {
            let d = a.abs_diff(b);
            max = max.max(d);
            l1 += d;
        };
        for (a, b) in self.vertex_state.iter().zip(other.vertex_state) {
            tally(*a, b);
        }
        for (a, b) in self.totals.iter().zip(other.totals) {
            tally(*a, b);
        }
        for (a, b) in self.excited.iter().zip(other.excited) {
            tally(*a, b);
        }
        tally(self.disagreement, other.disagreement);
        let fold_mismatches = self.fold.iter().zip(other.fold).filter(|(a, b)| **a != *b).count() as u64;
        (max, l1, fold_mismatches)
    }
}

/// CPU lattice state.
#[derive(Debug, Clone)]
pub struct Lattice {
    cells: u32,
    seed: u32,
    registers: Vec<u8>,
    graph: Cuboctahedron,
}

impl Lattice {
    /// Seed a lattice.
    pub fn seeded(cells: u32, seed: u32) -> Result<Self> {
        validate_request(cells, 1)?;
        let mut registers = vec![0u8; cells as usize * VERTICES];
        for cell in 0..cells {
            for vertex in 0..VERTICES as u32 {
                registers[cell as usize * VERTICES + vertex as usize] = seed_register(seed, cell, vertex);
            }
        }
        Ok(Self {
            cells,
            seed,
            registers,
            graph: Cuboctahedron::derive(),
        })
    }

    /// Cell count.
    pub const fn cells(&self) -> u32 {
        self.cells
    }

    /// Seed.
    pub const fn seed(&self) -> u32 {
        self.seed
    }

    /// Flat register view, `cells * 12` entries.
    pub fn registers(&self) -> &[u8] {
        &self.registers
    }

    /// The derived graph.
    pub const fn graph(&self) -> &Cuboctahedron {
        &self.graph
    }

    fn local(&mut self, round: u32, inverse: bool) {
        let plan = plan(self.seed, round);
        let edges: Vec<usize> = self.graph.matching_edges(plan.matching).collect();
        for cell in 0..self.cells {
            let base = cell as usize * VERTICES;
            for &edge in &edges {
                let (a, b) = self.graph.edges[edge];
                let (ctrl, tgt) = if plan.flip { (b, a) } else { (a, b) };
                let control_value = self.registers[base + ctrl as usize];
                let word = control(self.seed, round, cell, u32::from(self.graph.control_face[edge]));
                let delta = increment(control_value, word, edge as u32, cell);
                let slot = &mut self.registers[base + tgt as usize];
                *slot = if inverse {
                    (*slot + 4 - delta) & 3
                } else {
                    (*slot + delta) & 3
                };
            }
        }
    }

    fn couple(&mut self, round: u32) {
        let plan = plan(self.seed, round);
        let face = self.graph.triangles[plan.face as usize];
        let offset = round & 1;
        for pair in 0..self.cells / 2 {
            let left = (2 * pair + offset) % self.cells;
            let right = (left + 1) % self.cells;
            let (lb, rb) = (left as usize * VERTICES, right as usize * VERTICES);
            for vertex in face {
                self.registers.swap(lb + vertex as usize, rb + vertex as usize);
            }
        }
    }

    /// Apply one forward round.
    pub fn step_forward(&mut self, round: u32) {
        self.local(round, false);
        self.couple(round);
    }

    /// Undo one round.
    pub fn step_inverse(&mut self, round: u32) {
        self.couple(round);
        self.local(round, true);
    }

    /// Apply rounds `0..rounds` forward.
    pub fn forward(&mut self, rounds: u32) {
        for round in 0..rounds {
            self.step_forward(round);
        }
    }

    /// Undo rounds `rounds-1..=0`.
    pub fn inverse(&mut self, rounds: u32) {
        for round in (0..rounds).rev() {
            self.step_inverse(round);
        }
    }

    /// Count registers that differ from the seeded state.
    pub fn mismatches_from_seed(&self) -> u64 {
        let mut mismatches = 0;
        for cell in 0..self.cells {
            for vertex in 0..VERTICES as u32 {
                if self.registers[cell as usize * VERTICES + vertex as usize] != seed_register(self.seed, cell, vertex)
                {
                    mismatches += 1;
                }
            }
        }
        mismatches
    }

    /// Take a census of the current state.
    pub fn census(&self) -> Census {
        let mut census = Census {
            vertex_state: [0; VERTICES * 4],
            totals: [0; 4],
            excited: [0; TRIANGLES],
            disagreement: 0,
            fold: [0; 4],
        };
        for cell in 0..self.cells {
            let base = cell as usize * VERTICES;
            let regs = &self.registers[base..base + VERTICES];
            let mut word = 0u32;
            for (vertex, &value) in regs.iter().enumerate() {
                census.vertex_state[vertex * 4 + value as usize] += 1;
                census.totals[value as usize] += 1;
                word |= u32::from(value) << (2 * vertex);
            }
            for (face, tri) in self.graph.triangles.iter().enumerate() {
                let sum = tri.iter().map(|&v| u32::from(regs[v as usize])).sum::<u32>() & 3;
                if sum != 0 {
                    census.excited[face] += 1;
                }
            }
            for &(a, b) in &self.graph.edges {
                if regs[a as usize] != regs[b as usize] {
                    census.disagreement += 1;
                }
            }
            let lane = (cell & 3) as usize;
            census.fold[lane] = census.fold[lane].wrapping_add(stir(word ^ stir(cell)));
        }
        census
    }
}

/// Model-defined work counts for a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Workload {
    /// Six controlled increments per cell per round.
    pub edge_gates: u64,
    /// Three register swaps per cell pair per round.
    pub face_swaps: u64,
    /// `24 * cells` bits of classical configuration space.
    pub configuration_bits: u64,
    /// Bytes two `u32` register buffers would occupy on a GPU.
    pub gpu_resident_bytes: u64,
    /// Bytes the CPU reference holds.
    pub cpu_state_bytes: u64,
}

impl Workload {
    /// Compute the workload for a request.
    pub fn of(cells: u32, rounds: u32) -> Self {
        let cells = u64::from(cells);
        let rounds = u64::from(rounds);
        Self {
            edge_gates: 6 * cells * rounds,
            face_swaps: 3 * (cells / 2) * rounds,
            configuration_bits: 24 * cells,
            gpu_resident_bytes: GPU_RESIDENT_BYTES_PER_CELL * cells,
            cpu_state_bytes: VERTICES as u64 * cells,
        }
    }
}

/// Outcome of a CPU verification.
#[derive(Debug, Clone)]
pub struct Verification {
    /// Census before any round.
    pub initial: Census,
    /// Census after the forward schedule.
    pub final_state: Census,
    /// Registers not restored by the inverse schedule.
    pub restoration_mismatches: u64,
    /// Wall time for forward, census, and inverse.
    pub elapsed: Duration,
    /// Workload counts.
    pub workload: Workload,
}

impl Verification {
    /// True when the lattice evolved and was restored exactly.
    pub fn passed(&self) -> bool {
        self.restoration_mismatches == 0 && self.initial.fold != self.final_state.fold
    }
}

/// Run forward then inverse on the CPU, verifying exact restoration.
pub fn verify(cells: u32, rounds: u32, seed: u32) -> Result<Verification> {
    validate_request(cells, rounds)?;
    let mut lattice = Lattice::seeded(cells, seed)?;
    let start = Instant::now();
    let initial = lattice.census();
    lattice.forward(rounds);
    let final_state = lattice.census();
    lattice.inverse(rounds);
    let restoration_mismatches = lattice.mismatches_from_seed();
    Ok(Verification {
        initial,
        final_state,
        restoration_mismatches,
        elapsed: start.elapsed(),
        workload: Workload::of(cells, rounds),
    })
}

/// Outcome of a forward-only CPU run.
#[derive(Debug, Clone)]
pub struct Run {
    /// Census before any round.
    pub initial: Census,
    /// Census after the forward schedule.
    pub final_state: Census,
    /// Wall time for the forward schedule only.
    pub elapsed: Duration,
    /// Workload counts.
    pub workload: Workload,
}

/// Run the forward schedule on the CPU and census both ends.
pub fn run(cells: u32, rounds: u32, seed: u32) -> Result<Run> {
    validate_request(cells, rounds)?;
    let mut lattice = Lattice::seeded(cells, seed)?;
    let initial = lattice.census();
    let start = Instant::now();
    lattice.forward(rounds);
    let elapsed = start.elapsed();
    let final_state = lattice.census();
    Ok(Run {
        initial,
        final_state,
        elapsed,
        workload: Workload::of(cells, rounds),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_then_inverse_restores_every_register() {
        for (cells, rounds) in [(2_u32, 1_u32), (4, 3), (64, 16), (1_024, 16), (6, 257)] {
            let v = verify(cells, rounds, DEFAULT_SEED).unwrap();
            assert_eq!(v.restoration_mismatches, 0, "cells={cells} rounds={rounds}");
            assert!(v.passed());
            v.initial.validate(cells).unwrap();
            v.final_state.validate(cells).unwrap();
        }
    }

    #[test]
    fn single_round_is_a_bijection_on_small_lattice() {
        // Exhaustively check injectivity of one round on two cells by
        // sampling many seeds: distinct seeded states must map to distinct
        // outputs under the same plan.
        let mut outputs = std::collections::HashSet::new();
        for seed in 0..2_000u32 {
            let mut lattice = Lattice::seeded(2, seed).unwrap();
            // Fix the round plan across seeds by overriding the seed used for
            // the plan: use the same schedule (seed 7) but different states.
            lattice.seed = 7;
            lattice.registers = (0..24)
                .map(|v| seed_register(seed, (v / 12) as u32, (v % 12) as u32))
                .collect();
            let before = lattice.registers.clone();
            lattice.step_forward(0);
            let after = lattice.registers.clone();
            let inserted = outputs.insert(after.clone());
            // Two different inputs may coincide only if the inputs coincide.
            if !inserted {
                let mut other = Lattice::seeded(2, 0).unwrap();
                other.seed = 7;
                other.registers = after;
                other.step_inverse(0);
                assert_eq!(other.registers, before);
            }
        }
    }

    #[test]
    fn plan_and_control_are_deterministic_and_bounded() {
        for round in 0..1_000 {
            let p = plan(DEFAULT_SEED, round);
            assert!(p.matching < 4 && p.face < 8);
            assert_eq!(p, plan(DEFAULT_SEED, round));
            assert!(control(DEFAULT_SEED, round, 5, 3) < 3);
        }
        let used: std::collections::HashSet<u8> = (0..64).map(|r| plan(DEFAULT_SEED, r).matching).collect();
        assert_eq!(used.len(), 4, "all matchings appear in 64 rounds");
    }

    #[test]
    fn partners_are_symmetric() {
        for cells in [2_u32, 8, 10] {
            for round in 0..4 {
                for cell in 0..cells {
                    let p = partner(cells, round, cell);
                    assert_eq!(partner(cells, round, p), cell);
                    assert_ne!(p, cell);
                }
            }
        }
    }

    #[test]
    fn workload_matches_model_formulas() {
        let w = Workload::of(1 << 20, 128);
        assert_eq!(w.edge_gates, 805_306_368);
        assert_eq!(w.face_swaps, 201_326_592);
        assert_eq!(w.gpu_resident_bytes, 100_663_296);
    }

    #[test]
    fn evolution_is_nontrivial() {
        let r = run(256, 8, DEFAULT_SEED).unwrap();
        assert_ne!(r.initial, r.final_state);
        let (max, _, fold_mismatches) = r.initial.compare(&r.final_state);
        assert!(max > 0 || fold_mismatches > 0);
    }

    #[test]
    fn requests_are_validated() {
        assert!(validate_request(3, 1).is_err());
        assert!(validate_request(0, 1).is_err());
        assert!(validate_request(2, 0).is_err());
        assert!(validate_request(2, MAX_ROUNDS + 1).is_err());
        assert!(validate_request(2, MAX_ROUNDS).is_ok());
    }
}
