// SPDX-License-Identifier: BSD-2-Clause

package engine

// Serial is the operation requested by the slice adapter. A rank of -1
// requests every position for in-place refinement.
type Serial uint8

const (
	Refine Serial = iota
	Indices
	Cached
)

// Direct binds primitive slice types once. Wrappers use the sealed codec.
func Direct[K any](records []K, op Serial, rank int) []int {
	switch values := any(records).(type) {
	case []uint8:
		return directIdentity[uint8](values, op, rank, lowMask(8))
	case []uint16:
		return directIdentity[uint16](values, op, rank, lowMask(16))
	case []uint32:
		return directIdentity[uint32](values, op, rank, lowMask(32))
	case []uint64:
		return directIdentity[uint64](values, op, rank, lowMask(64))
	case []int8:
		return directBuiltin[int8, uint8](values, op, rank, lowMask(8))
	case []int16:
		return directBuiltin[int16, uint16](values, op, rank, lowMask(16))
	case []int32:
		return directBuiltin[int32, uint32](values, op, rank, lowMask(32))
	case []int64:
		return directBuiltin[int64, uint64](values, op, rank, lowMask(64))
	case []int:
		return directWord(values, op, rank)
	case []uint:
		return directWord(values, op, rank)
	case []uintptr:
		return directWord(values, op, rank)
	case []float32:
		return directBuiltin[float32, uint32](values, op, rank, lowMask(32))
	case []float64:
		return directBuiltin[float64, uint64](values, op, rank, lowMask(64))
	case []bool:
		return directBuiltin[bool, uint8](values, op, rank, Uint128{Lo: 1})
	case []Uint128, []Int128, [][1]byte, [][2]byte, [][3]byte, [][4]byte, [][5]byte, [][6]byte, [][7]byte, [][8]byte, [][9]byte, [][10]byte, [][11]byte, [][12]byte, [][13]byte, [][14]byte, [][15]byte, [][16]byte:
		return directFixed(records, op, rank)
	case []CharKey:
		return directBuiltin[CharKey, uint32](values, op, rank, Uint128{Lo: 0x001f_ffff})
	}
	codec := BindKey[K]()
	switch codec.StorageBits {
	case 8:
		return directCodecRun[K, uint8](records, op, rank, codec)
	case 16:
		return directCodecRun[K, uint16](records, op, rank, codec)
	case 32:
		return directCodecRun[K, uint32](records, op, rank, codec)
	case 64:
		return directCodecRun[K, uint64](records, op, rank, codec)
	default:
		return directCodecRun[K, Uint128](records, op, rank, codec)
	}
}

func directFixed[K any](records []K, op Serial, rank int) []int {
	bytes := recordBytes[K]()
	mask := lowMask(uint(bytes) * 8)
	switch {
	case bytes == 1:
		return directBuiltin[K, uint8](records, op, rank, mask)
	case bytes == 2:
		return directBuiltin[K, uint16](records, op, rank, mask)
	case bytes <= 4:
		return directBuiltin[K, uint32](records, op, rank, mask)
	case bytes <= 8:
		return directBuiltin[K, uint64](records, op, rank, mask)
	default:
		return directBuiltin[K, Uint128](records, op, rank, mask)
	}
}

func directWord[K any](records []K, op Serial, rank int) []int {
	if wordBits == 32 {
		return directBuiltin[K, uint32](records, op, rank, lowMask(32))
	}
	return directBuiltin[K, uint64](records, op, rank, lowMask(64))
}

func directIdentity[V encoded](records []V, op Serial, rank int, mask Uint128) []int {
	e := extractor[V, V, V, V, directIdentityMode]{mask: mask}
	if op == Indices {
		indexed := extractor[int, V, V, V, directIdentityIndexMode]{source: records, mask: mask}
		return argsort(records, &e, &indexed)
	}
	refine(records, &e, rank, tileBytes)
	return nil
}

func directBuiltin[K any, V encoded](records []K, op Serial, rank int, mask Uint128) []int {
	e := extractor[K, K, K, V, directMode]{mask: mask}
	if op == Indices {
		indexed := extractor[int, K, K, V, directIndexMode]{source: records, mask: mask}
		return argsort(records, &e, &indexed)
	}
	refine(records, &e, rank, tileBytes)
	return nil
}

func directCodecRun[K any, V encoded](records []K, op Serial, rank int, codec Codec[K]) []int {
	e := extractor[K, K, K, V, directCodecMode]{codec: codec, mask: codec.VaryingMask}
	if op == Indices {
		indexed := extractor[int, K, K, V, directCodecIndexMode]{codec: codec, source: records, mask: codec.VaryingMask}
		return argsort(records, &e, &indexed)
	}
	refine(records, &e, rank, tileBytes)
	return nil
}

// Keyed binds before the first call of the key function, even on empty input.
func Keyed[T, K any](records []T, key func(*T) K, op Serial, rank int) []int {
	codec := keyedCodec[K]()
	switch codec.StorageBits {
	case 8:
		return keyedRun[T, K, uint8](records, key, op, rank, codec)
	case 16:
		return keyedRun[T, K, uint16](records, key, op, rank, codec)
	case 32:
		return keyedRun[T, K, uint32](records, key, op, rank, codec)
	case 64:
		return keyedRun[T, K, uint64](records, key, op, rank, codec)
	default:
		return keyedRun[T, K, Uint128](records, key, op, rank, codec)
	}
}

// Native shapes need only a layout. Binding function values here would
// allocate even when the order probe finishes without scratch.
func keyedCodec[K any]() Codec[K] {
	var zero K
	var width uint
	switch any(zero).(type) {
	case uint8, int8:
		width = 8
	case uint16, int16:
		width = 16
	case uint32, int32, float32:
		width = 32
	case uint64, int64, float64:
		width = 64
	case int, uint, uintptr:
		width = wordBits
	case bool:
		return Codec[K]{Width: 8, StorageBits: 8, VaryingMask: Uint128{Lo: 1}}
	case Uint128, Int128, [1]byte, [2]byte, [3]byte, [4]byte, [5]byte, [6]byte, [7]byte, [8]byte,
		[9]byte, [10]byte, [11]byte, [12]byte, [13]byte, [14]byte, [15]byte, [16]byte:
		width = uint(recordBytes[K]()) * 8
		storage := storageBits(width)
		return Codec[K]{Width: width, StorageBits: storage, VaryingMask: lowMask(width)}
	case CharKey:
		return Codec[K]{Width: 32, StorageBits: 32, VaryingMask: Uint128{Lo: 0x001f_ffff}}
	default:
		return BindKey[K]()
	}
	return Codec[K]{Width: width, StorageBits: width, VaryingMask: lowMask(width)}
}

func keyedRun[T, K any, V encoded](records []T, key func(*T) K, op Serial, rank int, codec Codec[K]) []int {
	e := extractor[T, T, K, V, keyedMode]{key: key, codec: codec, mask: codec.VaryingMask}
	switch op {
	case Indices:
		indexed := extractor[int, T, K, V, keyedIndexMode]{key: key, codec: codec, source: records, mask: codec.VaryingMask}
		return argsort(records, &e, &indexed)
	case Cached:
		sortCached(records, &e)
	default:
		refine(records, &e, rank, tileBytes)
	}
	return nil
}

func argsort[T, R, K any, V encoded, M, I sourceMode](records []T, e *extractor[T, R, K, V, M], indexed *extractor[int, R, K, V, I]) []int {
	indices, spans := orderIndices(records, recordBytes[int](), e, indexed)
	if spans != nil {
		spans.writeIndices(indices)
	}
	return indices
}
