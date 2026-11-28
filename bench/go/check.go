// SPDX-License-Identifier: BSD-2-Clause

package main

import (
	"cmp"
	"fmt"
	"hash/fnv"
	"slices"
	"strings"
	"unsafe"
)

type prepared struct {
	bytes   int
	restore func(int)
	run     func()
	check   func() error
	digest  func() string
}

func assemble[T comparable, K any](c inputCase, req request, r records[T, K]) (prepared, string) {
	if recordSize[T]() != req.RecordBytes {
		return prepared{}, "record width differs from the catalog"
	}
	run, route := chooseCall(c, req, r)
	if run == nil {
		return prepared{}, route
	}
	if r.values == nil {
		return prepared{bytes: req.RecordBytes}, route
	}
	op := c.Operation
	order := func(a, b T) int { return wordOrder(r.order(a), r.order(b)) }
	if op.Kind == "partition-buckets" {
		order = func(a, b T) int {
			return cmp.Compare(category(r.order(a), op.Buckets), category(r.order(b), op.Buckets))
		}
	}
	// An independent reference compares original indices as the final tie-break.
	ranks := indices(len(r.values))
	slices.SortFunc(ranks, func(a, b int) int {
		if sign := order(r.values[a], r.values[b]); sign != 0 {
			return sign
		}
		return cmp.Compare(a, b)
	})
	size := req.RecordBytes * c.Len
	if r.owned {
		size += 16 * c.Len
	}
	indexBytes := int(unsafe.Sizeof(int(0)))
	var offsets []int
	var original []T
	switch op.Kind {
	case "argsort":
		size = indexBytes * c.Len
		original = slices.Clone(r.values)
	case "partition-buckets":
		size += indexBytes * (op.Buckets + 1)
		offsets = make([]int, op.Buckets+1)
		next := 0
		for bucket := range offsets {
			for next < len(ranks) && category(r.order(r.values[ranks[next]]), op.Buckets) < bucket {
				next++
			}
			offsets[bucket] = next
		}
	}
	// Float equality here is bit identity, including NaNs and signed zero.
	same := func(a, b T) bool { return a == b }
	if c.Type == "f64" || c.Type == "reverse_f64" {
		same = func(a, b T) bool { return r.order(a) == r.order(b) }
	}

	var buffers [][]T
	var outputs [][]int
	return prepared{
		bytes: size,
		restore: func(batch int) {
			bufferCount := len(buffers)
			buffers = slices.Grow(buffers[:bufferCount], max(0, batch-bufferCount))
			buffers = buffers[:batch]
			for i := range buffers {
				if op.Kind == "argsort" {
					buffers[i] = r.values
					continue
				}
				if buffers[i] == nil {
					buffers[i] = make([]T, c.Len)
				}
				copy(buffers[i], r.values)
				if r.owned {
					for j := range buffers[i] {
						// Retain the independently allocated string until this slot ends.
						v := any(&buffers[i][j]).(*owned)
						v.payload = strings.Clone(v.payload)
					}
				}
			}
			outputs = slices.Grow(outputs[:0], batch)[:batch]
			clear(outputs)
		},
		run: func() {
			for i, values := range buffers {
				outputs[i] = run(values)
			}
		},
		check: func() error {
			if op.Kind == "argsort" && !slices.EqualFunc(r.values, original, same) {
				return fmt.Errorf("argsort modified the source records")
			}
			for i, values := range buffers {
				if op.Kind == "argsort" {
					if !slices.Equal(outputs[i], ranks) {
						return fmt.Errorf("output differs from the stable index permutation")
					}
					continue
				}
				if !slices.Equal(outputs[i], offsets) {
					return fmt.Errorf("category offsets differ from the reference")
				}
				if err := checkRecords(values, r.values, ranks, op, order, same); err != nil {
					return err
				}
			}
			return nil
		},
		digest: func() string {
			hash := fnv.New64a()
			buffer := make([]byte, 0, 128)
			for _, v := range r.values {
				buffer = r.append(buffer[:0], v)
				hash.Write(buffer)
			}
			return fmt.Sprintf("%016x", hash.Sum64())
		},
	}, route
}

func checkRecords[T any](values, original []T, ranks []int, op operation, order func(T, T) int, same func(T, T) bool) error {
	matches := func(actual []T, expected []int) bool {
		for i, index := range expected {
			if !same(actual[i], original[index]) {
				return false
			}
		}
		return true
	}
	if op.Kind == "select_nth" {
		rank := op.Index
		if !same(values[rank], original[ranks[rank]]) {
			return fmt.Errorf("selected record differs from the stable rank")
		}
		for _, side := range [][2]int{{0, rank}, {rank + 1, len(values)}} {
			members := slices.Clone(values[side[0]:side[1]])
			slices.SortStableFunc(members, order)
			if !matches(members, ranks[side[0]:side[1]]) {
				return fmt.Errorf("outer region differs from the stable rank membership")
			}
		}
	} else if !matches(values, ranks) {
		return fmt.Errorf("output differs from stable reference (order, permutation or stability)")
	}
	return nil
}
