// SPDX-License-Identifier: BSD-2-Clause

package engine

import (
	"slices"
	"testing"
)

func TestScanDispatchBoundaries(t *testing.T) {
	t.Run("u8", checkScanBoundaries[uint8, uint8])
	t.Run("u16", checkScanBoundaries[uint16, uint16])
	t.Run("u32", checkScanBoundaries[uint32, uint32])
	t.Run("u64", checkScanBoundaries[uint64, uint64])
	t.Run("uint", checkScanBoundaries[uint, uint64])
	t.Run("i8", checkScanBoundaries[int8, uint8])
	t.Run("i16", checkScanBoundaries[int16, uint16])
	t.Run("i32", checkScanBoundaries[int32, uint32])
	t.Run("i64", checkScanBoundaries[int64, uint64])
	t.Run("int", checkScanBoundaries[int, uint64])
	t.Run("f32", checkScanBoundaries[float32, uint32])
	t.Run("f64", checkScanBoundaries[float64, uint64])
}

func checkScanBoundaries[T any, V encoded](t *testing.T) {
	codec := BindKey[T]()
	e := extractor[T, V]{encode: func(record *T) Uint128 { return codec.Encode(*record) }, decode: codec.Decode}
	lo, hi := codec.Decode(Uint128{}), codec.Decode(codec.VaryingMask)
	var previous V
	// Include a four-vector group after the four-pair peel, covering
	// 0..3*lanes+1 for widths up to 512 bits and the 64-record cutoff.
	maxLen := max(65, 4*64/recordBytes[V]()+6)
	for n := 0; n <= maxLen; n++ {
		records := make([]T, n)
		for i := range records {
			records[i] = hi
		}
		if got := scalarFirstInversion(records, &e); got != -1 || !scalarConfirmAscending(records, &e, previous) {
			t.Fatalf("constant input of length %d failed, inversion %d", n, got)
		}
		for at := n - 2; at >= 0; at-- {
			records[at+1] = lo
			got := scalarFirstInversion(records, &e)
			want := firstInversion(records, &e)
			if got != want || got != at {
				t.Fatalf("length %d, inversion at %d: dispatch %d, scalar %d", n, at, got, want)
			}
			if got, want := scalarConfirmAscending(records, &e, previous), confirmAscending(records, &e, previous); got || got != want {
				t.Fatalf("length %d, inversion at %d: ascending dispatch %v, scalar %v", n, at, got, want)
			}
		}
		if n >= 2 {
			high := fromBits[V](codec.VaryingMask)
			if scalarConfirmAscending(records[1:], &e, high) {
				t.Fatalf("length %d: missed an inversion at the predecessor", n)
			}
		}
	}
}

func TestPrefixSumBoundaries(t *testing.T) {
	for n := 0; n <= 3*32+1; n++ {
		counts := make([]uint16, n)
		want := make([]uint16, n)
		var offset, largest uint16
		for i := range counts {
			count := uint16(i * 137 % 251)
			if i == n-1 {
				count = ^uint16(0) - offset
			}
			counts[i], want[i] = count, offset
			offset += count
			largest = max(largest, count)
		}
		if got := prefixSum(counts); got != largest || !slices.Equal(counts, want) {
			t.Fatalf("length %d: wrong prefix or largest count %d (want %d)", n, got, largest)
		}
	}
}
