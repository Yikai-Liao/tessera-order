// SPDX-License-Identifier: BSD-2-Clause

package tessera_test

import (
	"fmt"
	"math/bits"
	"math/rand/v2"
	"runtime"
	"slices"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	. "github.com/Yikai-Liao/tessera-order/go/tessera"
)

func checkParallelKeys[K comparable](t *testing.T, keys []K, same func(K, K) bool) {
	t.Helper()
	// Exercise each codec through an eligible by-key root. A large record
	// crosses the byte gate with relatively few keys.
	type row struct {
		id int
		_  [128]byte
	}
	const length = 1 << 20 / 128
	rows := make([]row, length)
	rng := rand.New(rand.NewPCG(51, 6))
	selected := make([]K, length)
	for i := range rows {
		selected[i] = keys[rng.IntN(len(keys))]
		rows[i].id = i
	}
	key := func(r *row) K { return selected[r.id] }
	serial := slices.Clone(rows)
	SortByKey(serial, key)
	ParSortByKey(rows, key)
	if !slices.Equal(rows, serial) {
		t.Fatal("parallel keyed sort changed the golden key order")
	}
	serialKeys := slices.Clone(selected)
	Sort(serialKeys)
	ParSort(selected)
	if !slices.EqualFunc(selected, serialKeys, same) {
		t.Fatal("parallel direct sort changed the golden key order")
	}
}

func TestParallelMatchesSerial(t *testing.T) {
	const minBytes = 1 << 20
	// testRow is three machine words and carries a pointer into user data.
	rowBytes := 3 * (bits.UintSize / 8)
	gate := (minBytes + rowBytes - 1) / rowBytes
	for _, workers := range []int{1, 2, 4} {
		t.Run(fmt.Sprint(workers), func(t *testing.T) {
			previous := runtime.GOMAXPROCS(workers)
			t.Cleanup(func() { runtime.GOMAXPROCS(previous) })
			for _, test := range []struct {
				pattern string
				length  int
			}{
				{"random", gate - 1}, {"random", gate}, {"random", gate + 137},
				{"sorted", gate}, {"reversed", gate}, {"constant", gate},
				{"categories", gate}, {"repeated", gate}, {"ties", gate},
			} {
				t.Run(fmt.Sprintf("%s/%d", test.pattern, test.length), func(t *testing.T) {
					rows := testRows(test.length, test.pattern)
					for i := range rows {
						switch test.pattern {
						case "sorted":
							rows[i].key = uint64(i / 3)
						case "reversed":
							rows[i].key = uint64((len(rows) - i) / 3)
						case "constant":
							rows[i].key = 8
						}
					}
					checkParallelRows(t, rows)
				})
			}
		})
	}
}

func checkParallelRows(t testing.TB, rows []testRow) {
	t.Helper()
	ordered := slices.Clone(rows)
	actual := slices.Clone(rows)
	SortByKey(ordered, rowKey)
	ParSortByKey(actual, rowKey)
	if !slices.Equal(actual, ordered) {
		t.Fatal("ParSortByKey differs from SortByKey")
	}
	got, want := ParArgsortByKey(rows, rowKey), ArgsortByKey(rows, rowKey)
	if !slices.Equal(got, want) {
		t.Fatal("ParArgsortByKey differs from ArgsortByKey")
	}
}

func TestParallelDirectWideKeys(t *testing.T) {
	source := testRows((1<<20)/16+19, "random")
	keys := make([]Uint128, len(source))
	for _, half := range []string{"low", "high", "both"} {
		t.Run(half, func(t *testing.T) {
			for i, row := range source {
				switch half {
				case "low":
					keys[i] = Uint128{Hi: 43, Lo: row.key & 0x1b}
				case "high":
					keys[i] = Uint128{Hi: row.key & 0x1b, Lo: 43}
				case "both":
					keys[i] = Uint128{Hi: row.key & 7, Lo: row.key >> 3}
				}
			}
			got, want := slices.Clone(keys), slices.Clone(keys)
			ParSort(got)
			Sort(want)
			if !slices.Equal(got, want) || !slices.Equal(ParArgsort(keys), Argsort(keys)) {
				t.Fatal("parallel wide keys differ from serial")
			}
		})
	}
}

func TestParallelKeyPanicAndReentry(t *testing.T) {
	previous := runtime.GOMAXPROCS(4)
	t.Cleanup(func() { runtime.GOMAXPROCS(previous) })
	rows := testRows(1<<16, "random")
	// Random full-width keys end the serial variation scan near the start.
	// These IDs also avoid the order probe's adjacent pairs. Each belongs
	// to a different packing task.
	target := func(id int) bool { return id%(len(rows)/4) == 313 }
	t.Run("worker panic", func(t *testing.T) {
		before := runtime.NumGoroutine()
		token := new(int)
		var active atomic.Int32
		got := recoverPanic(func() {
			ParArgsortByKey(rows, func(row *testRow) uint64 {
				active.Add(1)
				defer active.Add(-1)
				if target(row.id) {
					panic(token)
				}
				return row.key
			})
		})
		if got != token || active.Load() != 0 {
			t.Fatalf("got panic %v and %d active callbacks", got, active.Load())
		}
		deadline := time.Now().Add(time.Second)
		for runtime.NumGoroutine() > before && time.Now().Before(deadline) {
			time.Sleep(time.Millisecond)
		}
		if now := runtime.NumGoroutine(); now > before {
			t.Fatalf("goroutines after call = %d, started with %d", now, before)
		}
	})
	t.Run("all workers reenter", func(t *testing.T) {
		var workers sync.WaitGroup
		workers.Add(4)
		var visits [4]atomic.Bool
		got := ParArgsortByKey(rows, func(row *testRow) uint64 {
			if target(row.id) && visits[row.id/(len(rows)/4)].CompareAndSwap(false, true) {
				workers.Done()
				workers.Wait()
				nested := make([]uint32, (1<<20)/4)
				for i := range nested {
					nested[i] = uint32(i*17^i>>4) & 127
				}
				ParSort(nested)
				if !slices.IsSorted(nested) || !slices.Equal(Argsort([]int{2, 1}), []int{1, 0}) {
					t.Error("reentrant sort failed")
				}
			}
			return row.key
		})
		if !slices.Equal(got, ArgsortByKey(rows, rowKey)) {
			t.Fatal("reentry changed the outer argsort")
		}
	})
}

func TestParallelChangingKeysKeepOwners(t *testing.T) {
	source := testRows(1<<16, "random")
	var calls atomic.Uint64
	key := func(row *testRow) uint64 {
		count := calls.Add(1)
		if count > uint64(len(source))*100 {
			panic("unbounded changing-key refinement")
		}
		return row.key ^ count*0x9e3779b97f4a7c15
	}
	rows := slices.Clone(source)
	ParSortByKey(rows, key)
	checkRecordOwners(t, rows, source)
	calls.Store(0)
	checkPermutation(t, ParArgsortByKey(source, key), len(source))
}

func FuzzParallelOrdering(f *testing.F) {
	for i, seed := range goldenSeeds(f) {
		f.Add(seed, uint8(i))
	}
	f.Add([]byte{1, 3, 2, 7, 5, 6}, uint8(255))
	f.Fuzz(func(t *testing.T, data []byte, scale uint8) {
		rows := fuzzRows(data, scale)
		if len(rows) != 0 && scale == 255 {
			// Cross both public gates with a short seed and many prefix ties.
			source := rows
			rows = make([]testRow, 1<<16)
			for i := range rows {
				rows[i] = source[i%len(source)]
				rows[i].id = i
				rows[i].key ^= uint64(i) * 0x100100001
			}
		}
		checkParallelRows(t, rows)
	})
}

func ExampleParSort() {
	keys := []int{2, 1, 2}
	ParSort(keys)
	fmt.Println(keys)
	// Output: [1 2 2]
}

func ExampleParSortByKey() {
	rows := []string{"pear", "fig", "plum"}
	ParSortByKey(rows, func(row *string) int { return len(*row) })
	fmt.Println(rows)
	// Output: [fig pear plum]
}

func ExampleParArgsort() {
	fmt.Println(ParArgsort([]int{2, 1, 2}))
	// Output: [1 0 2]
}

func ExampleParArgsortByKey() {
	rows := []string{"pear", "fig", "plum"}
	fmt.Println(ParArgsortByKey(rows, func(row *string) int { return len(*row) }))
	// Output: [1 0 2]
}
