# NEXUS

NEXUS is a deterministic, dependency-free Rust engine for **centre-transit
fields** and the evidence pipeline around them. A logical field of arbitrary
cardinality is represented by a bounded set of strands. Each strand runs from
an ingress point through the exact origin (the *nexus*) to its antipodal
egress. Every layer above that geometry is built so that an independent
machine can re-run it and compare bytes, not prose.

## What is in the box

| Layer | Module | Guarantee |
|---|---|---|
| Strand selection | `field` | Exact integer mapping (`floor(iL/N)` via a carry cursor or `u128`) and a 64-bit fixed-point golden ordering. No allocation proportional to `L`. |
| Transit | `transit` | One branch-free signed-progress formula. Centre is literally zero; endpoints are antipodal bit for bit. |
| Ternary metadata | `ternary` | `Trit` (inbound / nexus / outbound) and `Lane` (`ordinal mod 3`). Labels over continuous geometry, nothing more. |
| Execution | `exec` | Scalar, sequential, and order-preserving parallel evaluation on `std` scoped threads with an atomic block cursor. Parallel output equals sequential output exactly. |
| Ledgers | `ledger`, `sha256` | Fixed 40-byte canonical records, three lane digests, a stream digest, and a root that binds them with the counts. Own SHA-256, checked against FIPS vectors. |
| Sonification | `sonify` | Configurable semitone and gain tables; the default is the 432 Hz map. |
| Audit | `audit` | Worst-case invariant deviations, identical on one thread or many. |
| Atlas | `ve::atlas` | Twelve-direction cuboctahedral classification and a reversible `Z_3 x Z_4` walk. |
| Lattice | `ve::lattice` | Exactly reversible coupled `Z_4` lattice on cuboctahedron combinatorics **derived from coordinates at start-up**. CPU reference engine. |
| GPU mirror | `gpu` (feature) | Vulkan/WGSL implementation that uploads the CPU-derived tables and is compared exactly against the CPU reference. |

Zero runtime dependencies in the default build. `#![forbid(unsafe_code)]`
throughout. Declared minimum Rust version 1.82.

## Build and test

```sh
cargo build --release --locked
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo doc --locked --no-deps
```

Optional GPU mirror (needs a current toolchain and a Vulkan driver):

```sh
cargo build --release --locked --features gpu --bin nexus-lattice-gpu
```

## Command line

Metric reports print `metric,value` CSV by default, or a flat JSON object
with `--json`. Tabular commands always print CSV with a header row.

```sh
nexus audit                              # invariant audit of the default field
nexus audit --workers 8                  # same result, sharded over workers
nexus trace 17 1000                      # one strand through the nexus
nexus trace 17 1000 --ordering golden    # golden ordering, same geometry
nexus events 17 1000 --base-hz 432       # sonification controls
nexus ledger 100000                      # lane and root SHA-256 ledger
nexus bench 1000                         # scalar throughput
nexus bench-exec 100 262144 8            # scalar vs batch vs parallel-batch
nexus sweep                              # cardinality x strand grid
nexus atlas 1000000 --rounds 16          # twelve-direction atlas census
nexus lattice-verify 1024 --rounds 16    # CPU forward + inverse restoration
nexus lattice-run 65536 128              # CPU forward benchmark with census
```

Field options accepted everywhere: `--cardinality`, `--strands`, `--tracers`,
`--radius`, `--phase`, `--twist`, `--ordering uniform|golden`.

GPU mirror:

```sh
NEXUS_GPU_ADAPTER="RTX" nexus-lattice-gpu info
nexus-lattice-gpu verify 1024 --rounds 16
nexus-lattice-gpu run 1048576 128
```

## Library

```rust
use nexus::{FieldSpec, Job, Ordering, Pos, Trit};

let field = FieldSpec::default().build()?;
let sample = nexus::sample(&field, Ordering::Uniform, Job { ordinal: 17, progress: 0.5 })?;
assert_eq!(sample.pos, Pos::ORIGIN);
assert_eq!(sample.trit, Trit::Nexus);

let ledger = nexus::canonical_ledger(&field, Ordering::Uniform, 10_000)?;
println!("{}", ledger.root_hex());
# Ok::<(), nexus::Error>(())
```

See `docs/` for the model contract, ledger encoding, lattice algebra, atlas
rules, and the GPU mapping. `protocol/QBRAID.md` is the independent
multicore verification protocol with its harness in `scripts/`.

## Scientific boundary

NEXUS is classical, deterministic software. Trits and lanes are labels over
continuous geometry; they are not a finite-field core and they do not create
threads. Ledgers prove that encoded bytes are identical; they do not prove
physical truth. The golden ordering is an alternative representative
ordering, not a replacement for the uniform partition. Sonification controls
are mappings. The lattice is a reversible state machine inspired by
cuboctahedral symmetry; it asserts no quantum hardware, no Hilbert space, and
no physical substrate. Logical cardinality, strand count, embedding dimension,
and physical dimension are distinct concepts.

## Licence

Mozilla Public License 2.0. See `LICENSE`.
