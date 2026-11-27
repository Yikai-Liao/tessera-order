// SPDX-License-Identifier: BSD-2-Clause

//go:build tessera_trace

package engine

import (
	"slices"
	"testing"
)

func observe(t *testing.T) *[]routeEvent {
	t.Helper()
	var events []routeEvent
	routeObserver = func(event routeEvent) { events = append(events, event) }
	t.Cleanup(func() { routeObserver = nil })
	return &events
}

func recorded(events []routeEvent, stage routeStage, value int) bool {
	for _, event := range events {
		if event.stage == stage && event.a == value {
			return true
		}
	}
	return false
}

func TestObservedOrderAndVariation(t *testing.T) {
	events := observe(t)
	for _, test := range []struct {
		keys     []uint64
		want     order
		constant bool
	}{
		{[]uint64{1, 2, 3, 4}, ascending, false},
		{[]uint64{4, 3, 2, 1}, descendingStrict, false},
		{[]uint64{4, 4, 2, 1}, descendingTies, false},
		{[]uint64{3, 3, 3, 3}, ascending, true},
	} {
		*events = (*events)[:0]
		Direct(slices.Clone(test.keys), Refine, -1)
		if !recorded(*events, routeProbe, int(test.want)) || recorded(*events, routeConstant, len(test.keys)) != test.constant {
			t.Fatalf("probe for %v: %v", test.keys, *events)
		}
	}
	values := make([]uint64, orderSampleMinLen)
	for i := range values {
		values[i] = uint64(i)
	}
	mid := len(values)/2 + 1
	values[mid], values[mid+1] = values[mid+1], values[mid]
	e := testDirect[uint64]()
	if got := probeOrder(values, &e); got != unordered {
		t.Fatalf("unsampled inversion reported as %v", got)
	}
	values = make([]uint64, 3*orderSampleMinLen)
	values[4] = 1
	*events = (*events)[:0]
	var scratch workspace[uint64]
	sortChild(values, lowMask(64), -1, tileCapacity[uint64](tileBytes), &scratch, &e)
	if !recorded(*events, routeProbe, int(flatSample)) || !recorded(*events, routeProbe, int(unordered)) || !slices.IsSorted(values) {
		t.Fatal("flat sample hid an inversion")
	}
}

func TestObservedTilesBlocksAndNarrowing(t *testing.T) {
	events := observe(t)
	keys := make([]uint64, 257)
	for i := range keys {
		keys[i] = uint64(i) * 0x9e3779b97f4a7c15
	}
	direct := testDirect[uint64]()
	refine(slices.Clone(keys), &direct, -1, tileBytes)
	if !recorded(*events, routeTile, len(keys)) || !recorded(*events, routeTileDigits, 0) {
		t.Fatal(*events)
	}
	*events = (*events)[:0]
	sortNode(slices.Clone(keys[:101]), lowMask(64), -1, 64, &workspace[uint64]{}, &direct)
	if !recorded(*events, routeBlock, 101) || !recorded(*events, routeNodeDigit, 0) {
		t.Fatal(*events)
	}

	for _, high := range []bool{false, true} {
		keys := make([]Uint128, 127)
		for i := range keys {
			digit := uint64((i * 11) % 16)
			keys[i] = Uint128{Hi: 17, Lo: 71}
			if high {
				keys[i].Hi |= digit << 24
			} else {
				keys[i].Lo |= digit << 24
			}
		}
		*events = (*events)[:0]
		Direct(keys, Refine, -1)
		offset := 0
		if high {
			offset = 64
		}
		if !recorded(*events, routeNarrow, offset) || !recorded(*events, routeCountWindow, len(keys)) {
			t.Fatal(*events)
		}
	}
}

func TestObservedCachedAndIndexRoutes(t *testing.T) {
	events := observe(t)
	for _, tc := range []struct{ n, cached, distribution int }{
		{80, 0, -1}, {80, 1, 0}, {8193, 1, 1}, {80, 2, -1},
	} {
		*events = (*events)[:0]
		values := make([]uint64, tc.n)
		calls := 0
		Keyed(values, func(*uint64) uint64 {
			i := calls
			calls++
			switch tc.cached {
			case 0:
				return uint64(i)
			case 1:
				return uint64((i*13)%16) << 33
			default:
				return uint64(i) * 0x9e3779b97f4a7c15
			}
		}, Cached, -1)
		if calls != tc.n || !recorded(*events, routeCached, tc.cached) || tc.distribution >= 0 && !recorded(*events, routeDistribution, tc.distribution) {
			t.Fatalf("cached case %v, %d calls", tc, calls)
		}
	}

	for _, route := range []int{0, 1, 2, 3} {
		n := 8193
		if route == 1 {
			n = 191
		}
		values := make([]uint64, n)
		for i := range values {
			switch route {
			case 0, 2:
				values[i] = uint64(i)
			default:
				values[i] = uint64(i) * 0x9e3779b97f4a7c15
			}
		}
		if route == 2 {
			values[8], values[n-2] = values[n-2], values[8]
		}
		*events = (*events)[:0]
		Direct(values, Indices, -1)
		if !recorded(*events, routeIndices, route) {
			t.Fatalf("wanted index route %d", route)
		}
	}
}

func TestTraceEveryRecordScattersOnceAndTransfersIntoItsBand(t *testing.T) {
	events := observe(t)
	rows := make([]uint32, 41)
	for i := range rows {
		rows[i] = uint32((i*37 + 11) % 40)
	}
	expected := slices.Clone(rows)
	slices.SortStableFunc(expected, func(a, b uint32) int { return int(a/10) - int(b/10) })
	e := testKeyed[uint32, uint32](func(row *uint32) uint32 { return *row / 10 })
	var buffers blockBuffers[uint32]
	partitionBlocks(rows, 2, 3, &buffers, blockClassifier[uint32]{digit: Digit{Width: 2}}, &e, nil)

	sourceLengths := make([]int, 0)
	scattered, transferred, bands := 0, 0, 0
	for _, event := range *events {
		switch event.stage {
		case routeScatter:
			scattered++
		case routeSource:
			if len(sourceLengths) != event.a {
				t.Fatal("source IDs must be contiguous")
			}
			sourceLengths = append(sourceLengths, event.b)
		case routeTransfer:
			transferred += sourceLengths[event.a]
		case routeBand:
			bands += event.b
		}
	}
	if scattered != len(rows) || transferred != len(rows) || bands != len(rows) || !slices.Equal(rows, expected) {
		t.Fatalf("scatter, transfer, band totals = %d, %d, %d", scattered, transferred, bands)
	}
}

func TestRankPrunesSkippedBands(t *testing.T) {
	events := observe(t)
	const n = 193
	for _, tile := range []int{2, n / 3 * 2} {
		*events = (*events)[:0]
		rows := make([]ownedRow, n)
		seen := make([]bool, n)
		for i := range rows {
			rows[i] = ownedRow{key: uint64(i) * 0x9e3779b97f4a7c15 >> 1, id: i}
			if i%3 != 0 {
				rows[i].key |= 1 << 63
			}
		}
		e := testKeyed[ownedRow, uint64](func(row *ownedRow) uint64 {
			if seen[row.id] && row.key>>63 != 0 {
				t.Fatalf("skipped record %d was analyzed", row.id)
			}
			seen[row.id] = true
			return row.key
		})
		var scratch workspace[ownedRow]
		executeNode(rows, lowMask(64), 2, tile, &scratch, &e, nodePlan[uint64]{digit: Digit{63, 1}})
		if !recorded(*events, routeRank, 2) {
			t.Fatal("rank was not projected")
		}
	}
}

func TestRankPrunesFinishedEqualityBand(t *testing.T) {
	events := observe(t)
	const n = 193
	const equal = uint64(1) << 63
	rows := make([]ownedRow, n)
	calls := make([]bool, n)
	less, same := 0, 0
	for i := range rows {
		key := equal
		switch i % 5 {
		case 0:
			key = uint64(i)
			less++
		case 1:
			key += uint64(i + 1)
		default:
			same++
		}
		rows[i] = ownedRow{key: key, id: i}
	}
	e := testKeyed[ownedRow, uint64](func(row *ownedRow) uint64 {
		if calls[row.id] {
			t.Fatalf("completed or unrequested record %d was analyzed", row.id)
		}
		calls[row.id] = true
		return row.key
	})
	var scratch workspace[ownedRow]
	rank := less + same/2
	executeNode(rows, lowMask(64), rank, 2, &scratch, &e, nodePlan[uint64]{mode: true, equal: equal})
	if !recorded(*events, routeNodeDigit, 1) || rows[rank].key != equal {
		t.Fatal("wrong equality route")
	}
}

func TestRankProjectsAfterRunRotation(t *testing.T) {
	events := observe(t)
	const n = 193
	high, low := n/3, n-n/3
	for _, rank := range []int{2, low - 1, low, n - 1} {
		*events = (*events)[:0]
		rows := make([]ownedRow, n)
		for i := range rows {
			rows[i] = ownedRow{key: uint64(i%19) * 0x9e3779b97f4a7c15 >> 1, id: i}
			if i < high {
				rows[i].key |= 1 << 63
			}
		}
		e := testKeyed[ownedRow, uint64](func(row *ownedRow) uint64 {
			if (rank < low) != (row.id >= high) {
				t.Fatalf("unrequested run was analyzed at %d", rank)
			}
			return row.key
		})
		var scratch workspace[ownedRow]
		executeNode(rows, lowMask(64), rank, 2, &scratch, &e, nodePlan[uint64]{boundary: high, set: true})
		seen := false
		for _, event := range *events {
			if event.stage == routeRank && event.a == rank && event.b == low && event.c == n {
				seen = true
			}
		}
		if !seen {
			t.Fatal("rank was not projected in post-rotation coordinates")
		}
	}
}
