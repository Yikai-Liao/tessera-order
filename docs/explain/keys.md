# 2. Keys as unsigned integers

A radix sort indexes arrays with digits, so it needs keys whose **bit pattern** sorts in the same order as their **values**. Unsigned integers already do. Every other key type that Tessera supports is first *encoded* into the narrowest unsigned integer that covers its encoded bits, and the engine only ever sees the encoded key. The order they produce is part of the [ordering semantics](../../conformance/semantics.md#key-encoding).

## Encodings

| Key | Encoding | Why it works |
|---|---|---|
| `u8` … `u128` | unchanged | Unsigned bit patterns already sort by value. |
| `i8` … `i128` | flip the sign bit | Two's complement puts negative numbers above positive ones when read as unsigned. Flipping the sign bit moves them below and keeps the order within each sign. |
| `f32`, `f64` | negative: invert all bits; otherwise flip the sign bit | A positive IEEE 754 float sorts like an unsigned integer. A negative one sorts backwards, so inverting all of its bits reverses that order and puts it below every positive value. This is `total_cmp` order. |
| `Reverse<K>` | invert all effective bits of `K`'s encoding | Inverting all effective bits reverses unsigned order exactly while leaving unused high bits zero. Equal keys stay equal, so stability is kept. |
| Tuples (2 to 16 fields, up to 128 encoded bits) | concatenate fields in order into the narrowest covering unsigned integer | Placing the first field in the highest bits makes unsigned integer order match lexicographical tuple order. Each field keeps its own encoding, including floats and `Reverse`. |

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

Tessera finds these bits with one scan:

- the **union** (bitwise OR) of all keys has a 1 wherever *some* key has a 1;
- the **intersection** (bitwise AND) has a 1 wherever *every* key has a 1;
- `union XOR intersection` is 1 exactly where the keys disagree.

<VaryingBits />

The result, `varying`, is used everywhere later:

- If `varying` is 0, every key is equal and the input is already sorted.
- Every digit is placed over varying bits only. A digit never spends a pass on constant bits ([digit planning](tiles.md#digit-planning)). In a tile, a `u64` key that varies in only 20 bits takes two 10-bit counting passes instead of eight byte passes.
- For 128-bit keys whose variation fits in one 64-bit half, the engine sorts by that half as a `u64`, which halves the work per key.
- After a partition on a digit, that digit is constant inside each child, so the child's scan excludes it and stops once every remaining candidate bit has been seen in both states.

The scan considers only the key's *effective* bits, not every bit of the integer that stores it. A `(u16, u8)` tuple key is stored in a `u32`, but only its low 24 bits belong to fields; the top eight are padding and are always 0. The scan reads a 1,025-record prefix first. If every effective bit already varies there, as with random wide keys, it stops early: a random `(u16, u8)` key can stop once all 24 field bits have varied, even though the padding never changes. If some effective bit has not varied yet, the scan keeps reading.
