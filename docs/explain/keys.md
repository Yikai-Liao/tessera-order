# 2. Keys as unsigned integers

A radix sort indexes arrays with digits, so it needs keys whose **bit pattern** sorts in the same order as their **values**. Unsigned integers already do. Every other key type that Tessera supports is first *encoded* into an unsigned integer of the same width, and the engine only ever sees the encoded key. The encodings are in [`key.rs`](../../rust/tessera-sort/src/key.rs), and the order they produce is part of the [ordering semantics](../../conformance/semantics.md#key-encoding).

## Encodings

| Key | Encoding | Why it works |
|---|---|---|
| `u8` … `u128` | unchanged | Unsigned bit patterns already sort by value. |
| `i8` … `i128` | flip the sign bit | Two's complement puts negative numbers above positive ones when read as unsigned. Flipping the sign bit moves them below and keeps the order within each sign. |
| `f32`, `f64` | negative: invert all bits; otherwise flip the sign bit | A positive IEEE 754 float sorts like an unsigned integer. A negative one sorts backwards, so inverting all of its bits reverses that order and puts it below every positive value. This is `total_cmp` order. |
| `Reverse<K>` | invert all bits of `K`'s encoding | Inverting all bits reverses unsigned order exactly. Equal keys stay equal, so stability is kept. |

Try it: type keys, switch the type, and read the encoded integers. Sorting by them gives the order Tessera produces.

<KeyEncoder />

What to notice:

- `-0.0` and `+0.0` encode to different integers. `-0.0` sorts first, as in `f64::total_cmp`.
- `NaN` with the sign bit clear sorts after `+inf`. A NaN with the sign bit set would sort before `-inf`.
- Subnormal floats such as `3e-39` need no special case.
- `Reverse` turns an ascending sort into a descending one *without* reversing equal keys, which reversing an ascending result would do.

Normalize the key in the key function if you want every NaN equal, or both zeros equal.

## Varying bits

Real keys rarely use all of their bits. Timestamps share their high bits, small IDs in a `u64` have dozens of leading zeros, and a flag stored in bit 40 is the only bit that changes. A bit that has the same value in every key cannot affect the order.

Tessera finds these bits with one scan (`varying_key_bits` in [`block.rs`](../../rust/tessera-sort/src/block.rs)):

- the **union** (bitwise OR) of all keys has a 1 wherever *some* key has a 1;
- the **intersection** (bitwise AND) has a 1 wherever *every* key has a 1;
- `union XOR intersection` is 1 exactly where the keys disagree.

<VaryingBits />

The result, `varying`, is used everywhere later:

- If `varying` is 0, every key is equal and the input is already sorted.
- Every digit is placed over varying bits only. A digit never spends a pass on constant bits ([digit planning](tiles.md#digit-planning)).
- For 128-bit keys whose variation fits in one 64-bit half, the engine sorts by that half as a `u64` (`KeyHalf` in `key.rs`), which halves the work per key.
- After a partition on a digit, that digit is constant inside each child, so the child's scan excludes it and stops once every remaining candidate bit has been seen in both states (`scan_varying_bits` in `engine.rs`).

The scan reads a 1,025-record prefix first. If every bit already varies there, as with random wide keys, it stops early.
