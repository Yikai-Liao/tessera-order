// SPDX-License-Identifier: BSD-2-Clause

package engine

import (
	"runtime"
	"slices"
	"sort"
	"sync/atomic"
	"testing"
	"time"
)

func TestParallelShardTail(t *testing.T) {
	type record struct{ key, id int }
	const n = 1349
	rows := make([]record, n)
	for i := range rows {
		rows[i] = record{(i*13 ^ i>>2) & 7, i}
	}
	want := slices.Clone(rows)
	slices.SortStableFunc(want, func(a, b record) int { return a.key - b.key })
	e := extractor[record, uint8]{encode: func(r *record) Uint128 { return Uint128{Lo: uint64(r.key)} }}
	var scratch blockBuffers[record]
	part := make([]bool, 8)
	partitionParallel(rows, 3, &scratch, blockClassifier[uint8]{digit: Digit{0, 3}}, &e, 4, 4, func(_, bucket int, band []record) {
		part[bucket] = true
		first := sort.Search(len(want), func(i int) bool { return want[i].key >= bucket })
		if cap(band) != len(band) || !slices.Equal(band, want[first:first+len(band)]) {
			t.Errorf("bucket %d published an incorrect stable band", bucket)
		}
	})
	if !slices.Equal(rows, want) || slices.Contains(part, false) {
		t.Fatal("shard completion changed the final order or skipped a bucket")
	}
}

func TestParallelWorkerPanicStopsClaimingAndJoins(t *testing.T) {
	before := runtime.NumGoroutine()
	started := make(chan struct{})
	panicking := make(chan struct{})
	var finished atomic.Bool
	first, second := new(int), new(int)
	got := func() (value any) {
		defer func() { value = recover() }()
		runTasks(2, 10, func(_, task int) {
			switch task {
			case 0:
				<-started
				close(panicking)
				panic(first)
			case 1:
				close(started)
				<-panicking
				finished.Store(true)
				panic(second)
			default:
				t.Error("claimed a task after both workers had panicked")
			}
		})
		return
	}()
	if (got != first && got != second) || !finished.Load() {
		t.Fatalf("caller recovered %v, joined=%v", got, finished.Load())
	}
	deadline := time.Now().Add(time.Second)
	for runtime.NumGoroutine() > before && time.Now().Before(deadline) {
		time.Sleep(time.Millisecond)
	}
	if now := runtime.NumGoroutine(); now > before {
		t.Fatalf("goroutines after call = %d, started with %d", now, before)
	}
}

func TestParallelEntryGates(t *testing.T) {
	indexGate := 4 * tileCapacity[int](tileBytes)
	wordBytes := recordBytes[int]()
	indexByteGate := ceilDiv(minParallelBytes, wordBytes)
	for _, probe := range []struct {
		name                    string
		len, recordLen, workers int
		op                      Serial
		fallback                bool
	}{
		{"one worker", minParallelBytes, 1, 1, Refine, true},
		{"one record", 1, minParallelBytes, 4, Refine, true},
		{"zero-sized records", minParallelBytes, 0, 4, Refine, true},
		{"sort below byte gate", minParallelBytes - 1, 1, 4, Refine, true},
		{"sort at byte gate", minParallelBytes, 1, 4, Refine, false},
		{"argsort below tile gate", indexGate - 1, 128, 4, Indices, true},
		{"argsort at tile gate", indexGate, 128, 4, Indices, false},
		{"argsort below byte gate", indexByteGate - 1, 1, 4, Indices, true},
		{"argsort at byte gate via index width", indexByteGate, 1, 4, Indices, false},
	} {
		t.Run(probe.name, func(t *testing.T) {
			if got := useSerial(probe.len, probe.recordLen, probe.op, probe.workers); got != probe.fallback {
				t.Fatalf("serial fallback = %v, want %v", got, probe.fallback)
			}
		})
	}
}

func TestParallelChildTileGate(t *testing.T) {
	type wide [64]byte
	ordinary := tileCapacity[wide](tileBytes)
	limit := finishTileBytes / 64
	if parallelTileCapacity[wide](ordinary-1, ordinary) != ordinary ||
		parallelTileCapacity[wide](limit, ordinary) != limit ||
		parallelTileCapacity[wide](limit+1, ordinary) != ordinary {
		t.Fatal("parallel tile allowance must fit the entire child")
	}
	if parallelTileCapacity[byte](maxTileRecords+1, tileCapacity[byte](tileBytes)) != maxTileRecords {
		t.Fatal("parallel tile allowance exceeded the cursor width")
	}
}

func TestParallelIndexVariationWaves(t *testing.T) {
	const length = 4 * 8192
	const indexMask = length - 1
	const prefix = 1 << 24
	for _, test := range []struct {
		name      string
		firstDiff int
		maxReads  int
	}{
		{"serial probe", 1, 64},
		{"first wave", 65, 64 + 2*8192},
		{"last wave", length - 1, length},
		{"no variation", -1, length},
	} {
		t.Run(test.name, func(t *testing.T) {
			words := make([]int, length)
			for i := range words {
				words[i] = prefix | i
			}
			var reads atomic.Int32
			e := extractor[int, uint64]{encode: func(i *int) Uint128 {
				reads.Add(1)
				if *i == test.firstDiff {
					return Uint128{Lo: 1}
				}
				return Uint128{}
			}}
			got := unmaskAndScanVarying(words, indexMask, Uint128{Lo: 1}, &e, 2)
			if (got.Lo != 0) != (test.firstDiff >= 0) || int(reads.Load()) != test.maxReads {
				t.Fatalf("variation = %v and %d reads, want at most %d reads", got, reads.Load(), test.maxReads)
			}
			for i, word := range words {
				if word != i {
					t.Fatalf("word %d retained prefix: %#x", i, word)
				}
			}
		})
	}
}
