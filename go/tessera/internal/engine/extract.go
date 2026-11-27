// SPDX-License-Identifier: BSD-2-Clause

package engine

type encoded interface {
	uint8 | uint16 | uint32 | uint64 | Uint128
}

func lowWord[V encoded](key V) uint64 {
	switch value := any(key).(type) {
	case uint8:
		return uint64(value)
	case uint16:
		return uint64(value)
	case uint32:
		return uint64(value)
	case uint64:
		return value
	default:
		return any(key).(Uint128).Lo
	}
}

func fromWord[V encoded](value uint64) V {
	var zero V
	switch any(zero).(type) {
	case uint8:
		return any(uint8(value)).(V)
	case uint16:
		return any(uint16(value)).(V)
	case uint32:
		return any(uint32(value)).(V)
	case uint64:
		return any(value).(V)
	default:
		return any(Uint128{Lo: value}).(V)
	}
}

func fromBits[V encoded](value Uint128) V {
	if isWide[V]() {
		return any(value).(V)
	}
	return fromWord[V](value.Lo)
}

func keyBits[V encoded](value V) Uint128 {
	if isWide[V]() {
		return any(value).(Uint128)
	}
	return Uint128{Lo: lowWord(value)}
}

func isWide[V encoded]() bool {
	return recordBytes[V]() > 8
}

func less[V encoded](left, right V) bool {
	if isWide[V]() {
		a, b := any(left).(Uint128), any(right).(Uint128)
		return a.Hi < b.Hi || a.Hi == b.Hi && a.Lo < b.Lo
	}
	return lowWord(left) < lowWord(right)
}

func wordBucket[V encoded](key V, digit Digit) int {
	return int(lowWord(key) >> digit.Shift & ((1 << digit.Width) - 1))
}

func keyDigit[V encoded](key V, digit Digit) int {
	if isWide[V]() {
		return int(digit.Bucket(any(key).(Uint128)))
	}
	return wordBucket(key, digit)
}

func xor[V encoded](a, b V) V {
	if isWide[V]() {
		return any(any(a).(Uint128).xor(any(b).(Uint128))).(V)
	}
	return fromWord[V](lowWord(a) ^ lowWord(b))
}

func or[V encoded](a, b V) V {
	if isWide[V]() {
		return any(any(a).(Uint128).or(any(b).(Uint128))).(V)
	}
	return fromWord[V](lowWord(a) | lowWord(b))
}

func and[V encoded](a, b V) V {
	if isWide[V]() {
		return any(any(a).(Uint128).and(any(b).(Uint128))).(V)
	}
	return fromWord[V](lowWord(a) & lowWord(b))
}

// bind keeps key families out of the engine. Only the record and encoded
// widths affect its instantiation.
type extractor[T any, V encoded] struct {
	encode func(*T) Uint128
	decode func(Uint128) T
	mask   Uint128
	half   uint
}

func (e *extractor[T, V]) get(record *T) V {
	return fromBits[V](e.encode(record).rsh(e.half))
}

func (e *extractor[T, V]) direct() bool {
	return e.decode != nil
}

// Fixed byte arrays use big-endian digits, with zeroes above the effective width.
func packArray[A byteArray](value A) Uint128 {
	var bits Uint128
	for i := 0; i < max(len(value)-8, 0); i++ {
		bits.Hi = bits.Hi<<8 | uint64(value[i])
	}
	for i := max(len(value)-8, 0); i < len(value); i++ {
		bits.Lo = bits.Lo<<8 | uint64(value[i])
	}
	return bits
}

func unpackArray[A byteArray](bits Uint128) A {
	var value A
	i := len(value)
	for ; i > max(len(value)-8, 0); i-- {
		value[i-1] = byte(bits.Lo)
		bits.Lo >>= 8
	}
	for ; i > 0; i-- {
		value[i-1] = byte(bits.Hi)
		bits.Hi >>= 8
	}
	return value
}
