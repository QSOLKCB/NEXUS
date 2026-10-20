# Model contract

## Field

| Symbol | Name | Default | Constraint |
|---|---|---:|---|
| `L` | cardinality | `2^24` | `L >= 1` |
| `N` | strands | `1024` | `1 <= N <= L` |
| `P` | tracers | `512` | `P >= 1` |
| `R` | radius | `0.56` | finite, `> 0` |
| `theta_0` | phase | `0` | finite |
| `t` | twist | `1.5` | finite; `abs(phase) + 2 pi + 2 pi abs(t)` must remain finite |

No data structure proportional to `L` is ever allocated.

## Strand selection

Uniform (canonical):

```text
logical(i) = floor(i * L / N)
```

Random access uses a 128-bit intermediate. Iteration uses a carry cursor:
advance by `L div N` and accumulate `L mod N`, carrying one when the
remainder reaches `N`. Both agree exactly (tested).

Golden (optional):

```text
frac(i)    = (i * 0x9E3779B97F4A7C15) mod 2^64      // fixed-point fract(i / phi)
logical(i) = (frac(i) * L) >> 64
```

This is integer-only, always below `L`, and collision-free for every tested
prefix. It is a representative *ordering*, not a partition.

## Geometry

```text
f_i     = logical(i) / L
theta_i = theta_0 + 2 pi f_i
E_i     = R (cos theta_i, sin theta_i)         // egress
I_i     = -E_i                                 // ingress, exactly antipodal
```

## Transit

With progress `p in [0, 1]` and signed progress `s = 2p - 1`:

```text
psi(s) = -sign(s) * 2 pi t * (1 - |s|)
P_i(s) = s * R * (cos(theta_i + psi), sin(theta_i + psi))
```

Consequences (signed zero is normalised; radius comparisons use floating-point tolerances):

- `P_i(-1) = I_i` and `P_i(+1) = E_i` (the radial factor is `±R` and
  `psi = 0`);
- `P_i(0) = (0, 0)` because the radial factor is literally zero;
- `||P_i(s)|| = R |s|`, so the radius decreases monotonically to the nexus
  and increases monotonically after it.

Twist `t = 0` is a straight through-centre segment; non-zero twist winds the
approach and departure in opposite senses.

## Eversion

```text
eversion(p) = s = 2p - 1
```

Negative inbound, exactly zero at the nexus, positive outbound.

## Trit and lane

| Progress | Trit | Value |
|---|---|---:|
| `s < 0` | inbound | `-1` |
| `s = 0` | nexus | `0` |
| `s > 0` | outbound | `+1` |

```text
lane(i) = i mod 3
```

## Tracers

```text
tracer_strand(j) = floor(j * N / P),   0 <= j < P
```

## Canonical job stream

Ledgers and atlas censuses use:

```text
job(k) = (ordinal = k mod N, progress = (k mod 1001) / 1000)
```

so `p = 0.5` occurs exactly once per 1001 jobs. Benchmarks instead sweep
strands with progress `((iteration + ordinal) mod 1001) / 1000`.

## Audit fields

| Field | Meaning |
|---|---|
| `radius_error` | `max_i abs(||E_i|| - R)` |
| `antipodal_error` | `max_i ||I_i + E_i||` |
| `nexus_error` | `max_i ||P_i(1/2)||` |
| `endpoint_error` | `max_i` of ingress/egress reconstruction distance |
| `gap_error` | spread of consecutive logical distances minus one (zero for the uniform partition) |
| `eversion_*` | eversion at `1/4`, `1/2`, `3/4` |
| `tracer_first`, `tracer_last` | strand of the first and last tracer |

The parallel audit shards strands over workers and merges maxima, including
the boundary gap between shards, so it equals the sequential audit exactly.
