// SPDX-License-Identifier: BSD-2-Clause

package engine

import "testing"

func TestUint128Shifts(t *testing.T) {
	value := Uint128{Hi: 1 << 63, Lo: 3}
	for _, tc := range []struct {
		shift       uint
		left, right Uint128
	}{
		{0, value, value},
		{63, Uint128{Hi: 1, Lo: 1 << 63}, Uint128{Hi: 1, Lo: 0}},
		{64, Uint128{Hi: 3}, Uint128{Lo: 1 << 63}},
		{65, Uint128{Hi: 6}, Uint128{Lo: 1 << 62}},
		{127, Uint128{Hi: 1 << 63}, Uint128{Lo: 1}},
		{128, Uint128{}, Uint128{}},
	} {
		if got := value.lsh(tc.shift); got != tc.left {
			t.Errorf("left shift %d = %#v, want %#v", tc.shift, got, tc.left)
		}
		if got := value.rsh(tc.shift); got != tc.right {
			t.Errorf("right shift %d = %#v, want %#v", tc.shift, got, tc.right)
		}
	}
	for _, tc := range []struct {
		value Uint128
		width int
	}{
		{Uint128{}, 0},
		{Uint128{Lo: 1}, 1},
		{Uint128{Lo: 1 << 63}, 64},
		{Uint128{Hi: 1}, 65},
		{value, 128},
	} {
		if got := tc.value.bitLen(); got != tc.width {
			t.Errorf("bit length of %#v = %d, want %d", tc.value, got, tc.width)
		}
	}
}

func TestKeyDigitCrossesWordBoundary(t *testing.T) {
	digit := Digit{Shift: 60, Width: 8}
	if got := digit.Mask(); got != (Uint128{Hi: 0xf, Lo: 0xf000_0000_0000_0000}) {
		t.Fatalf("digit mask = %#v", got)
	}
	key := Uint128{Hi: 0xc7, Lo: 0xb000_0000_0000_0099}
	if got := digit.Bucket(key); got != 0x7b {
		t.Fatalf("digit bucket = %#x, want 0x7b", got)
	}
}
