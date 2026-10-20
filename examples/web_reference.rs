// SPDX-License-Identifier: MPL-2.0
//! Emit browser parity data directly from the Rust reference (no dependencies).
use nexus::ve::{
    graph::Cuboctahedron,
    lattice::{Lattice, DEFAULT_SEED},
};
fn main() {
    let g = Cuboctahedron::derive();
    println!(
        "{{\"vertices\":{:?},\"edges\":{:?},\"triangles\":{:?},\"matching\":{:?},\"controlFace\":{:?},\"cases\":[",
        g.vertices,
        g.edges.map(|(a, b)| [a, b]),
        g.triangles,
        g.matching,
        g.control_face
    );
    let mut separator = "";
    for seed in [0, 7, DEFAULT_SEED, u32::MAX] {
        for cells in [2, 6] {
            let mut lattice = Lattice::seeded(cells, seed).unwrap();
            for round in 0..=128 {
                if round != 0 {
                    lattice.step_forward(round - 1);
                }
                if round <= 16 || [32, 64, 128].contains(&round) {
                    println!(
                        "{separator}{{\"seed\":{seed},\"cells\":{cells},\"round\":{round},\"registers\":{:?}}}",
                        lattice.registers()
                    );
                    separator = ",";
                }
            }
        }
    }
    println!("]}}");
}
