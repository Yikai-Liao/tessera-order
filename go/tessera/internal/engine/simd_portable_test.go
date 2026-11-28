// SPDX-License-Identifier: BSD-2-Clause

//go:build goexperiment.simd

package engine

import "testing"

func TestNativeScanEligibility(t *testing.T) {
	const cutoff = 64
	values := make([]uint64, cutoff)
	if _, used := nativeFirstInversion(values[:cutoff-1], true); used {
		t.Fatal("native scan accepted a short slice")
	}
	if _, used := nativeFirstInversion(values, true); !used {
		t.Fatal("native scan declined at the minimum length")
	}
	if _, used := nativeFirstInversion(values, false); used {
		t.Fatal("native scan accepted a user-key slice")
	}
	if _, used := nativeFirstInversion(make([]float64, cutoff), true); used {
		t.Fatal("native scan accepted a floating-point slice")
	}
	if _, used := nativeFirstInversion(make([]int, cutoff), true); used {
		t.Fatal("native scan accepted a machine-word slice")
	}
	if _, used := nativeFirstInversion(make([]Uint128, cutoff), true); used {
		t.Fatal("native scan accepted a wide-key slice")
	}
}
