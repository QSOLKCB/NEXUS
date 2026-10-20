# Changelog

## Unreleased

- Reject unused CLI options, extra positional arguments and nonfinite benchmark checksums.
- Reject overflowing transit angles, invalid audio tables and frequency underflow.
- Validate census marginals and malformed counts without arithmetic panics.
- Fix large-ring partner overflow and align increment wrapping with WGSL.
- Bound GPU counts and read back only the requested census regions.
- Account for CPU quotas; exclude failed timings and reject empty audit comparisons.
- Restore upload omissions with CI, ignore rules and executable script modes.
- Add a dependency-free WebGL cuboctahedron explorer, Rust/browser parity checks,
  GitHub Pages deployment workflow and review documentation.

## 1.0.0

First release of this implementation. It is a clean rewrite and shares no
code, tables, encodings, or command surface with earlier runners.

- Branch-free signed-progress transit with an exact zero centre.
- Carry-cursor uniform partition and 64-bit fixed-point golden ordering.
- Order-preserving parallel execution via an atomic block cursor.
- 40-byte canonical records; lane, stream, and root SHA-256 ledgers.
- Configurable sonification map (432 Hz default).
- Sharded invariant audit with exact sequential/parallel agreement.
- Cuboctahedron derived from coordinates; one-factorisation by search.
- Reversible `Z_4` lattice CPU reference with order-independent fold.
- Twelve-direction atlas with reversible `Z_3 x Z_4` walk.
- Optional Vulkan/WGSL lattice mirror behind the `gpu` feature.
- Independent multicore verification protocol and harness.
