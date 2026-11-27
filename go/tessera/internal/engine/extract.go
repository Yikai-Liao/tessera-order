// SPDX-License-Identifier: BSD-2-Clause

package engine

import "math"

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

// The array length is a constant of the instantiated shape. Unused extraction
// branches, including calls to user functions, disappear from direct kernels.
type directMode = [0]byte
type keyedMode = [1]byte
type directCodecMode = [2]byte
type directIdentityMode = [3]byte
type directIndexMode = [4]byte
type keyedIndexMode = [5]byte
type directCodecIndexMode = [6]byte
type directIdentityIndexMode = [7]byte
type cachedIndexMode = [8]byte
type prefixIndexMode = [9]byte
type categoryMode = [10]byte
type sourceMode interface {
	directMode | keyedMode | directCodecMode | directIdentityMode |
		directIndexMode | keyedIndexMode | directCodecIndexMode | directIdentityIndexMode |
		cachedIndexMode | prefixIndexMode | categoryMode
}

type extractor[T, R, K any, V encoded, M sourceMode] struct {
	key    func(*R) K
	codec  Codec[K]
	source []R
	keys   []V
	mask   Uint128
	shift  uint
	offset uint
	digit  Digit
}

func (e *extractor[T, R, K, V, M]) direct() bool {
	var mode M
	return len(mode) == 0 || len(mode) == 2 || len(mode) == 3
}

func (e *extractor[T, R, K, V, M]) get(record *T) V {
	var mode M
	if len(mode) != 3 {
		return e.getOther(record)
	}
	return *any(record).(*V)
}

func (e *extractor[T, R, K, V, M]) getOther(record *T) V {
	var mode M
	switch len(mode) {
	case 0:
		return builtinEncode[K, V](*any(record).(*K), e.offset)
	case 1:
		key := e.key(any(record).(*R))
		if e.codec.Encode != nil {
			return fromBits[V](e.codec.Encode(key).rsh(e.offset))
		}
		return builtinEncode[K, V](key, e.offset)
	case 2:
		return fromBits[V](e.codec.Encode(*any(record).(*K)).rsh(e.offset))
	case 7:
		return any(e.source[*any(record).(*int)]).(V)
	case 4:
		return builtinEncode[K, V](any(e.source[*any(record).(*int)]).(K), e.offset)
	case 5:
		key := e.key(&e.source[*any(record).(*int)])
		if e.codec.Encode != nil {
			return fromBits[V](e.codec.Encode(key).rsh(e.offset))
		}
		return builtinEncode[K, V](key, e.offset)
	case 6:
		return fromBits[V](e.codec.Encode(any(e.source[*any(record).(*int)]).(K)).rsh(e.offset))
	case 8:
		return e.keys[*any(record).(*int)]
	case 9:
		return fromWord[V](uint64(uint(*any(record).(*int)) >> e.shift))
	default:
		return fromWord[V](uint64(e.shiftedCategory(record)))
	}
}

// Category replay consumes the cached digits once in source order.
func (e *extractor[T, R, K, V, M]) shiftedCategory(_ *T) int {
	index := int(e.shift)
	e.shift++
	return keyDigit(e.keys[index], e.digit)
}

func (e *extractor[T, R, K, V, M]) fullBits(record *T) Uint128 {
	var mode M
	if len(mode) == 0 || len(mode) == 3 {
		if recordBytes[K]() > 8 {
			return encodeWide(*any(record).(*K))
		}
		return keyBits(e.get(record))
	}
	return e.codec.Encode(*any(record).(*K))
}

func (e *extractor[T, R, K, V, M]) decode(key Uint128) T {
	var mode M
	if len(mode) == 3 {
		return any(fromBits[V](key)).(T)
	}
	if len(mode) == 0 {
		return any(builtinDecode[K](key)).(T)
	}
	return any(e.codec.Decode(key)).(T)
}

func builtinEncode[K any, V encoded](key K, offset uint) V {
	if recordBytes[K]() == 1 {
		return any(encodeByte(key)).(V)
	}
	if recordBytes[K]() == 2 {
		return any(encodeHalfword(key)).(V)
	}
	if recordBytes[K]() == 4 {
		return any(encodeWord(key)).(V)
	}
	if recordBytes[K]() == 8 {
		return any(encodeDoubleword(key)).(V)
	}
	if recordBytes[K]() > 8 {
		return fromBits[V](encodeWide(key).rsh(offset))
	}
	return fromWord[V](encodePartialWord(key))
}

func encodeByte[K any](key K) uint8 {
	switch value := any(key).(type) {
	case uint8:
		return value
	case int8:
		return uint8(value) ^ (1 << 7)
	case bool:
		if value {
			return 1
		}
		return 0
	default:
		return any(key).([1]byte)[0]
	}
}

func encodeHalfword[K any](key K) uint16 {
	switch value := any(key).(type) {
	case uint16:
		return value
	case int16:
		return uint16(value) ^ (1 << 15)
	default:
		return uint16(packArrayWord(any(key).([2]byte)))
	}
}

func encodeWord[K any](key K) uint32 {
	switch value := any(key).(type) {
	case uint32:
		return value
	case int32:
		return uint32(value) ^ (1 << 31)
	case float32:
		raw := math.Float32bits(value)
		return raw ^ ((0 - (raw >> 31)) | (1 << 31))
	case CharKey:
		return uint32(value.value)
	case uint:
		return uint32(value)
	case uintptr:
		return uint32(value)
	case int:
		return uint32(value) ^ (1 << 31)
	default:
		return uint32(packArrayWord(any(key).([4]byte)))
	}
}

func encodeDoubleword[K any](key K) uint64 {
	switch value := any(key).(type) {
	case uint64:
		return value
	case int64:
		return uint64(value) ^ (1 << 63)
	case float64:
		raw := math.Float64bits(value)
		return raw ^ ((0 - (raw >> 63)) | (1 << 63))
	case uint:
		return uint64(value)
	case uintptr:
		return uint64(value)
	case int:
		return uint64(value) ^ (1 << 63)
	default:
		return packArrayWord(any(key).([8]byte))
	}
}

func builtinDecode[K any](key Uint128) K {
	word := key.Lo
	var zero K
	switch any(zero).(type) {
	case Uint128:
		return any(key).(K)
	case uint8:
		return any(uint8(word)).(K)
	case uint16:
		return any(uint16(word)).(K)
	case uint32:
		return any(uint32(word)).(K)
	case uint64:
		return any(word).(K)
	case uint:
		return any(uint(word)).(K)
	case uintptr:
		return any(uintptr(word)).(K)
	case int8:
		return any(int8(word ^ (1 << 7))).(K)
	case int16:
		return any(int16(word ^ (1 << 15))).(K)
	case int32:
		return any(int32(word ^ (1 << 31))).(K)
	case int64:
		return any(int64(word ^ (1 << 63))).(K)
	case int:
		return any(int(word ^ (1 << (wordBits - 1)))).(K)
	case float32:
		raw := uint32(word)
		if raw>>31 != 0 {
			raw ^= 1 << 31
		} else {
			raw = ^raw
		}
		return any(math.Float32frombits(raw)).(K)
	case float64:
		if word>>63 != 0 {
			word ^= 1 << 63
		} else {
			word = ^word
		}
		return any(math.Float64frombits(word)).(K)
	case bool:
		return any(word != 0).(K)
	case CharKey:
		return any(CharKey{rune(word)}).(K)
	default:
		return decodeNonWord[K](key)
	}
}

// Fixed byte arrays use big-endian digits, with zeroes above the effective width.
func packArrayWord[A byteArray](value A) uint64 {
	var word uint64
	for i := 0; i < len(value); i++ {
		word = word<<8 | uint64(value[i])
	}
	return word
}

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

func encodePartialWord[K any](key K) uint64 {
	switch value := any(key).(type) {
	case [3]byte:
		return packArrayWord(value)
	case [5]byte:
		return packArrayWord(value)
	case [6]byte:
		return packArrayWord(value)
	default:
		return packArrayWord(any(key).([7]byte))
	}
}

func encodeWide[K any](key K) Uint128 {
	switch value := any(key).(type) {
	case Uint128:
		return value
	case Int128:
		return Uint128{Hi: uint64(value.Hi) ^ (1 << 63), Lo: value.Lo}
	case [9]byte:
		return packArray(value)
	case [10]byte:
		return packArray(value)
	case [11]byte:
		return packArray(value)
	case [12]byte:
		return packArray(value)
	case [13]byte:
		return packArray(value)
	case [14]byte:
		return packArray(value)
	case [15]byte:
		return packArray(value)
	case [16]byte:
		return packArray(value)
	default:
		panic("tessera: invalid native key shape")
	}
}

func decodeNonWord[K any](bits Uint128) K {
	var zero K
	switch any(zero).(type) {
	case Int128:
		return any(Int128{Hi: int64(bits.Hi ^ (1 << 63)), Lo: bits.Lo}).(K)
	case [1]byte:
		return any(unpackArray[[1]byte](bits)).(K)
	case [2]byte:
		return any(unpackArray[[2]byte](bits)).(K)
	case [3]byte:
		return any(unpackArray[[3]byte](bits)).(K)
	case [4]byte:
		return any(unpackArray[[4]byte](bits)).(K)
	case [5]byte:
		return any(unpackArray[[5]byte](bits)).(K)
	case [6]byte:
		return any(unpackArray[[6]byte](bits)).(K)
	case [7]byte:
		return any(unpackArray[[7]byte](bits)).(K)
	case [8]byte:
		return any(unpackArray[[8]byte](bits)).(K)
	case [9]byte:
		return any(unpackArray[[9]byte](bits)).(K)
	case [10]byte:
		return any(unpackArray[[10]byte](bits)).(K)
	case [11]byte:
		return any(unpackArray[[11]byte](bits)).(K)
	case [12]byte:
		return any(unpackArray[[12]byte](bits)).(K)
	case [13]byte:
		return any(unpackArray[[13]byte](bits)).(K)
	case [14]byte:
		return any(unpackArray[[14]byte](bits)).(K)
	case [15]byte:
		return any(unpackArray[[15]byte](bits)).(K)
	case [16]byte:
		return any(unpackArray[[16]byte](bits)).(K)
	default:
		panic("tessera: invalid native key shape")
	}
}
