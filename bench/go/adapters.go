// SPDX-License-Identifier: BSD-2-Clause

package main

import (
	"cmp"
	"math/bits"
	"slices"
	"sort"

	"github.com/Yikai-Liao/tessera-order/go/tessera"
	"github.com/klauspost/radsort"
	"github.com/zhangyunhao116/pdqsort"
)

type directCalls[T any] struct {
	sort, parSort       func([]T)
	argsort, parArgsort func([]T) []int
	selectNth           func([]T, int)
}

type comparison[T any] struct {
	values []T
	cmp    func(T, T) int
}

func (s comparison[T]) Len() int           { return len(s.values) }
func (s comparison[T]) Less(i, j int) bool { return s.cmp(s.values[i], s.values[j]) < 0 }
func (s comparison[T]) Swap(i, j int)      { s.values[i], s.values[j] = s.values[j], s.values[i] }

func indices(n int) []int {
	out := make([]int, n)
	for i := range out {
		out[i] = i
	}
	return out
}

func category(key word, count int) int {
	divisor := uint64(count)
	_, rem := bits.Div64(key.Hi%divisor, key.Lo, divisor)
	return int(rem)
}

// chooseCall returns the same route for a capability probe and a prepared batch.
func chooseCall[T comparable, K any](c inputCase, req request, r records[T, K]) (func([]T) []int, string) {
	op := c.Operation
	parallel := req.Parallel
	if parallel && op.Kind != "sort" && op.Kind != "argsort" {
		return nil, "operation has no parallel route"
	}
	if parallel && req.Implementation != "go-tessera" && req.Implementation != "go-radsort" {
		return nil, "implementation is serial"
	}
	if req.Implementation == "go-slices-sort" || req.Implementation == "go-pdqsort" {
		if op.Kind != "sort" {
			return nil, "only direct sorting is supported"
		}
		var run func([]T)
		switch c.Type {
		case "u32":
			run = unsignedSort[T, uint32](req.Implementation)
		case "u64":
			run = unsignedSort[T, uint64](req.Implementation)
		default:
			return nil, "requires direct unsigned keys"
		}
		return func(v []T) []int { run(v); return nil }, map[string]string{
			"go-slices-sort": "slices.Sort", "go-pdqsort": "pdqsort.Slice",
		}[req.Implementation]
	}
	if req.Implementation == "go-radsort" {
		if parallel && req.Threads > 8 {
			return nil, "radsort caps parallel execution at eight workers"
		}
		if c.Type == "u128" && op.Kind != "partition-buckets" {
			return nil, "radsort requires keys of at most 64 bits"
		}
	}
	order := func(a, b T) int { return wordOrder(r.order(a), r.order(b)) }
	if req.Implementation == "go-tessera" && op.Kind != "partition-buckets" {
		switch op.Kind {
		case "sort":
			if r.key == nil {
				run, name := r.direct.sort, "Sort"
				if parallel {
					run, name = r.direct.parSort, "ParSort"
				}
				return func(v []T) []int { run(v); return nil }, name
			}
			if parallel {
				return func(v []T) []int { tessera.ParSortByKey(v, r.key); return nil }, "ParSortByKey"
			}
			return func(v []T) []int { tessera.SortByKey(v, r.key); return nil }, "SortByKey"
		case "argsort":
			if r.key == nil {
				if parallel {
					return r.direct.parArgsort, "ParArgsort"
				}
				return r.direct.argsort, "Argsort"
			}
			if parallel {
				return func(v []T) []int { return tessera.ParArgsortByKey(v, r.key) }, "ParArgsortByKey"
			}
			return func(v []T) []int { return tessera.ArgsortByKey(v, r.key) }, "ArgsortByKey"
		case "select_nth":
			if r.key == nil {
				return func(v []T) []int { r.direct.selectNth(v, op.Index); return nil }, "SelectNth"
			}
			return func(v []T) []int { tessera.SelectNthByKey(v, op.Index, r.key); return nil }, "SelectNthByKey"
		}
	}
	radixRounds := 8
	if c.Type == "u32" || c.Type == "row_u32" {
		radixRounds = 4
	}
	route := "slices.SortStableFunc"
	sortValues := slices.SortStableFunc[[]T, T]
	switch req.Implementation {
	case "go-slices-stable", "go-tessera":
	case "go-sort-stable":
		route = "sort.Stable"
		sortValues = func(v []T, compare func(T, T) int) { sort.Stable(comparison[T]{v, compare}) }
	case "go-radsort":
		route = "radsort.SortKey"
		if parallel {
			route = "radsort.ParallelSorter.SortKey"
		}
	default:
		return nil, "unknown Go implementation"
	}
	switch op.Kind {
	case "argsort":
		return func(v []T) []int {
			result := indices(len(v))
			switch req.Implementation {
			case "go-sort-stable":
				sort.Stable(comparison[int]{result, func(a, b int) int { return order(v[a], v[b]) }})
			case "go-radsort":
				radixSort(result, radixRounds, parallel, func(i int) uint64 { return r.order(v[i]).Lo })
			default:
				slices.SortStableFunc(result, func(a, b int) int { return order(v[a], v[b]) })
			}
			return result
		}, route + " on indices"
	case "partition-buckets":
		key := func(v T) int { return category(r.order(v), op.Buckets) }
		if req.Implementation == "go-tessera" {
			route = "SortByCachedKey"
		}
		return func(v []T) []int {
			offsets := make([]int, op.Buckets+1)
			switch req.Implementation {
			case "go-tessera":
				sortCategories(v, key, op.Buckets)
			case "go-radsort":
				radixSort(v, categoryBytes(op.Buckets), false, func(v T) uint64 { return uint64(key(v)) })
			default:
				sortValues(v, func(a, b T) int { return cmp.Compare(key(a), key(b)) })
			}
			for _, value := range v {
				offsets[key(value)+1]++
			}
			for i := 1; i < len(offsets); i++ {
				offsets[i] += offsets[i-1]
			}
			return offsets
		}, route + " on categories plus offsets"
	case "sort", "select_nth":
		if req.Implementation == "go-radsort" && (c.Type == "u32" || c.Type == "u64") {
			run := radixUnsigned[T](c.Type, parallel)
			switch {
			case c.Type == "u32" && parallel:
				route = "radsort.ParallelUint32s"
			case c.Type == "u32":
				route = "radsort.Uint32s"
			case parallel:
				route = "radsort.ParallelUint64s"
			default:
				route = "radsort.Uint64s"
			}
			if op.Kind == "select_nth" {
				route += " (full sort)"
			}
			return func(v []T) []int { run(v); return nil }, route
		}
		if op.Kind == "select_nth" {
			route += " (full sort)"
		}
		return func(v []T) []int {
			if req.Implementation == "go-radsort" {
				radixSort(v, radixRounds, parallel, func(v T) uint64 { return r.order(v).Lo })
			} else {
				sortValues(v, order)
			}
			return nil
		}, route
	default:
		return nil, "unknown operation"
	}
}

func unsignedSort[T any, U uint32 | uint64](name string) func([]T) {
	if name == "go-slices-sort" {
		return func(v []T) { slices.Sort(any(v).([]U)) }
	}
	return func(v []T) { pdqsort.Slice(any(v).([]U)) }
}

func radixUnsigned[T any](ty string, parallel bool) func([]T) {
	switch {
	case ty == "u32" && parallel:
		return func(v []T) { radsort.ParallelUint32s(any(v).([]uint32)) }
	case ty == "u32":
		return func(v []T) { radsort.Uint32s(any(v).([]uint32)) }
	case parallel:
		return func(v []T) { radsort.ParallelUint64s(any(v).([]uint64)) }
	default:
		return func(v []T) { radsort.Uint64s(any(v).([]uint64)) }
	}
}

func radixSort[T any](values []T, rounds int, parallel bool, key func(T) uint64) {
	if parallel {
		var sorter radsort.ParallelSorter[T]
		sorter.SortKey(values, rounds, key)
	} else {
		radsort.SortKey(values, rounds, key)
	}
}

func categoryBytes(count int) int {
	return max(1, (bits.Len(uint(count-1))+7)/8)
}

func sortCategories[T any](values []T, key func(T) int, count int) {
	switch {
	case count-1 <= 0xff:
		tessera.SortByCachedKey(values, func(v *T) uint8 { return uint8(key(*v)) })
	case count-1 <= 0xffff:
		tessera.SortByCachedKey(values, func(v *T) uint16 { return uint16(key(*v)) })
	case count-1 <= 0xffffffff:
		tessera.SortByCachedKey(values, func(v *T) uint32 { return uint32(key(*v)) })
	default:
		tessera.SortByCachedKey(values, func(v *T) uint { return uint(key(*v)) })
	}
}
