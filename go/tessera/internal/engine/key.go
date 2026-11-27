// SPDX-License-Identifier: BSD-2-Clause

package engine

import (
	"encoding/binary"
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
	storageBits := uint(8) << bits.Len((width-1)/8)
	return Codec[K]{width, storageBits, lowMask(width), encode, decode}
}

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
		return codecAs[K](unsignedCodec[uint8](8))
	case uint16:
		return codecAs[K](unsignedCodec[uint16](16))
	case uint32:
		return codecAs[K](unsignedCodec[uint32](32))
	case uint64:
		return codecAs[K](unsignedCodec[uint64](64))
	case uint:
		return codecAs[K](unsignedCodec[uint](bits.UintSize))
	case uintptr:
		return codecAs[K](unsignedCodec[uintptr](bits.UintSize))
	case int8:
		return codecAs[K](signedCodec[int8](8))
	case int16:
		return codecAs[K](signedCodec[int16](16))
	case int32:
		return codecAs[K](signedCodec[int32](32))
	case int64:
		return codecAs[K](signedCodec[int64](64))
	case int:
		return codecAs[K](signedCodec[int](bits.UintSize))
	case float32:
		return codecAs[K](floatCodec(32, math.Float32bits, math.Float32frombits))
	case float64:
		return codecAs[K](floatCodec(64, math.Float64bits, math.Float64frombits))
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

func unsignedCodec[K uint8 | uint16 | uint32 | uint64 | uint | uintptr](width uint) Codec[K] {
	return newCodec(width, func(value K) Uint128 {
		return Uint128{Lo: uint64(value)}
	}, func(encoded Uint128) K {
		return K(encoded.Lo)
	})
}

func signedCodec[K int8 | int16 | int32 | int64 | int](width uint) Codec[K] {
	mask := lowMask(width).Lo
	sign := uint64(1) << (width - 1)
	return newCodec(width, func(value K) Uint128 {
		return Uint128{Lo: (uint64(value) & mask) ^ sign}
	}, func(encoded Uint128) K {
		return K(encoded.Lo ^ sign)
	})
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

func floatCodec[K float32 | float64, U uint32 | uint64](
	width uint, toBits func(K) U, fromBits func(U) K,
) Codec[K] {
	sign := U(1) << (width - 1)
	return newCodec(width, func(value K) Uint128 {
		raw := toBits(value)
		if raw&sign != 0 {
			return Uint128{Lo: uint64(^raw)}
		}
		return Uint128{Lo: uint64(raw ^ sign)}
	}, func(encoded Uint128) K {
		raw := U(encoded.Lo)
		if raw&sign != 0 {
			return fromBits(raw ^ sign)
		}
		return fromBits(^raw)
	})
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
	return newCodec(uint(len(zero))*8, func(value K) Uint128 {
		var buf [16]byte
		// Start with zeroed high bytes, then pack the key in big-endian order.
		for i := 0; i < len(value); i++ {
			buf[16-len(value)+i] = value[i]
		}
		return Uint128{Hi: binary.BigEndian.Uint64(buf[:8]), Lo: binary.BigEndian.Uint64(buf[8:])}
	}, func(encoded Uint128) K {
		var buf [16]byte
		binary.BigEndian.PutUint64(buf[:8], encoded.Hi)
		binary.BigEndian.PutUint64(buf[8:], encoded.Lo)
		var value K
		for i := 0; i < len(value); i++ {
			value[i] = buf[16-len(value)+i]
		}
		return value
	})
}

type ReverseKey[K any] struct {
	Value K
}

func (ReverseKey[K]) radixCodec() Codec[ReverseKey[K]] {
	inner := BindKey[K]()
	mask := lowMask(inner.Width)
	codec := newCodec(inner.Width, func(key ReverseKey[K]) Uint128 {
		return inner.Encode(key.Value).xor(mask)
	}, func(encoded Uint128) ReverseKey[K] {
		return ReverseKey[K]{Value: inner.Decode(encoded.xor(mask))}
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
