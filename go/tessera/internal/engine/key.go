// SPDX-License-Identifier: BSD-2-Clause

package engine

import (
	"fmt"
	"math"
	"math/bits"
	"unicode/utf8"
)

// Codec maps a key to unsigned order and back. Width is the effective bit
// width, while StorageBits is the width of its covering unsigned integer.
type Codec[K any] struct {
	Width       uint
	StorageBits uint
	VaryingMask Uint128
	Encode      func(K) Uint128
	Decode      func(Uint128) K
}

func newCodec[K any](width uint, encode func(K) Uint128, decode func(Uint128) K) Codec[K] {
	return Codec[K]{width, storageBits(width), lowMask(width), encode, decode}
}

func storageBits(width uint) uint {
	return uint(8) << bits.Len((width-1)/8)
}

var (
	uint8Codec = newCodec(8,
		func(value uint8) Uint128 { return Uint128{Lo: uint64(value)} },
		func(encoded Uint128) uint8 { return uint8(encoded.Lo) })
	uint16Codec = newCodec(16,
		func(value uint16) Uint128 { return Uint128{Lo: uint64(value)} },
		func(encoded Uint128) uint16 { return uint16(encoded.Lo) })
	uint32Codec = newCodec(32,
		func(value uint32) Uint128 { return Uint128{Lo: uint64(value)} },
		func(encoded Uint128) uint32 { return uint32(encoded.Lo) })
	uint64Codec = newCodec(64,
		func(value uint64) Uint128 { return Uint128{Lo: uint64(value)} },
		func(encoded Uint128) uint64 { return uint64(encoded.Lo) })
	uintCodec = newCodec(bits.UintSize,
		func(value uint) Uint128 { return Uint128{Lo: uint64(value)} },
		func(encoded Uint128) uint { return uint(encoded.Lo) })
	uintptrCodec = newCodec(bits.UintSize,
		func(value uintptr) Uint128 { return Uint128{Lo: uint64(value)} },
		func(encoded Uint128) uintptr { return uintptr(encoded.Lo) })
	int8Codec = newCodec(8,
		func(value int8) Uint128 { return signedBits(uint64(value), 8) },
		func(encoded Uint128) int8 { return int8(signedBits(encoded.Lo, 8).Lo) })
	int16Codec = newCodec(16,
		func(value int16) Uint128 { return signedBits(uint64(value), 16) },
		func(encoded Uint128) int16 { return int16(signedBits(encoded.Lo, 16).Lo) })
	int32Codec = newCodec(32,
		func(value int32) Uint128 { return signedBits(uint64(value), 32) },
		func(encoded Uint128) int32 { return int32(signedBits(encoded.Lo, 32).Lo) })
	int64Codec = newCodec(64,
		func(value int64) Uint128 { return signedBits(uint64(value), 64) },
		func(encoded Uint128) int64 { return int64(signedBits(encoded.Lo, 64).Lo) })
	intCodec = newCodec(bits.UintSize,
		func(value int) Uint128 { return signedBits(uint64(value), bits.UintSize) },
		func(encoded Uint128) int { return int(signedBits(encoded.Lo, bits.UintSize).Lo) })
	float32Codec = newCodec(32,
		func(value float32) Uint128 { return floatBits(math.Float32bits(value)) },
		func(encoded Uint128) float32 { return math.Float32frombits(rawFloatBits(uint32(encoded.Lo))) })
	float64Codec = newCodec(64,
		func(value float64) Uint128 { return floatBits(math.Float64bits(value)) },
		func(encoded Uint128) float64 { return math.Float64frombits(rawFloatBits(uint64(encoded.Lo))) })
)

type sealedKey[K any] interface {
	radixCodec() Codec[K]
}

// BindKey checks the concrete key type before any record or callback access.
// The returned functions use the concrete key type without further dispatch.
func BindKey[K any]() Codec[K] {
	var zero K
	switch key := any(zero).(type) {
	case sealedKey[K]:
		return key.radixCodec()
	case uint8:
		return codecAs[K](uint8Codec)
	case uint16:
		return codecAs[K](uint16Codec)
	case uint32:
		return codecAs[K](uint32Codec)
	case uint64:
		return codecAs[K](uint64Codec)
	case uint:
		return codecAs[K](uintCodec)
	case uintptr:
		return codecAs[K](uintptrCodec)
	case int8:
		return codecAs[K](int8Codec)
	case int16:
		return codecAs[K](int16Codec)
	case int32:
		return codecAs[K](int32Codec)
	case int64:
		return codecAs[K](int64Codec)
	case int:
		return codecAs[K](intCodec)
	case float32:
		return codecAs[K](float32Codec)
	case float64:
		return codecAs[K](float64Codec)
	case bool:
		return codecAs[K](boolCodec())
	case [1]byte:
		return codecAs[K](byteArrayCodec[[1]byte]())
	case [2]byte:
		return codecAs[K](byteArrayCodec[[2]byte]())
	case [3]byte:
		return codecAs[K](byteArrayCodec[[3]byte]())
	case [4]byte:
		return codecAs[K](byteArrayCodec[[4]byte]())
	case [5]byte:
		return codecAs[K](byteArrayCodec[[5]byte]())
	case [6]byte:
		return codecAs[K](byteArrayCodec[[6]byte]())
	case [7]byte:
		return codecAs[K](byteArrayCodec[[7]byte]())
	case [8]byte:
		return codecAs[K](byteArrayCodec[[8]byte]())
	case [9]byte:
		return codecAs[K](byteArrayCodec[[9]byte]())
	case [10]byte:
		return codecAs[K](byteArrayCodec[[10]byte]())
	case [11]byte:
		return codecAs[K](byteArrayCodec[[11]byte]())
	case [12]byte:
		return codecAs[K](byteArrayCodec[[12]byte]())
	case [13]byte:
		return codecAs[K](byteArrayCodec[[13]byte]())
	case [14]byte:
		return codecAs[K](byteArrayCodec[[14]byte]())
	case [15]byte:
		return codecAs[K](byteArrayCodec[[15]byte]())
	case [16]byte:
		return codecAs[K](byteArrayCodec[[16]byte]())
	default:
		panic(fmt.Sprintf("tessera: unsupported key type %T", (*K)(nil)))
	}
}

// The matching switch arm proves K and T are the same concrete type.
func codecAs[K, T any](codec Codec[T]) Codec[K] {
	return any(codec).(Codec[K])
}

func signedBits(raw uint64, width uint) Uint128 {
	flip := uint64(1) << (width - 1)
	return Uint128{Lo: raw&(flip|(flip-1)) ^ flip}
}

func (Uint128) radixCodec() Codec[Uint128] {
	return newCodec(128, func(value Uint128) Uint128 { return value }, func(encoded Uint128) Uint128 {
		return encoded
	})
}

func (Int128) radixCodec() Codec[Int128] {
	const sign = uint64(1) << 63
	return newCodec(128, func(value Int128) Uint128 {
		return Uint128{Hi: uint64(value.Hi) ^ sign, Lo: value.Lo}
	}, func(encoded Uint128) Int128 {
		return Int128{Hi: int64(encoded.Hi ^ sign), Lo: encoded.Lo}
	})
}

func floatBits[U uint32 | uint64](raw U) Uint128 {
	sign := ^U(0) ^ (^U(0) >> 1)
	floatMask := sign - 1
	if raw&sign != 0 {
		return Uint128{Lo: uint64(raw ^ (floatMask | sign))}
	}
	return Uint128{Lo: uint64(raw ^ sign)}
}

func rawFloatBits[U uint32 | uint64](encoded U) U {
	sign := ^U(0) ^ (^U(0) >> 1)
	if encoded&sign != 0 {
		return encoded ^ sign
	}
	return ^encoded
}

func boolCodec() Codec[bool] {
	codec := newCodec(8, func(value bool) Uint128 {
		if value {
			return Uint128{Lo: 1}
		}
		return Uint128{}
	}, func(encoded Uint128) bool {
		return encoded.Lo != 0
	})
	codec.VaryingMask = Uint128{Lo: 1}
	return codec
}

type CharKey struct {
	value rune
}

func NewChar(value rune) CharKey {
	if !utf8.ValidRune(value) {
		panic("tessera: invalid Unicode scalar")
	}
	return CharKey{value}
}

func (CharKey) radixCodec() Codec[CharKey] {
	codec := newCodec(32, func(value CharKey) Uint128 {
		return Uint128{Lo: uint64(value.value)}
	}, func(encoded Uint128) CharKey {
		return NewChar(rune(encoded.Lo))
	})
	codec.VaryingMask = Uint128{Lo: 0x001f_ffff}
	return codec
}

type byteArray interface {
	[1]byte | [2]byte | [3]byte | [4]byte | [5]byte | [6]byte | [7]byte | [8]byte |
		[9]byte | [10]byte | [11]byte | [12]byte | [13]byte | [14]byte | [15]byte | [16]byte
}

func byteArrayCodec[K byteArray]() Codec[K] {
	var zero K
	return newCodec(uint(len(zero))*8, packArray[K], unpackArray[K])
}

type ReverseKey[K any] struct {
	Value K
}

func (ReverseKey[K]) radixCodec() Codec[ReverseKey[K]] {
	inner := BindKey[K]()
	reverseMask := lowMask(inner.Width)
	codec := newCodec(inner.Width, func(key ReverseKey[K]) Uint128 {
		return inner.Encode(key.Value).xor(reverseMask)
	}, func(encoded Uint128) ReverseKey[K] {
		return ReverseKey[K]{Value: inner.Decode(encoded.xor(reverseMask))}
	})
	codec.VaryingMask = inner.VaryingMask
	return codec
}

type PairKey[A, B any] struct {
	First  A
	Second B
}

func (PairKey[A, B]) radixCodec() Codec[PairKey[A, B]] {
	first, second := BindKey[A](), BindKey[B]()
	width := first.Width + second.Width
	if width > 128 {
		panic("tessera: composite key exceeds 128 encoded bits")
	}
	secondMask := lowMask(second.Width)
	codec := newCodec(width, func(key PairKey[A, B]) Uint128 {
		return first.Encode(key.First).lsh(second.Width).or(second.Encode(key.Second))
	}, func(encoded Uint128) PairKey[A, B] {
		return PairKey[A, B]{
			First:  first.Decode(encoded.rsh(second.Width)),
			Second: second.Decode(encoded.and(secondMask)),
		}
	})
	codec.VaryingMask = first.VaryingMask.lsh(second.Width).or(second.VaryingMask)
	return codec
}

// Digit is one pass worth of bits starting at Shift.
type Digit struct {
	Shift, Width uint
}

func (digit Digit) Mask() Uint128 {
	return lowMask(digit.Width).lsh(digit.Shift)
}

func (digit Digit) Bucket(encoded Uint128) uint {
	return uint(encoded.rsh(digit.Shift).Lo & lowMask(digit.Width).Lo)
}
