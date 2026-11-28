// SPDX-License-Identifier: BSD-2-Clause

package engine

import (
	"runtime"
	"sync"
)

const (
	// The archived experiment did not reliably win at 512 KiB.
	minParallelBytes = 1 << 20
	// Nearby sizes showed no reliable win in the Rust root task experiment.
	minTaskBytes = 256 * 1024
	// Enlarge the tile only when the entire child fits this byte and cursor budget.
	finishTileBytes = 1024 * 1024
)

func DirectParallel[K any](records []K, op Serial) []int {
	workers := runtime.GOMAXPROCS(0)
	if useSerial(len(records), recordBytes[K](), op, workers) {
		return Direct(records, op, allRanks)
	}
	codec := BindKey[K]()
	return dispatchParallel(records, func(record *K) Uint128 { return codec.Encode(*record) }, codec.Decode, codec.VaryingMask, codec.StorageBits, op, workers)
}

func KeyedParallel[T, K any](records []T, key func(*T) K, op Serial) []int {
	workers := runtime.GOMAXPROCS(0)
	if useSerial(len(records), recordBytes[T](), op, workers) {
		return Keyed(records, key, op, allRanks)
	}
	codec := BindKey[K]()
	return dispatchParallel(records, func(record *T) Uint128 { return codec.Encode(key(record)) }, nil, codec.VaryingMask, codec.StorageBits, op, workers)
}

func useSerial(length, recordLen int, op Serial, workers int) bool {
	minLen := 2
	if op == Indices {
		minLen = 4 * tileCapacity[int](tileBytes)
		recordLen = max(recordLen, recordBytes[int]())
	}
	return workers == 1 || length < minLen || recordLen == 0 ||
		length < ceilDiv(minParallelBytes, recordLen)
}

// Bind the same codec widths in a separate entry to keep goroutine escapes
// out of the serial fast paths.
func dispatchParallel[T any](records []T, encode func(*T) Uint128, decode func(Uint128) T, mask Uint128, width uint, op Serial, workers int) []int {
	switch width {
	case 8:
		return runParallel(records, extractor[T, uint8]{encode: encode, decode: decode, mask: mask}, op, workers)
	case 16:
		return runParallel(records, extractor[T, uint16]{encode: encode, decode: decode, mask: mask}, op, workers)
	case 32:
		return runParallel(records, extractor[T, uint32]{encode: encode, decode: decode, mask: mask}, op, workers)
	case 64:
		return runParallel(records, extractor[T, uint64]{encode: encode, decode: decode, mask: mask}, op, workers)
	default:
		return runParallel(records, extractor[T, Uint128]{encode: encode, decode: decode, mask: mask}, op, workers)
	}
}

func runParallel[T any, V encoded](records []T, e extractor[T, V], op Serial, workers int) []int {
	if op == Indices {
		return argsortParallel(records, e, workers)
	}
	refineParallel(records, e, workers)
	return nil
}

// runTasks joins the entire phase before propagating a worker panic. A worker
// that leaves through runtime.Goexit has not finished its task, so the caller
// leaves the same way after the join, as singleflight does, rather than
// returning a partial result. The queue lock covers claims and cancellation
// and is released before user code.
func runTasks(workers, tasks int, run func(worker, task int)) {
	if min(workers, tasks) <= 1 {
		for task := range tasks {
			run(0, task)
		}
		return
	}

	var group sync.WaitGroup
	var queue sync.Mutex
	var panicValue any
	exited := false
	next := 0
	for worker := range min(workers, tasks) {
		group.Go(func() {
			finished := false
			defer func() {
				value := recover()
				if finished {
					return
				}
				queue.Lock()
				if value == nil {
					exited = true
				} else if panicValue == nil {
					panicValue = value
				}
				queue.Unlock()
			}()
			for {
				queue.Lock()
				if next == tasks || panicValue != nil || exited {
					queue.Unlock()
					finished = true
					return
				}
				task := next
				next++
				queue.Unlock()
				run(worker, task)
			}
		})
	}
	group.Wait()
	if panicValue != nil {
		panic(panicValue)
	}
	if exited {
		runtime.Goexit()
	}
}

func ceilDiv(length, parts int) int {
	if length%parts == 0 {
		return length / parts
	}
	return length/parts + 1
}
