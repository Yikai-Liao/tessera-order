// SPDX-License-Identifier: BSD-2-Clause

package engine

import "testing"

func TestEncodingConstants(t *testing.T) {
	t.Run("signed", func(t *testing.T) {
		codec := BindKey[int64]()
		if got := codec.Encode(-1); got != (Uint128{Lo: 0x7fff_ffff_ffff_ffff}) {
			t.Fatalf("encoded -1 as %#v", got)
		}
	})
	t.Run("float", func(t *testing.T) {
		codec := BindKey[float64]()
		if got := codec.Encode(-1); got != (Uint128{Lo: 0x400f_ffff_ffff_ffff}) {
			t.Fatalf("encoded -1 as %#v", got)
		}
	})
	t.Run("reverse", func(t *testing.T) {
		codec := BindKey[ReverseKey[uint16]]()
		if got := codec.Encode(ReverseKey[uint16]{Value: 1}); got != (Uint128{Lo: 0xfffe}) {
			t.Fatalf("encoded Reverse(1) as %#v", got)
		}
	})
}
