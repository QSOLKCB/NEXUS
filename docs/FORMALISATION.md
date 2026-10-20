# Lean formalisation

This proof layer is based on **NEXUS v1.0.0**, immutable release commit
`24c7b5bb8b158d84e08b10ccc8fdd16b6889b7e4`. It adds specifications,
proofs and conformance checks without changing the released runtime source.
The proof package has its own version, `0.1.0`.

## What is established

The strongest result is `Nexus.Lattice.rewind_evolve`: for **every even cell
count at least two, every valid register state, every sequence of round
plans, and every natural-number round count**, applying the forward schedule
and its inverse returns the original state. The converse direction and
injectivity are also proved. This quantifies over all states, independently
of the finite seed samples used by regression tests.

Registers are `Fin 4`, vertices are `Fin 12`, and states are functions from
cells and vertices to registers. A delta may be any function of the unchanged
control register. The local inverse therefore covers the wrapping Rust
increment without assuming that the mixer is injective or statistically random.
The coupled face swap uses a proved fixed-point-free involution on even rings.

`Nexus.Runtime.seeded_restoration` specialises that result to the executable
UInt32 seed and round-plan specification. Vector snapshots are related to the
functional state by a separate equality theorem; the conformance program uses
those snapshots to avoid repeated evaluation of earlier states.

## Coverage map

The complete list of **71 named theorems** is in
[`formal/theorems.json`](../formal/theorems.json). Lean also generates auxiliary
theorems, which the axiom audit inspects alongside the named results.

| Released source | Lean module and representative results | Boundary |
|---|---|---|
| `src/ve/graph.rs` | `Graph.vertices_derived`, `adjacency_exact`, `triangles_exact`, `degree_four`, `matching_incident_unique`, `unique_control_face` | The finite graph and matching certificate are proved correct. The Rust search implementation is checked against this certificate by live comparison. |
| `src/ve/lattice.rs` | `Lattice.partner_involution`, `local_inverse_forward`, `inverse_forward`, `rewind_evolve`, `evolve_rewind`, `evolution_injective` | Universal proofs of the functional discrete model. No verified translation of Rust mutation, allocation or indexing is supplied. |
| `src/ve/mod.rs`, lattice seed/control/plan functions | `Runtime.seeded_restoration`, `run_eq_evolve`; executable UInt32 specification | Exact wrapping operations are exercised by parity. Mixer bijectivity and randomness claims are outside the proof set. |
| `src/field.rs` | `Field.uniform_lt`, `golden_lt`, `cursor_exact`, `uniform_strict_step`, `product_fits_u128`, cursor bounds | Natural-number arithmetic contracts, including the same carry branch. Floating-point bearing construction is outside scope. Golden range is proved; uniqueness of golden selections is not asserted. |
| `src/ve/atlas.rs` | `Atlas.index_lt`, `walk_inverse`, `period_twelve` | Reversal for every valid direction, lane and round count. Floating-point classification, tie handling and census validation remain Rust-tested. The executable atlas model represents inbound/outbound walks; exact-nexus samples are excluded by the Rust caller. |
| `src/transit.rs`, `src/ternary.rs` | `Contracts.signed_centre`, `signed_range`, `nexus_exact`, `endpoints_antipodal`, `lane_bound`, `lane_period` | Exact integer algebra for a scaled progress numerator and arbitrary direction components. This does not prove trigonometry, IEEE-754 arithmetic or signed-zero handling. |
| `src/exec.rs` | `Contracts.ordered_blocks`, `workers_bound` | Successful block mapping preserves input order; worker counts are bounded. Thread scheduling, mutexes, panic/error propagation and platform branches are outside the model. |
| `src/ledger.rs` | `Contracts.littleEndian_roundtrip`, `littleEndian_bytes`, `record_length`, `lane_count_conservation`, `encoded_bytes_safe` | Fixed layout and reversible encoding of bounded words. Floating-point fields are opaque 64-bit patterns. Digest equality alone is not a proof of message equality. |
| `src/sonify.rs`, `src/audit.rs` | `Contracts.gain_envelope`, `audit_max_assoc`, `audit_max_comm` | Ideal fixed-denominator gain bounds and finite nonnegative maximum reduction. Floating-point validation, exponential pitch calculation and perceptual/audio claims are outside scope. |
| `src/gpu/mod.rs`, `src/gpu/lattice.wgsl` | `Contracts.gpu_census_fits`, `gpu_readback_sizes`, `workload_capacity` | Capacity and sizing arithmetic. Shader execution, dispatch, GPU atomics and driver correctness require separate hardware validation. |
| `site/model.mjs`, other web files | Existing Rust/browser parity plus the new Rust/Lean comparison | No direct JavaScript refinement proof or proof of WebGL rendering is claimed. |
| `src/sha256.rs`, CLI/error modules, evidence scripts, workflows | Existing tests and source review | SHA-256 implementation/security, parsers, filesystem/process behaviour and workflow delivery have no end-to-end Lean proof here. |

The source lock and comparison tests provide traceability and finite conformance
evidence. A complete Rust-to-Lean semantic refinement proof remains a separate
obligation. The formalisation concerns classical software contracts; it adds no
claims about quantum hardware or physical geometry.

## Reproduce

Install Rust and [Elan](https://lean-lang.org/install/manual/) using the official
instructions, clone the repository with its release tags, then run:

```sh
python3 -I scripts/check_formal.py
```

The command checks the pinned release and its 25 runtime files, checks the named
theorem inventory, builds the proofs and executable, audits axiom dependencies,
and compares freshly executed Lean and Rust outputs. It rejects failed commands,
timeouts, incomplete output, mismatches and failed inverse restoration. Four
negative checks deliberately exercise empty/truncated output, a changed register
and matching outputs that fail to restore the seed. Local mutation checks also
confirmed rejection of an injected custom axiom and a changed runtime source.

For proofs alone:

```sh
cd formal
lake build
lake env lean -DwarningAsError=true Audit.lean
```

Lean is pinned to **4.19.0** in `formal/lean-toolchain`. There are no Mathlib or
other Lake package dependencies. Proofs use ordinary kernel-checked terms and
finite `decide` calculations. No `sorry`, `admit`, custom axioms or
`native_decide` occur in the proof sources. The audit permits only Lean's standard
foundational axioms `propext`, `Quot.sound` and `Classical.choice`; any other
axiom in a theorem's dependency closure fails the check.

The CI job **Lean proofs and Rust parity** runs on pull requests and pushes to
`main`. The normal Rust/browser/evidence checks continue to run separately.
A source change intentionally fails the release binding until its relationship
to the formal model is reviewed and the binding is explicitly revised.

## Conformance evidence

Both programs calculate their own results at check time. They do not load a
shared table of expected output or reuse a saved run.

| Comparison | Rows |
|---|---:|
| Vertex, edge/matching/control-face and triangle data | 44 |
| Uniform, golden and carry-cursor selection | 54 |
| Atlas walks | 576 |
| Ring partners, including near-u32-limit cells | 64 |
| Round plans, control words and seed registers | 312 |
| Complete lattice states: four seeds, three even cell counts, nine checkpoints | 108 |
| Inverse-restored complete lattice states | 108 |
| Wrapping increments, including overflow cases | 64 |
| Canonical records, including signed zero, infinities and NaN bit patterns | 126 |
| **Total** | **1,456** |

The state runs use cells `2, 6, 10`, seeds `0, 7, 0x4e455853, 0xffffffff`, and
checkpoints `0, 1, 2, 3, 7, 16, 32, 64, 128`. Ledger tests construct raw samples
solely to check bit preservation; those inputs are not claims of valid geometry.

## Verification environment

The local proof build and comparisons passed with Lean 4.19.0. The hosted CI
job uses the unmodified official toolchain on Ubuntu. Locally, the execution
environment exposes `/proc/self/exe` but lacks the corresponding numeric PID
path expected by Lean. A temporary path-lookup compatibility shim was needed
for the local toolchain launcher; it is not part of this repository, and it
changes no proof terms or kernel logic. Hosted CI is the independent check
without that shim.
