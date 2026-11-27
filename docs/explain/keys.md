# 2. Keys as unsigned integers

**The core idea:** a radix pass indexes buckets directly with bits, so every supported Rust key type is first mapped to an unsigned integer whose numerical order matches the requested sort order — and any bit that never changes across the input is dropped before sorting starts.

## Turning any key into an unsigned integer

Unsigned integers (`u8` through `u128`) already sort identically by bit pattern and by value. Every other key type needs only a tiny branchless bit transform so the engine never needs type-specific comparisons:

| Key family | Bit transform | Why it preserves order |
|---|---|---|
| `bool`, `char`, `NonZero<T>` | Identity | Bit patterns already sort by value (`false < true`, Unicode scalar order, zero absent). |
| Signed integers (`i8`…`i128`) | Flip the sign bit | Two's complement puts negatives above positives when read as unsigned; flipping the top bit swaps the halves cleanly. |
| Floats (`f32`, `f64`) | Negative: invert all bits; positive: flip sign bit | IEEE 754 positives already sort like unsigned integers while negatives run backwards; this produces exact `total_cmp` order, including subnormals, `-0.0 < +0.0`, and NaNs. |
| Byte arrays `[u8; 1..=16]` | Big-endian pack, zero high padding | First byte sits in the highest bits, matching lexicographical order. |
| Tuples (2–16 fields, ≤128 bits) | Concatenate fields high-to-low, zero high padding | Earlier fields dominate higher bits, matching lexicographical order while preserving each field's own encoding. |
| `Reverse<K>` | Invert all effective bits of `K` | Reverses bucket order while keeping equal keys equal, so descending sort stays **stable** (reversing an ascending result would invert equal keys). |

Switch key types or toggle `Reverse` below — highlighted bits mark every bit flipped by the encoder, and the rows reorder immediately by their resulting unsigned integer:

<KeyEncoder />

To treat all NaNs as equal or `-0.0` as equal to `+0.0`, normalize the key inside your key closure. The formal contract for every supported type lives in [ordering semantics](../../conformance/semantics.md#key-encoding).

## Varying bits

Real keys rarely use all 64 bits. Timestamps share their upper bits, small IDs carry dozens of leading zeros, and composite tuples often pack flags into a few positions. Any bit that has the same value in every record cannot distinguish two keys.

Tessera finds the active bits with two bitwise reductions in one pass:

- **Union (`OR`)** has a `1` wherever *any* key has a `1`.
- **Intersection (`AND`)** has a `1` wherever *every* key has a `1`.
- **`varying = OR ^ AND`** has a `1` strictly at the bit positions where keys disagree.

<VaryingBits />

### Exact rules & thresholds

- **Zero varying bits (`varying == 0`):** every key in the slice is equal, so the sort finishes immediately without allocating scratch.
- **Digit placement:** every digit window is placed strictly over varying bits; constant gaps cost zero passes (for example, a `u64` varying only in 20 bits takes two 10-bit tile passes instead of eight byte passes).
- **128-bit narrowing:** if a 128-bit key's variation fits entirely within its low or high 64-bit half, the engine drops the constant half and sorts it as a 64-bit key.
- **Effective candidate masks:** the scan ignores guaranteed-constant padding bits (the top 8 zero bits of `(u16, u8)` or `[u8; 3]`, bits 1–7 of `bool`, bits 21–31 of `char`, and their `1`-filled complements under `Reverse`).
- **Doubling-batch early exit:** after reading the first key, the scan reads batches of `64, 128, 256, …` keys and stops as soon as every candidate bit has been seen in both `0` and `1` states — random narrow or padded keys typically saturate in the first 65 records. Inside every recursive child, partitioned parent bits are excluded from the candidate mask so they can never reappear.

Next: [3. Routing an input](routing.md) shows how Tessera skips radix passes altogether on sorted, reversed, nearly sorted, or skewed data.
