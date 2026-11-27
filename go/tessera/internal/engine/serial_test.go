// SPDX-License-Identifier: BSD-2-Clause

package engine

import (
	"reflect"
	"slices"
	"testing"
)

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
	e := extractor[uint64, uint64, uint64, uint64, directMode]{}
	if _, ok := insertionLoop(values, 1, &e); ok || !slices.Equal(values, []uint64{1, 0, 2, 3}) {
		t.Fatalf("budget exit = %v", values)
	}
}

func TestTileCachesChangingClassifications(t *testing.T) {
	rows := rowsForRecovery(59)
	original := slices.Clone(rows)
	calls := 0
	e := extractor[ownedRow, ownedRow, uint64, uint64, keyedMode]{key: func(r *ownedRow) uint64 {
		calls++
		return uint64(calls * 7)
	}}
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
		rows := rowsForRecovery(length)
		expected := slices.Clone(rows)
		slices.SortStableFunc(expected, func(a, b ownedRow) int { return int(a.key&7) - int(b.key&7) })
		e := extractor[ownedRow, ownedRow, uint64, uint64, keyedMode]{key: func(r *ownedRow) uint64 { return r.key }}
		var buffers blockBuffers[ownedRow]
		start := 0
		ends := partitionBlocks(rows, 3, 7, &buffers, blockClassifier[uint64]{digit: Digit{Width: 3}}, &e,
			func(bucket, lo, hi int, band []ownedRow) {
				if lo != start || !slices.Equal(band, expected[lo:hi]) {
					t.Fatalf("published bucket %d", bucket)
				}
				start = hi
			})
		if !slices.Equal(rows, expected) || ends[7] != len(rows) {
			t.Fatal("wrong block distribution")
		}
	}
}

func TestNodeRoutes(t *testing.T) {
	e := extractor[uint64, uint64, uint64, uint64, directMode]{}
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
			plan := planDigits(Uint128{Lo: varying}, width, maxInt, false)
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
	e := extractor[uint64, uint64, uint64, uint64, keyedMode]{key: func(key *uint64) uint64 {
		calls++
		if calls > len(keys) {
			return 1
		}
		return *key
	}}
	p := caught(func() { scatterIndices(keys, indices, &e, 0, 0, 0, false) })
	if p != "tessera: inconsistent radix partition" {
		t.Fatalf("panic = %v", p)
	}
	if !reflect.DeepEqual(keys, []uint64{0, 1, 1, 0}) {
		t.Fatal("scatter moved source")
	}
}
