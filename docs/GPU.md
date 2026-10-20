# GPU mirror (`--features gpu`)

`nexus-lattice-gpu` runs the reversible lattice on Vulkan through `wgpu` and
WGSL. It is a mirror of `ve::lattice`, never a replacement: verification
compares its census exactly with the CPU reference.

## Mapping

| Pass | Dispatch | Work |
|---|---|---|
| `seed_state` | one invocation per register | writes the seeded `Z_4` values |
| `local_round` | one 32-invocation workgroup per cell | lanes 0..23 are edges, lanes 24..31 compute face controls; six active edges apply collision-free increments through workgroup memory |
| `couple_round` | one invocation per register | copies, or swaps with the partner cell, depending on face membership |
| `census_pass` | one workgroup per cell | atomics into a 65-word region |
| `compare_pass` | one invocation per register | counts registers that differ from the seed |

Requests are also bounded by `cells <= floor(u32::MAX / 24)` (and must be
even), so shader census counts and register indices cannot wrap. Actual
adapter buffer limits can impose a smaller bound.

Two storage buffers hold the state (96 bytes per cell). Rounds alternate the
roles of the buffers. Per-round parameters live in one uniform buffer with a
dynamic offset per slot, so a complete forward (and inverse) schedule is
recorded in a single command buffer. Dispatch grids use an `x * y` layout to
exceed 65 535 workgroups.

The cuboctahedron tables are **uploaded from the CPU derivation**
(`gpu::table_words`), so the shader contains no vertex, edge, matching, or
face numbers of its own.

## Verification contract

`nexus-lattice-gpu verify` passes only when:

1. two GPU executions produce identical initial and final censuses and the
   same mismatch count;
2. both censuses are structurally valid;
3. GPU inverse replay restores every register;
4. the CPU reference restores every register;
5. initial and final CPU/GPU census deltas are zero;
6. all four initial and final fold words match.

## Readback

`run` returns two 65-word regions (520 bytes) regardless of cell count.

## Testing without a GPU

`cargo test --features gpu` parses and validates the WGSL with naga and
checks the table layout, grid, census decoding, and parameter encoding. The
device path itself needs a Vulkan adapter.
