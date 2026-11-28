// SPDX-License-Identifier: BSD-2-Clause

package engine

import (
	"reflect"
	"slices"
	"testing"
)

func testDirect[V encoded]() extractor[V, V] {
	codec := BindKey[V]()
	return extractor[V, V]{encode: func(value *V) Uint128 { return codec.Encode(*value) }, decode: codec.Decode, mask: codec.VaryingMask}
}

func testKeyed[T any, V encoded](key func(*T) V) extractor[T, V] {
	codec := BindKey[V]()
	return extractor[T, V]{encode: func(value *T) Uint128 { return codec.Encode(key(value)) }, mask: codec.VaryingMask}
}

func caught(action func()) (value any) {
	defer func() { value = recover() }()
	action()
	return
}

type ownedRow struct {
	key     uint64
	id      int
	payload *int
}

func rowsForRecovery(length int) []ownedRow {
	rows := make([]ownedRow, length)
	for i := range rows {
		value := i
		rows[i] = ownedRow{key: uint64(i % 13), id: i, payload: &value}
	}
	return rows
}

func checkOwners(t *testing.T, records, original []ownedRow) {
	t.Helper()
	seen := make([]bool, len(records))
	for _, record := range records {
		if record.id < 0 || record.id >= len(seen) || seen[record.id] || record.payload != original[record.id].payload {
			t.Fatalf("lost or duplicated owner %d", record.id)
		}
		seen[record.id] = true
	}
}

func TestInsertionKeepsPermutationOnBudgetExit(t *testing.T) {
	values := []uint64{1, 2, 3, 0}
	e := testDirect[uint64]()
	if _, ok := insertionLoop(values, 1, &e); ok || !slices.Equal(values, []uint64{1, 0, 2, 3}) {
		t.Fatalf("budget exit = %v", values)
	}
}

func TestTileCachesChangingClassifications(t *testing.T) {
	rows := rowsForRecovery(59)
	original := slices.Clone(rows)
	calls := 0
	e := testKeyed[ownedRow, uint64](func(r *ownedRow) uint64 {
		calls++
		return uint64(calls * 7)
	})
	var scratch tileScratch[ownedRow]
	var counts [maxDigitBuckets]uint16
	countingPasses(&scratch, rows, []Digit{{Width: 4}}, &counts, &e)
	if calls != len(rows) {
		t.Fatalf("classifier called %d times", calls)
	}
	checkOwners(t, rows, original)
	expected := slices.Clone(original)
	slices.SortStableFunc(expected, func(a, b ownedRow) int { return ((a.id + 1) * 7 & 15) - ((b.id + 1) * 7 & 15) })
	if !slices.Equal(rows, expected) {
		t.Fatal("tile did not preserve cached classification order")
	}
}

func TestBlockReturnsStableBandEnds(t *testing.T) {
	for _, length := range []int{2, 7, 8, 57, 101, 123} {
		checkBlockBands(t, rowsForRecovery(length))
	}
	for _, length := range []int{21, 57, 101, 1027} {
		rows := make([]ownedRow, length)
		for i := range rows {
			rows[i] = ownedRow{key: 1, id: i}
			if i%3 == 0 {
				rows[i].key = 5
			}
		}
		checkBlockBands(t, rows)
	}
}

func checkBlockBands(t *testing.T, rows []ownedRow) {
	t.Helper()
	bucketOf := func(r ownedRow) int { return int(r.key & 7) }
	expected := slices.Clone(rows)
	slices.SortStableFunc(expected, func(a, b ownedRow) int { return bucketOf(a) - bucketOf(b) })
	want := make([]int, 8)
	for _, r := range rows {
		want[bucketOf(r)]++
	}
	for i := 1; i < len(want); i++ {
		want[i] += want[i-1]
	}
	e := testKeyed[ownedRow, uint64](func(r *ownedRow) uint64 { return r.key })
	var buffers blockBuffers[ownedRow]
	seen := make([]int, 0, len(want))
	got := partitionBlocks(rows, 3, 7, &buffers, blockClassifier[uint64]{digit: Digit{Width: 3}}, &e,
		func(bucket, start, end int, band []ownedRow) {
			seen = append(seen, bucket)
			if start != want[bucket]-len(band) || end != want[bucket] || !slices.Equal(band, expected[start:end]) {
				t.Fatalf("bucket %d range [%d,%d)", bucket, start, end)
			}
		})
	var nonempty []int
	start := 0
	for bucket, end := range want {
		if end > start {
			nonempty = append(nonempty, bucket)
		}
		start = end
	}
	if !slices.Equal(got, want) || !slices.Equal(seen, nonempty) || !slices.Equal(rows, expected) {
		t.Fatalf("length %d ends %v seen %v", len(rows), got, seen)
	}
}

func TestNodeRoutes(t *testing.T) {
	e := testDirect[uint64]()
	var rows [512]uint64
	for i := range rows {
		rows[i] = uint64(512 - i)
	}
	varying := Uint128{Lo: 1023}
	if plan := planNode(rows[:], varying, 256, &e); plan.boundary != 1 || !plan.set {
		t.Fatalf("ordered run route = %+v", plan)
	}
	for i := range rows {
		rows[i] = 3
		if i%2 == 0 {
			rows[i] = uint64(i) * 307
		}
	}
	if plan := planNode(rows[:], varying, 64, &e); !plan.mode || plan.equal != 3 {
		t.Fatalf("equality route = %+v", plan)
	}
	if plan := planNode(rows[:], Uint128{Lo: 0x700}, 64, &e); plan.digit != (Digit{8, 3}) {
		t.Fatalf("complete digit route = %+v", plan)
	}
}

func TestTileDigitPlansMatchExhaustiveOracle(t *testing.T) {
	var optimum func(uint64, uint) (int, int)
	optimum = func(varying uint64, width uint) (int, int) {
		if varying == 0 {
			return 0, 0
		}
		best, slots := maxInt, maxInt
		hi := uint((Uint128{Lo: varying}).bitLen())
		for span := uint(1); span <= min(width, hi); span++ {
			shift := hi - span
			passes, cost := optimum(varying&(uint64(1)<<shift-1), width)
			passes++
			cost += 1 << span
			if passes < best || passes == best && cost < slots {
				best, slots = passes, cost
			}
		}
		return best, slots
	}
	for varying := uint64(1); varying < 256; varying++ {
		for width := uint(1); width <= 5; width++ {
			passes, slots := optimum(varying, width)
			plan := planDigits(Uint128{Lo: varying}, width)
			if plan.msd {
				if passes <= 2 {
					t.Fatalf("unexpected MSD for %x width %d", varying, width)
				}
				continue
			}
			cost := 0
			var mask Uint128
			for _, digit := range plan.digits[:plan.passes] {
				cost += 1 << digit.Width
				if mask.and(digit.Mask()) != (Uint128{}) {
					t.Fatal("overlapping digits")
				}
				mask = mask.or(digit.Mask())
			}
			if plan.passes != passes || cost != slots || mask.Lo&varying != varying {
				t.Fatalf("digits for %x width %d = %+v, expected %d passes and %d slots", varying, width, plan, passes, slots)
			}
		}
	}
}

func TestIndexScatterRejectsChangingCounts(t *testing.T) {
	keys := []uint64{0, 1, 1, 0}
	indices := make([]int, len(keys))
	calls := 0
	e := testKeyed[uint64, uint64](func(key *uint64) uint64 {
		calls++
		if calls > len(keys) {
			return 1
		}
		return *key
	})
	p := caught(func() { scatterIndices(keys, indices, &e, 0, 0, 0, false) })
	if p != "tessera: inconsistent radix partition" {
		t.Fatalf("panic = %v", p)
	}
	if !reflect.DeepEqual(keys, []uint64{0, 1, 1, 0}) {
		t.Fatal("scatter moved source")
	}
}
