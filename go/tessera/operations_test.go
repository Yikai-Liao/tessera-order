// SPDX-License-Identifier: BSD-2-Clause

package tessera_test

import (
	"math"
	"math/rand/v2"
	"slices"
	"testing"

	. "github.com/Yikai-Liao/tessera-order/go/tessera"
)

type testRow struct {
	key     uint64
	id      int
	payload *int
}

func rowKey(row *testRow) uint64 { return row.key }
func compareRows(a, b testRow) int {
	if a.key < b.key {
		return -1
	}
	if a.key > b.key {
		return 1
	}
	return 0
}

func testRows(length int, pattern string) []testRow {
	rng := rand.New(rand.NewPCG(17, 23))
	rows := make([]testRow, length)
	for i := range rows {
		payload := i
		key := rng.Uint64()
		switch pattern {
		case "categories":
			key = key&4095 | 1<<49
		case "repeated":
			if i%5 != 0 {
				key = 1 << 63
			}
		case "ties":
			key = (key&7)<<53 | (key >> 17 & 3)
		case "packed":
			key = key & 0xf000_ffff_0000_03ff
		case "runs":
			key = (uint64(i%29) * 0x10200101) | uint64(1-i/(length/2+1))<<63
		case "sparse":
			key = uint64(i) << 33
			if i%307 == 0 {
				key = rng.Uint64()
			}
		}
		rows[i] = testRow{key: key, id: i, payload: &payload}
	}
	return rows
}

func recoverPanic(action func()) (value any) {
	defer func() { value = recover() }()
	action()
	return
}

func checkRecordOwners(t testing.TB, records, source []testRow) {
	t.Helper()
	seen := make([]bool, len(source))
	for _, row := range records {
		if row.id < 0 || row.id >= len(seen) || seen[row.id] || row.payload != source[row.id].payload {
			t.Fatalf("lost or duplicated record identity %d", row.id)
		}
		seen[row.id] = true
	}
}

func verifySelection(t testing.TB, records, expected []testRow, rank int, before []testRow, nth *testRow, after []testRow) {
	t.Helper()
	if *nth != expected[rank] || len(before) != rank || cap(before) != rank || len(after) != len(records)-rank-1 {
		t.Fatalf("wrong record or view at rank %d", rank)
	}
	// Verify tie order across both regions before comparing their memberships.
	ties := make(map[uint64]int)
	for _, row := range records {
		if id, ok := ties[row.key]; ok && id >= row.id {
			t.Fatalf("rank %d moved equal IDs %d and %d", rank, id, row.id)
		}
		ties[row.key] = row.id
	}
	for _, region := range []struct{ got, want []testRow }{{before, expected[:rank]}, {after, expected[rank+1:]}} {
		left, right := slices.Clone(region.got), slices.Clone(region.want)
		byID := func(a, b testRow) int { return a.id - b.id }
		slices.SortFunc(left, byID)
		slices.SortFunc(right, byID)
		if !slices.Equal(left, right) {
			t.Fatalf("wrong region membership at rank %d", rank)
		}
	}
}

func TestSerialRecordShapesAndRoutes(t *testing.T) {
	for _, test := range []struct {
		name string
		n    int
	}{
		{"random", 17}, {"random", 1309}, {"random", 9002}, {"categories", 4601},
		{"repeated", 12012}, {"ties", 9301}, {"runs", 9301}, {"sparse", 17000}, {"packed", 65549},
	} {
		t.Run(test.name, func(t *testing.T) {
			source := testRows(test.n, test.name)
			expected := slices.Clone(source)
			slices.SortStableFunc(expected, compareRows)
			records := slices.Clone(source)
			SortByKey(records, rowKey)
			if !slices.Equal(records, expected) {
				t.Fatalf("sort does not match stable reference, n=%d", test.n)
			}
			indices := ArgsortByKey(source, rowKey)
			for i, index := range indices {
				if source[index] != expected[i] {
					t.Fatalf("argsort differs at %d", i)
				}
			}
			for _, rank := range []int{0, 1, test.n / 4, test.n / 2, test.n - 2, test.n - 1} {
				copy(records, source)
				before, nth, after := SelectNthByKey(records, rank, rowKey)
				verifySelection(t, records, expected, rank, before, nth, after)
			}
		})
	}
}

func TestDirectNarrowWindowsAndFloatBits(t *testing.T) {
	rng := rand.New(rand.NewPCG(1, 2))
	for _, n := range []int{33, 8192, 8193, 16385, 23013} {
		u32 := make([]uint32, n)
		u64 := make([]uint64, n)
		floats := make([]float64, n)
		for i := range u32 {
			raw := rng.Uint64()
			u32[i] = uint32(raw)
			u64[i] = raw&0xff<<48 | 7
			floats[i] = math.Float64frombits(raw)
		}
		expected32, expected64 := slices.Clone(u32), slices.Clone(u64)
		slices.Sort(expected32)
		slices.Sort(expected64)
		Sort(u32)
		Sort(u64)
		if !slices.Equal(u32, expected32) || !slices.Equal(u64, expected64) {
			t.Fatalf("direct sort n=%d", n)
		}
		expectedFloat := slices.Clone(floats)
		encode := func(v float64) uint64 {
			raw := math.Float64bits(v)
			if raw>>63 != 0 {
				return ^raw
			}
			return raw ^ (1 << 63)
		}
		slices.SortFunc(expectedFloat, func(a, b float64) int {
			x, y := encode(a), encode(b)
			if x < y {
				return -1
			}
			if x > y {
				return 1
			}
			return 0
		})
		Sort(floats)
		for i, value := range floats {
			if math.Float64bits(value) != math.Float64bits(expectedFloat[i]) {
				t.Fatalf("float order n=%d at %d", n, i)
			}
		}
	}
}

func TestDirectWideHalfAndCrossWord(t *testing.T) {
	rng := rand.New(rand.NewPCG(7, 1))
	for _, kind := range []string{"high", "low", "cross"} {
		keys := make([]Uint128, 10513)
		for i := range keys {
			raw := rng.Uint64()
			switch kind {
			case "high":
				keys[i] = Uint128{Hi: (raw&255)<<40 | 31, Lo: 73}
			case "low":
				keys[i] = Uint128{Hi: 19, Lo: raw}
			default:
				keys[i] = Uint128{Hi: raw >> 9, Lo: rng.Uint64()}
			}
		}
		expected := slices.Clone(keys)
		slices.SortFunc(expected, func(a, b Uint128) int {
			if a.Hi < b.Hi || a.Hi == b.Hi && a.Lo < b.Lo {
				return -1
			}
			if a == b {
				return 0
			}
			return 1
		})
		Sort(keys)
		if !slices.Equal(keys, expected) {
			t.Fatalf("wrong %s 128-bit route", kind)
		}
	}
}

func TestCachedEvaluatesExactlyOnceInInputOrder(t *testing.T) {
	for _, pattern := range []string{"categories", "packed", "constant", "ascending"} {
		for _, n := range []int{2, 519, 10021} {
			rows := testRows(n, pattern)
			for i := range rows {
				switch pattern {
				case "constant":
					rows[i].key = 4
				case "ascending":
					rows[i].key = uint64(i / 2)
				}
			}
			expected := slices.Clone(rows)
			slices.SortStableFunc(expected, compareRows)
			calls := 0
			SortByCachedKey(rows, func(row *testRow) uint64 {
				if row.id != calls {
					t.Fatalf("call %d visited record %d", calls, row.id)
				}
				calls++
				return row.key
			})
			if calls != n || !slices.Equal(rows, expected) {
				t.Fatalf("cached %s n=%d calls=%d", pattern, n, calls)
			}
		}
	}
	SortByCachedKey(make([]struct{}, 5), func(*struct{}) uint64 { t.Fatal("key called for zero-sized record"); return 0 })
	SortByCachedKey(make([]testRow, 1), func(*testRow) uint64 { t.Fatal("key called for single record"); return 0 })
}

func TestSelectRejectsIndexBeforeCallingKey(t *testing.T) {
	for _, n := range []int{0, 1, 67} {
		rows := testRows(n, "random")
		original := slices.Clone(rows)
		for _, rank := range []int{-1, n, n + 1} {
			called := false
			p := recoverPanic(func() {
				SelectNthByKey(rows, rank, func(row *testRow) uint64 {
					called = true
					return row.key
				})
			})
			if p != "tessera: selection index out of bounds" || called || !slices.Equal(rows, original) {
				t.Fatalf("index=%d length=%d panic=%v called=%v", rank, n, p, called)
			}
		}
	}
	records := []uint64{3, 1, 2, 3}
	before, nth, after := SelectNth(records, 1)
	before = append(before, 99)
	*nth = 22
	after[0] = 33
	if records[0] != 1 || records[1] != 22 || records[2] != 33 {
		t.Fatal("selection views overlap or lost alias")
	}
}

func TestOrderedAndConstantInPlaceAllocateNothing(t *testing.T) {
	for _, n := range []int{5, 4096, 16384} {
		for _, constant := range []bool{false, true} {
			direct := make([]uint64, n)
			keyed := make([]testRow, n)
			for i := range n {
				if !constant {
					direct[i] = uint64(i / 2)
				}
				keyed[i].key = direct[i]
			}
			type operation struct {
				name string
				run  func()
			}
			ops := []operation{
				{"Sort", func() { Sort(direct) }},
				{"SortByKey", func() { SortByKey(keyed, rowKey) }},
				{"SelectNth", func() { SelectNth(direct, n/2) }},
				{"SelectNthByKey", func() { SelectNthByKey(keyed, n/2, rowKey) }},
			}
			if n == 4096 {
				compact := make([]uint64, n)
				for i := range compact {
					compact[i] = uint64(i)
					if constant {
						compact[i] = 9
					}
				}
				ops = append(ops,
					operation{"Sort compact", func() { Sort(compact) }},
					operation{"SortByKey compact", func() { SortByKey(compact, func(x *uint64) uint64 { return *x }) }},
					operation{"SelectNth compact", func() { SelectNth(compact, n/2) }},
					operation{"ParSort compact", func() { ParSort(compact) }},
				)
			}
			for _, op := range ops {
				if got := testing.AllocsPerRun(20, op.run); got != 0 {
					t.Fatalf("%s len=%d constant=%v allocated %g", op.name, n, constant, got)
				}
			}
		}
	}
}

func TestChangingKeysPreserveOwnersOnReturn(t *testing.T) {
	for _, n := range []int{21, 709, 9102} {
		source := testRows(n, "random")
		for operation := range 3 {
			checkChangingKeys(t, source, operation, n/3)
		}
	}
}

func checkPermutation(t testing.TB, indices []int, n int) {
	t.Helper()
	if len(indices) != n {
		t.Fatalf("permutation has length %d, expected %d", len(indices), n)
	}
	seen := make([]bool, n)
	for _, index := range indices {
		if index < 0 || index >= n || seen[index] {
			t.Fatalf("invalid permutation index %d", index)
		}
		seen[index] = true
	}
}
