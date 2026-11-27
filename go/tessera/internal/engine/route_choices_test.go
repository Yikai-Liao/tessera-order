// SPDX-License-Identifier: BSD-2-Clause

package engine

import (
	"slices"
	"testing"
)

func TestOrderConfirmationPreservesDirectionAndTies(t *testing.T) {
	direct := testDirect[uint64]()
	keyed := testKeyed[uint64, uint64](func(key *uint64) uint64 { return *key })
	for length, combinations := 1, 3; length <= 6; length, combinations = length+1, combinations*3 {
		values := make([]uint64, length)
		for encoded := range combinations {
			for i := range values {
				values[i], encoded = uint64(encoded%3), encoded/3
			}
			want := unordered
			if slices.IsSorted(values) {
				want = ascending
			} else {
				down, strict := true, true
				for i := 1; i < len(values); i++ {
					down = down && values[i] <= values[i-1]
					strict = strict && values[i] < values[i-1]
				}
				if down {
					want = descendingTies
					if strict {
						want = descendingStrict
					}
				}
			}
			if confirmOrder(values, &direct) != want || confirmOrder(values, &keyed) != want {
				t.Fatalf("order for %v = %v, want %v", values, confirmOrder(values, &direct), want)
			}
		}
	}
}

func TestVariationRespectsCandidateAndLateBits(t *testing.T) {
	e := testDirect[Uint128]()
	narrow := testDirect[uint8]()
	for _, length := range []int{1, 64, 65, 192, 193, 1025} {
		keys := make([]Uint128, length)
		bytes := make([]uint8, length)
		for i := range keys {
			keys[i] = Uint128{Lo: 7}
			bytes[i] = 17
		}
		keys[length-1].Hi = 1 << 63
		for _, candidate := range []Uint128{{}, {Lo: 255}, lowMask(128)} {
			want := keys[0].xor(keys[length-1]).and(candidate)
			if got := scanVarying(keys, candidate, &e); got != want {
				t.Fatalf("length %d, candidate %v, got %v want %v", length, candidate, got, want)
			}
		}
		if got := scanVarying(bytes, lowMask(8), &narrow); got != (Uint128{}) {
			t.Fatal(got)
		}
	}
	d := testDirect[uint64]()
	if got := scanVarying([]uint64{0, ^uint64(0)}, lowMask(8), &d); got != lowMask(8) {
		t.Fatal(got)
	}
}

func TestPlannerSamplingCoverage(t *testing.T) {
	e := testDirect[uint64]()
	for _, length := range []int{1, 7, 8, 9, 63, 127, 128, 129, 1024} {
		values := make([]uint64, length)
		for i := range values {
			values[i] = uint64(i * 7919)
		}
		var storage [keySampleLen]uint64
		sample := sampleKeys(values, &e, &storage)
		if len(sample) != min(length, keySampleLen) || length <= keySampleLen && !slices.Equal(sample, values) {
			t.Fatal(length, sample)
		}
		for width := uint(1); width <= maxRadixBits; width++ {
			digit := Digit{24 - width, width}
			buckets := make(map[int]bool)
			for _, key := range sample {
				buckets[keyDigit(key, digit)] = true
			}
			if got := topDigitOccupancy(sample, digit); got != len(buckets) {
				t.Fatalf("length %d width %d got %d", length, width, got)
			}
		}
	}
}

func TestPlannerKeepsInitialWidthForLocalOrderAndConstantBits(t *testing.T) {
	e := testDirect[uint64]()
	values := make([]uint64, 65536)
	for i := range values {
		values[i] = uint64(i)
	}
	var storage [keySampleLen]uint64
	if digit := chooseSplitDigit(len(values), 512, lowMask(16), sampleKeys(values, &e, &storage)); digit.Width != 8 {
		t.Fatalf("locally ordered digit %v", digit)
	}
	const varying = 1<<63 | (1 << 40) - 1
	state := uint64(0x9e3779b97f4a7c15)
	for i := range values {
		state ^= state << 13
		state ^= state >> 7
		state ^= state << 17
		values[i] = state & varying
	}
	if digit := chooseSplitDigit(len(values), 512, Uint128{Lo: varying}, sampleKeys(values, &e, &storage)); digit != (Digit{56, 8}) {
		t.Fatalf("concentrated digit %v", digit)
	}
}

func TestOneSidedCutReadsOnlyPrefixAndOutlier(t *testing.T) {
	const length = 65536
	type row struct {
		id  int
		key uint64
	}
	rows := make([]row, length)
	for i := range rows {
		rows[i].id = i
	}
	rows[2].key = 7
	e := testKeyed[row, uint64](func(r *row) uint64 {
		if r.id > 5 {
			t.Fatalf("one-sided cut scanned past outlier at %d", r.id)
		}
		return r.key
	})
	refine(rows, &e, 4, tileBytes)
	for i, id := range []int{0, 1, 3, 4, 5} {
		if rows[i].id != id {
			t.Fatalf("row %d has id %d", i, rows[i].id)
		}
	}
	next, outliers := 6, 0
	for _, r := range rows[5:] {
		if r.id == 2 {
			if r.key != 7 {
				t.Fatal(r)
			}
			outliers++
			continue
		}
		if r.id != next || r.key != 0 {
			t.Fatal(r, next)
		}
		next++
	}
	if next != length || outliers != 1 {
		t.Fatal(next, outliers)
	}
}

func TestSparseBandsPublishLedgerRanges(t *testing.T) {
	for _, length := range []int{21, 57, 101, 1027} {
		rows := make([]ownedRow, length)
		for i := range rows {
			rows[i] = ownedRow{key: 1, id: i}
			if i%3 == 0 {
				rows[i].key = 5
			}
		}
		expected := slices.Clone(rows)
		slices.SortStableFunc(expected, func(a, b ownedRow) int { return int(a.key) - int(b.key) })
		ends := make([]int, 8)
		for _, r := range rows {
			ends[r.key]++
		}
		for i := 1; i < len(ends); i++ {
			ends[i] += ends[i-1]
		}
		e := testKeyed[ownedRow, uint64](func(r *ownedRow) uint64 { return r.key })
		var buffers blockBuffers[ownedRow]
		var seen []int
		got := partitionBlocks(rows, 3, 7, &buffers, blockClassifier[uint64]{digit: Digit{Width: 3}}, &e, func(bucket, start, end int, band []ownedRow) {
			seen = append(seen, bucket)
			if start != ends[bucket-1] || end != ends[bucket] || !slices.Equal(band, expected[start:end]) {
				t.Fatalf("bucket %d range [%d,%d)", bucket, start, end)
			}
		})
		if !slices.Equal(got, ends) || !slices.Equal(seen, []int{1, 5}) || !slices.Equal(rows, expected) {
			t.Fatalf("length %d ends %v seen %v", length, got, seen)
		}
	}
}

func TestEndpointAndOneSidedCutsPreserveStablePositions(t *testing.T) {
	cases := [][]uint8{{5, 5, 9, 1, 9, 1, 5, 5}, {5, 6, 9, 1, 9, 1, 6, 5}, {1, 6, 3, 7, 3, 4, 3, 6, 3, 9, 3}, {7, 1, 7, 4, 7, 6, 7, 3, 7, 4, 9}}
	for c, keys := range cases {
		ranks := []int{0, len(keys) - 1}
		if c >= 2 {
			ranks = []int{4}
		}
		if c == 3 {
			ranks = []int{6}
		}
		e := testKeyed[int, uint8](func(id *int) uint8 { return keys[*id] })
		compare := func(a, b int) int { return int(keys[a]) - int(keys[b]) }
		for _, rank := range ranks {
			indices := make([]int, len(keys))
			for i := range indices {
				indices[i] = i
			}
			expected := slices.Clone(indices)
			slices.SortStableFunc(expected, compare)
			if !finishCut(indices, rank, &e) || indices[rank] != expected[rank] {
				t.Fatalf("keys %v rank %d output %v", keys, rank, indices)
			}
			slices.SortStableFunc(indices[:rank], compare)
			slices.SortStableFunc(indices[rank+1:], compare)
			if !slices.Equal(indices, expected) {
				t.Fatalf("keys %v rank %d membership %v, expected %v", keys, rank, indices, expected)
			}
		}
	}
}

func TestSpanBoundedEndsAndDisjointWinners(t *testing.T) {
	values := []uint8{6, 1, 4, 4, 2, 0, 5, 1, 6, 0, 3, 0, 1, 5, 4, 2, 3}
	compare := func(a, b int) int { return int(values[a]) - int(values[b]) }
	ids := make([]int, len(values))
	for i := range ids {
		ids[i] = i
	}
	expect := slices.Clone(ids)
	slices.SortStableFunc(expect, compare)
	e := testDirect[uint8]()
	for count := 1; count <= len(values)/2; count++ {
		for _, end := range []sourceRun{{0, count}, {len(values) - count, len(values)}} {
			out := slices.Clone(ids)
			orderedEnd(values, end, &e).writeIndices(out)
			if !slices.Equal(out[end.start:end.end], expect[end.start:end.end]) {
				t.Fatal(end, out)
			}
			slices.SortStableFunc(out[:end.start], compare)
			slices.SortStableFunc(out[end.end:], compare)
			if !slices.Equal(out, expect) {
				t.Fatal(end, out)
			}
		}
	}
	type wide struct {
		key     uint8
		padding [128]byte
	}
	rows := make([]wide, 8)
	for i, key := range []uint8{9, 9, 6, 2, 3, 2, 1, 1} {
		rows[i].key = key
	}
	keyed := testKeyed[wide, uint8](func(r *wide) uint8 { return r.key })
	for _, end := range []sourceRun{{0, 2}, {6, 8}} {
		out := make([]int, len(rows))
		orderedEnd(rows, end, &keyed).writeIndices(out)
		if !slices.Equal(out, []int{6, 7, 2, 3, 4, 5, 0, 1}) {
			t.Fatal(end, out)
		}
	}
}

func TestConfirmationChecksGroupBoundariesAndCallbackOrder(t *testing.T) {
	for length := 0; length <= 17; length++ {
		for inverted := 0; inverted <= length; inverted++ {
			rows := make([]ownedRow, length)
			for i := range rows {
				rows[i] = ownedRow{key: uint64(i/2 + 1), id: i}
				if i == inverted {
					rows[i].key = 0
				}
			}
			for _, boundary := range []uint64{0, 10} {
				want, previous := true, boundary
				for _, row := range rows {
					want = want && previous <= row.key
					previous = row.key
				}
				visited := 0
				e := testKeyed[ownedRow, uint64](func(row *ownedRow) uint64 {
					if row.id != visited {
						t.Fatalf("record %d visited at position %d", row.id, visited)
					}
					visited++
					return row.key
				})
				if got := confirmAscending(rows, &e, boundary); got != want {
					t.Fatalf("length %d inversion %d boundary %d: got %v", length, inverted, boundary, got)
				}
			}
		}
	}
}

func TestSparseExceptionsAndRunsProjectStableMembership(t *testing.T) {
	const length = 193
	type wideRow struct {
		id   *int
		data [1024]byte
	}
	for _, kind := range []string{"ordered", "swapped", "dominant"} {
		keys := make([]uint64, length)
		for i := range keys {
			keys[i] = uint64(i / 3)
			if kind == "dominant" {
				switch {
				case i%11 == 0:
					keys[i] = uint64(i/11) % 2
				case i%7 == 0:
					keys[i] = 3 + uint64(i/7)%2
				default:
					keys[i] = 2
				}
			}
		}
		if kind == "swapped" {
			rotateLeft(keys[:length/2], length/2-1)
			rotateLeft(keys[length/2:], 1)
		}
		compare := func(a, b wideRow) int { return compareKey(keys[*a.id], keys[*b.id]) }
		for _, rank := range []int{allRanks, 0, 5, 18, 93, 167, 180, length - 1} {
			rows := make([]wideRow, length)
			for i := range rows {
				id := i
				rows[i].id = &id
			}
			want := slices.Clone(rows)
			slices.SortStableFunc(want, compare)
			e := testKeyed[wideRow, uint64](func(row *wideRow) uint64 { return keys[*row.id] })
			order := sourceOrderForNode(rows, lowMask(64), rank, &e, recordBytes[wideRow]())
			if order == nil {
				if rank == allRanks || kind == "dominant" {
					t.Fatalf("%s rank %d skipped span route", kind, rank)
				}
				continue
			}
			applySpans(order, rows)
			if rank != allRanks {
				if rows[rank].id != want[rank].id {
					t.Fatalf("%s rank %d: selected %d, want %d", kind, rank, *rows[rank].id, *want[rank].id)
				}
				slices.SortStableFunc(rows[:rank], compare)
				slices.SortStableFunc(rows[rank+1:], compare)
			}
			if !slices.Equal(rows, want) {
				t.Fatalf("%s rank %d: stable membership differs", kind, rank)
			}
		}
	}
}

func TestTileCachesOddSizedRecords(t *testing.T) {
	type odd [3]byte
	values := make([]odd, 65)
	for i := range values {
		key := uint16(i*251) & ((1 << maxTileDigitBits) - 1)
		values[i] = odd{byte(key), byte(key >> 8), byte(i)}
	}
	key := func(row *odd) uint16 { return uint16(row[0]) | uint16(row[1])<<8 }
	expected := slices.Clone(values)
	slices.SortStableFunc(expected, func(a, b odd) int { return int(key(&a)) - int(key(&b)) })
	e := testKeyed[odd, uint16](key)
	var scratch tileScratch[odd]
	var counts [maxDigitBuckets]uint16
	countingPasses(&scratch, values, []Digit{{Width: maxTileDigitBits}}, &counts, &e)
	if !slices.Equal(values, expected) {
		t.Fatal("odd record layout changed stable scatter")
	}
}
