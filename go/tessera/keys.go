// SPDX-License-Identifier: BSD-2-Clause

package tessera

import "github.com/Yikai-Liao/tessera-order/go/tessera/internal/engine"

// Uint128 holds the high and low words of an unsigned 128-bit key.
type Uint128 = engine.Uint128

// Int128 holds the signed high word and unsigned low word of a signed 128-bit key.
type Int128 = engine.Int128

// CharKey orders a Unicode scalar by code point. Its zero value is U+0000.
type CharKey = engine.CharKey

// Char turns a Unicode scalar into a key. It panics for invalid rune values.
func Char(value rune) CharKey {
	return engine.NewChar(value)
}

// ReverseKey orders its value in descending key order.
type ReverseKey[K any] = engine.ReverseKey[K]

// Reverse wraps a value in a descending key.
func Reverse[K any](value K) ReverseKey[K] {
	return ReverseKey[K]{Value: value}
}

// PairKey orders its fields lexicographically.
type PairKey[A, B any] = engine.PairKey[A, B]

// Pair forms a lexicographic key. Nested pairs may have up to 16 fields
// and 128 encoded bits in total.
func Pair[A, B any](first A, second B) PairKey[A, B] {
	return PairKey[A, B]{First: first, Second: second}
}
