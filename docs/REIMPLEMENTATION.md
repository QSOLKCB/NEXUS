# Why this is a distinct implementation

This crate was written from a mathematical contract, not ported from an
earlier code base. The table lists the choices that make its code, data, and
outputs independent of any previous runner for the same ideas.

| Concern | This implementation |
|---|---|
| Transit formula | One signed-progress expression `P(s) = s R (cos(theta + psi), sin(theta + psi))` with `psi = -sign(s) 2 pi t (1 - abs(s))`. No branch on the half of the path; the centre is zero because the radial factor is zero. |
| Eversion | The signed progress itself, `2p - 1`. |
| Uniform partition | Incremental carry cursor (`L div N` plus accumulated `L mod N`) for iteration; `u128` only for random access. |
| Golden ordering | 64-bit fixed-point Kronecker step `0x9E3779B97F4A7C15`; integer-only, no `f64` fract. |
| Parallelism | Mutex-protected queue of input blocks paired with disjoint output slices. An atomic counter verifies completion. |
| Canonical record | 40 bytes, little-endian, tag `0x4E`, trit/lane/ordering bytes first. |
| Ledger | Three lane digests plus a stream digest bound by a root with per-lane counts; streaming builder. |
| Sonification | Configurable `EventMap` tables; the 432 Hz map is the default, not hard-wired. |
| Audit | Shard-and-merge partials that carry boundary gaps, so parallel equals sequential exactly. |
| Cuboctahedron | Vertices, edges, triangles, control faces, and the one-factorisation are **derived from coordinates by search** at start-up and uploaded to the GPU. No hand-typed tables. |
| Lattice mixer | `stir`: xor-shift 16, `0x21f0aaad`, xor-shift 15, `0x735a2d97`, xor-shift 15. Different plan, seed, and control derivations. |
| Lattice fold | Additive (`wrapping_add`) per `cell & 3`, order-independent for GPU atomics. |
| Census layout | 65 words: 48 + 4 + 8 + 1 + 4. |
| CLI | `audit`, `trace`, `events`, `ledger`, `bench`, `bench-exec`, `sweep`, `atlas`, `lattice-verify`, `lattice-run`; `--json` reports. |
| Vocabulary | field, strand, ingress, egress, nexus, transit, trit, lane, ledger, census, fold. |

Where a quantity is the same because the mathematics forces it (antipodal
endpoints, an exact centre, `ordinal mod 3` lanes, the `-1/0/+1` trit, the
four-perfect-matching structure of the cuboctahedron), the equality is a
property of the model, not shared code.
