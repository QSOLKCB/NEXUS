# Twelve-direction atlas

The cuboctahedron's twelve vertices form three great-square families, each
with four signed states:

| Family | Index | Kept components |
|---|---:|---|
| `xy` | 0 | `(x, y)` |
| `xz` | 1 | `(x, eversion)` |
| `yz` | 2 | `(y, eversion)` |

## Classification

```text
v = (x / R, y / R, eversion)
drop the component with the smallest magnitude (ties -> lowest index)
family   = index of the dropped component mapped as above
quadrant = (first < 0) | (second < 0) << 1
```

The exact nexus (`v = 0`) is **not** classified and is counted separately.

## Walk on `Z_3 x Z_4`

Per round, with `lane = ordinal mod 3`:

```text
quadrant += +1 (outbound) or +3 (inbound)                  mod 4
family   += (lane + 1) mod 3 (outbound) or its negation      mod 3
```

Inbound and outbound samples of the same lane move in opposite senses, so
`walk(walk(d, out, r), in, r) = d` (tested for every lane, state, and several
round counts).

## Census

`nexus atlas <samples> --rounds <r>` reports twelve-bin initial and final
histograms, family and quadrant occupancy, the nexus count, and the
coherence count (initial family equals lane index). Coefficients of variation
are supplied for balance. The expected coherence ratio for a balanced field is
about one third; it is a classification statistic, not a conservation law.
