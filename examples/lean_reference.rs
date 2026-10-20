// SPDX-License-Identifier: MPL-2.0
//! Live Rust oracle for the separate Lean specification. No fixtures are trusted.
use nexus::{
    exec::Sample,
    field::{golden_logical, uniform_logical, FieldSpec, Ordering},
    ledger,
    ternary::{Lane, Trit},
    transit::Pos,
    ve::{
        atlas::{walk, Direction},
        graph::Cuboctahedron,
        lattice::{self, Lattice},
    },
};
fn emit(tag: &str, values: &[u64]) {
    print!("{tag}");
    for value in values {
        print!(",{value}");
    }
    println!();
}
fn main() {
    let g = Cuboctahedron::derive();
    for (v, p) in g.vertices.iter().enumerate() {
        println!("vertex,{v},{},{},{}", p[0], p[1], p[2]);
    }
    for (e, &(a, b)) in g.edges.iter().enumerate() {
        emit(
            "edge",
            &[
                e as u64,
                a.into(),
                b.into(),
                g.matching[e].into(),
                g.control_face[e].into(),
            ],
        );
    }
    for (f, t) in g.triangles.iter().enumerate() {
        emit("triangle", &[f as u64, t[0].into(), t[1].into(), t[2].into()]);
    }
    for l in [1, 10, 97, 1_000_003, 1u64 << 32, u64::MAX] {
        for n in [1u32, 3, 97, 777] {
            if u64::from(n) <= l {
                let field = FieldSpec {
                    cardinality: l,
                    strands: n,
                    ..FieldSpec::default()
                }
                .build()
                .unwrap();
                for i in [0, n / 2, n - 1] {
                    emit(
                        "field",
                        &[
                            l,
                            n.into(),
                            i.into(),
                            uniform_logical(l, n, i),
                            golden_logical(l, i),
                            field.iter(Ordering::Uniform).nth(i as usize).unwrap().logical,
                        ],
                    );
                }
            }
        }
    }
    for lane in Lane::ALL {
        for family in 0u8..3 {
            for quadrant in 0u8..4 {
                for out in [false, true] {
                    for r in [0, 1, 5, 12, 16, 128, 65536, u32::MAX] {
                        let d = walk(
                            Direction { family, quadrant },
                            lane,
                            if out { Trit::Outbound } else { Trit::Inbound },
                            r,
                        );
                        emit(
                            "atlas",
                            &[
                                family.into(),
                                quadrant.into(),
                                lane.index() as u64,
                                u64::from(out),
                                r.into(),
                                d.family.into(),
                                d.quadrant.into(),
                            ],
                        );
                    }
                }
            }
        }
    }
    for n in [2, 6, 10, u32::MAX - 1] {
        for r in [0, 1, 2, 65535] {
            for c in [0, 1, n - 2, n - 1] {
                emit(
                    "partner",
                    &[n.into(), r.into(), c.into(), lattice::partner(n, r, c).into()],
                );
            }
        }
    }
    for seed in [0, 7, lattice::DEFAULT_SEED, u32::MAX] {
        for r in [0, 1, 7, 128, 65535, u32::MAX] {
            let p = lattice::plan(seed, r);
            emit(
                "plan",
                &[
                    seed.into(),
                    r.into(),
                    p.matching.into(),
                    u64::from(p.flip),
                    p.face.into(),
                ],
            );
            for c in [0, 1, u32::MAX - 1] {
                for f in [0, 7] {
                    emit(
                        "control",
                        &[
                            seed.into(),
                            r.into(),
                            c.into(),
                            f.into(),
                            lattice::control(seed, r, c, f).into(),
                        ],
                    );
                }
            }
        }
        for c in [0, 1, u32::MAX - 1] {
            for v in 0..12 {
                emit(
                    "seed",
                    &[
                        seed.into(),
                        c.into(),
                        v.into(),
                        lattice::seed_register(seed, c, v).into(),
                    ],
                );
            }
        }
        for n in [2, 6, 10] {
            let mut s = Lattice::seeded(n, seed).unwrap();
            for r in 0..129 {
                if [0, 1, 2, 3, 7, 16, 32, 64, 128].contains(&r) {
                    let mut row = vec![seed.into(), n.into(), r.into()];
                    row.extend(s.registers().iter().map(|&v| u64::from(v)));
                    emit("state", &row);
                    let mut restored = s.clone();
                    restored.inverse(r);
                    let mut row = vec![seed.into(), n.into(), r.into()];
                    row.extend(restored.registers().iter().map(|&v| u64::from(v)));
                    emit("restored", &row);
                }
                s.step_forward(r);
            }
        }
    }
    for a in 0u8..4 {
        for w in [0, 1, 2, u32::MAX] {
            for e in [0, 23] {
                for c in [0, u32::MAX - 1] {
                    emit(
                        "increment",
                        &[
                            a.into(),
                            w.into(),
                            e.into(),
                            c.into(),
                            lattice::increment(a, w, e, c).into(),
                        ],
                    );
                }
            }
        }
    }
    for bits in [
        0,
        1,
        0x8000_0000_0000_0000,
        0x3fe0_0000_0000_0000,
        0x7ff0_0000_0000_0000,
        0x7ff8_0000_0000_0001,
        u64::MAX,
    ] {
        for trit in [Trit::Inbound, Trit::Nexus, Trit::Outbound] {
            for lane in Lane::ALL {
                for ordering in [Ordering::Uniform, Ordering::Golden] {
                    let s = Sample {
                        ordinal: u32::MAX,
                        logical: u64::MAX,
                        progress: f64::from_bits(bits),
                        pos: Pos {
                            x: f64::from_bits(bits),
                            y: f64::from_bits(bits),
                        },
                        trit,
                        lane,
                        ordering,
                        ..Sample::default()
                    };
                    let mut row = vec![
                        bits,
                        u64::from(trit.value() as u8),
                        lane.index() as u64,
                        u64::from(ordering.code()),
                    ];
                    row.extend(ledger::encode(&s).map(u64::from));
                    emit("record", &row);
                }
            }
        }
    }
}
