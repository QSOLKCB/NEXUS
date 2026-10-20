# Reversible cuboctahedral lattice

## Structure derived, not typed

`ve::graph::Cuboctahedron::derive()` builds the structure from coordinates:

- vertices: the twelve points in `{-1, 0, 1}^3` with exactly one zero,
  sorted lexicographically;
- edges: the 24 vertex pairs at squared distance 2;
- triangles: the 8 three-cliques;
- control face: the unique triangle containing each edge;
- matching: a proper 4-edge-colouring found by exhaustive search, giving four
  perfect matchings of six edges each.

Tests check the counts, the degree-4 regularity, the perfect-matching
property, and that each edge lies in exactly one triangle. The GPU mirror
uploads these same tables rather than embedding its own.

## State

`N` cells (even, `>= 2`), each with twelve `Z_4` registers. Seeding:

```text
x[c][v] = stir(stir(seed ^ 0x6e657875) + c*0x9e3779b1 + v*0x7f4a7c15) & 3
```

`stir` is the 32-bit mixer in `ve::stir` (xor-shift 16, multiply
`0x21f0aaad`, xor-shift 15, multiply `0x735a2d97`, xor-shift 15).

## Round plan

```text
h        = stir(rotl(seed, 7) ^ round*0x2545f491 ^ 0x734c4154)
matching = h & 3
flip     = (h >> 2) & 1
face     = (h >> 3) & 7
```

## Local operator

For every edge `e = (a, b)` in the active matching, control is `a` and target
is `b` unless `flip` exchanges them. With

```text
w     = stir(seed + round*0x27d4eb2f + cell*0x165667b1 + control_face(e)*0x9e3779b1) mod 3
delta = 1 + (x[control] + w + e + cell) mod 3           // in {1, 2, 3}
```

the forward update is `x[target] += delta (mod 4)` and the inverse is
`x[target] -= delta (mod 4)`. Because the matching is perfect, the six
targets are distinct and no control is also a target, so `delta` is available
unchanged during inversion. The local operator is a bijection on `Z_4^12`.

## Coupling

Cells pair as `(0,1), (2,3), ...` on even rounds and `(1,2), (3,4), ..., (N-1,0)`
on odd rounds. The three registers of the planned face are swapped between
partners. A swap is an involution.

## Round and inverse

```text
T_r      = couple_r o local_r
T_r^{-1} = local_r^{-1} o couple_r
Phi_R    = T_{R-1} o ... o T_0
```

`verify` runs `Phi_R`, then `Phi_R^{-1}`, and counts registers that differ
from the seed. The count must be zero and the fold must have changed.

## Census (65 words)

| Words | Content |
|---|---|
| 0..48 | `vertex * 4 + value` occupancy |
| 48..52 | aggregate `Z_4` totals |
| 52..60 | cells whose face registers do not sum to 0 mod 4 |
| 60 | edges whose endpoints differ |
| 61..65 | fold: `fold[cell & 3] += stir(packed_registers ^ stir(cell))` (mod 2^32) |

The fold is order-independent so GPU atomics reproduce it exactly. It is
regression evidence, not a cryptographic digest.

## Workload formulas

```text
edge_gates          = 6 * cells * rounds
face_swaps          = 3 * (cells / 2) * rounds
configuration_bits  = 24 * cells
gpu_resident_bytes  = 96 * cells
```

## Boundary

A deterministic classical state machine. `quantum_claim,false` is emitted on
purpose.
