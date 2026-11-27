// SPDX-License-Identifier: BSD-2-Clause

package tessera

import "github.com/Yikai-Liao/tessera-order/go/tessera/internal/engine"

// Sort stably sorts records whose values are supported Tessera keys.
func Sort[K any](records []K) {
	engine.Direct(records, engine.Refine, -1)
}

// SortByKey stably sorts records by key. The callback may run repeatedly
// and its record pointer is valid for the duration of that call. A key panic
// propagates and leaves the slice contents unspecified.
func SortByKey[T, K any](records []T, key func(*T) K) {
	engine.Keyed(records, key, engine.Refine, -1)
}

// SortByCachedKey stably sorts records after extracting each key once in
// input order. Slices with fewer than two records or zero-sized records
// need no extraction.
func SortByCachedKey[T, K any](records []T, key func(*T) K) {
	engine.Keyed(records, key, engine.Cached, -1)
}

// Argsort returns source indices in stable key order.
func Argsort[K any](records []K) []int {
	return engine.Direct(records, engine.Indices, -1)
}

// ArgsortByKey returns source indices in stable key order without rearranging records.
func ArgsortByKey[T, K any](records []T, key func(*T) K) []int {
	return engine.Keyed(records, key, engine.Indices, -1)
}

// SelectNth places the stable rank at index and returns its partition.
// Equal keys keep their relative order throughout the slice.
func SelectNth[K any](records []K, index int) (before []K, nth *K, after []K) {
	checkIndex(len(records), index)
	engine.Direct(records, engine.Refine, index)
	return records[:index:index], &records[index], records[index+1:]
}

// SelectNthByKey places the stable rank at index and returns its partition
// according to key. It validates index before calling key.
func SelectNthByKey[T, K any](records []T, index int, key func(*T) K) (before []T, nth *T, after []T) {
	checkIndex(len(records), index)
	engine.Keyed(records, key, engine.Refine, index)
	return records[:index:index], &records[index], records[index+1:]
}

func checkIndex(length, index int) {
	if index < 0 || index >= length {
		panic("tessera: selection index out of bounds")
	}
}
