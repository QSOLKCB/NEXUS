# Ledger encoding

A ledger proves that two runs produced identical encoded evidence. It does
not prove that the evidence is physically meaningful.

## Record (40 bytes, little-endian)

| Offset | Size | Field |
|---:|---:|---|
| 0 | 1 | tag `0x4E` |
| 1 | 1 | trit value as `i8` (`0xFF`, `0x00`, `0x01`) |
| 2 | 1 | lane index |
| 3 | 1 | ordering code (`0` uniform, `1` golden) |
| 4 | 4 | ordinal `u32` |
| 8 | 8 | logical index `u64` |
| 16 | 8 | progress `f64` bits |
| 24 | 8 | `x` `f64` bits |
| 32 | 8 | `y` `f64` bits |

## Digests

```text
lane_k  = SHA256("nexus/ledger/lane/v1\n" || k || records with lane k, in stream order)
stream  = SHA256("nexus/ledger/stream/v1\n" || all records in stream order)
root    = SHA256("nexus/ledger/root/v1\n"
                 || lane_0 || count_0 || lane_1 || count_1 || lane_2 || count_2
                 || stream || count)
```

Counts are `u64` little-endian. The builder is streaming: `canonical_ledger`
never materialises the sample vector, and tests confirm it equals the
materialised ledger.

## Output

`nexus ledger <samples>` prints the three lane counts and digests, the stream
digest, the root, the sample count, and `encoded_bytes = 40 * samples`.
