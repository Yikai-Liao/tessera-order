# 2. Keys as unsigned integers

A radix sort indexes arrays with digits, so it needs keys whose **bit pattern** sorts in the same order as their **values**. Unsigned integers already do. Every other key type that Tessera supports is first *encoded* into the narrowest unsigned integer that covers its encoded bits, and the engine only ever sees the encoded key. The exact representations and padding rules are defined in the [ordering semantics](../../conformance/semantics.md#key-encoding).

## Encodings

Each rule in the [key-encoding contract](../../conformance/semantics.md#key-encoding) preserves the type's order as an unsigned integer:

| Key | Why it works |
|---|---|
| `bool`, `char`, `NonZero<T>` | Each already orders like an unsigned or underlying integer (`false < true`, Unicode scalar values, and integer order with zero absent). |
| `u8` … `u128`, `usize` | Unsigned bit patterns already sort by value. |
| `i8` … `i128`, `isize` | Two's complement puts negative numbers above positive ones when read as unsigned; flipping the sign bit moves negative values below positive ones while keeping order within each sign. |
| `f32`, `f64` | Positive IEEE 754 floats sort like unsigned integers while negative ones sort backwards; inverting negative bits and flipping positive sign bits produces `total_cmp` order. |
| Byte arrays | Big-endian byte order aligns unsigned integer order with lexicographical byte order, while zeroed unused high bits keep `Reverse` and tuple packing exact. |
| `Reverse<K>` | Inverting the effective bits reverses unsigned order while keeping equal keys equal, preserving stability. |
| Tuples | Placing earlier fields in higher bits aligns unsigned integer order with lexicographical tuple order while preserving each field's encoding, including floats and `Reverse`. |

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

The scan considers only bits that can vary across valid keys, not guaranteed-constant high bits of the integer that stores them. A `(u16, u8)` tuple key or `[u8; 3]` key is stored in a `u32`, but its top eight bits are always 0; likewise `bool` only varies in bit 0 and `char` only varies in its low 21 bits (and `Reverse` flips those constant bits to 1 without making them vary). The scan reads the first key and then batches of 64, 128, 256 and so on, doubling each time, and checks after every batch whether all candidate bits have varied. If they have, it stops: a random `(u16, u8)` or `(bool, u64)` key often stops after the first 65 records, even though the constant bits never change. While some candidate bit has not varied and input remains, the scan keeps reading.
