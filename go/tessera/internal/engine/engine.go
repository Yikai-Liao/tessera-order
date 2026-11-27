// SPDX-License-Identifier: BSD-2-Clause

package engine

type Serial uint8

const (
	Refine Serial = iota
	Indices
	Cached
)

func Direct[K any](records []K, op Serial, rank int) []int {
	codec := BindKey[K]()
	return serial(records, func(record *K) Uint128 { return codec.Encode(*record) }, codec.Decode, codec.VaryingMask, codec.StorageBits, op, rank)
}

func Keyed[T, K any](records []T, key func(*T) K, op Serial, rank int) []int {
	codec := BindKey[K]()
	return serial(records, func(record *T) Uint128 { return codec.Encode(key(record)) }, nil, codec.VaryingMask, codec.StorageBits, op, rank)
}

func serial[T any](records []T, encode func(*T) Uint128, decode func(Uint128) T, mask Uint128, width uint, op Serial, rank int) []int {
	switch width {
	case 8:
		return runSerial(records, extractor[T, uint8]{encode: encode, decode: decode, mask: mask}, op, rank)
	case 16:
		return runSerial(records, extractor[T, uint16]{encode: encode, decode: decode, mask: mask}, op, rank)
	case 32:
		return runSerial(records, extractor[T, uint32]{encode: encode, decode: decode, mask: mask}, op, rank)
	case 64:
		return runSerial(records, extractor[T, uint64]{encode: encode, decode: decode, mask: mask}, op, rank)
	default:
		return runSerial(records, extractor[T, Uint128]{encode: encode, decode: decode, mask: mask}, op, rank)
	}
}

func runSerial[T any, V encoded](records []T, e extractor[T, V], op Serial, rank int) []int {
	switch op {
	case Indices:
		indexed := extractor[int, V]{encode: func(index *int) Uint128 { return e.encode(&records[*index]) }}
		indices, spans := orderIndices(records, recordBytes[int](), &e, &indexed)
		if spans != nil {
			spans.writeIndices(indices)
		}
		return indices
	case Cached:
		sortCached(records, &e)
	default:
		refine(records, &e, rank, tileBytes)
	}
	return nil
}
