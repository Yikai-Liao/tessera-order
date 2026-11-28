// SPDX-License-Identifier: BSD-2-Clause

package engine

import "math/bits"

// Cache one encoded key per record in source order. Key panics propagate
// before any movement starts.
func sortCached[T any, V encoded](records []T, e *extractor[T, V]) {
	if len(records) < 2 || recordBytes[T]() == 0 {
		return
	}
	keys := make([]V, len(records))
	first := e.get(&records[0])
	keys[0] = first
	previous := first
	var differences V
	ordered := true
	for i := 1; i < len(records); i++ {
		current := e.get(&records[i])
		keys[i] = current
		differences = or(differences, xor(current, first))
		if less(current, previous) {
			ordered = false
		}
		previous = current
	}
	if ordered {
		if observeRoutes {
			emitRoute(routeEvent{stage: routeCached, a: 0})
		}
		return
	}
	varying := keyBits(differences)
	low := trailingBits(varying)
	width := uint(varying.bitLen()) - low
	if width <= uint(bits.Len(uint(maxPartitionBuckets))-1) {
		if observeRoutes {
			emitRoute(routeEvent{stage: routeCached, a: 1, digit: Digit{low, width}})
		}
		distribute(records, keys, Digit{low, width})
		return
	}
	if observeRoutes {
		emitRoute(routeEvent{stage: routeCached, a: 2, mask: varying})
	}
	indices, order := orderCached(keys, recordBytes[T](), e.mask)
	keys = nil // The permutation no longer borrows the cached keys.
	if order != nil {
		applySpans(order, records)
	} else {
		applyCycles(records, indices)
	}
}

func orderCached[V encoded](keys []V, recordLen int, mask Uint128) ([]int, sourceOrder) {
	e := extractor[V, V]{encode: func(key *V) Uint128 { return keyBits(*key) }, mask: mask}
	indexed := extractor[int, V]{encode: func(index *int) Uint128 { return keyBits(keys[*index]) }, mask: mask}
	return orderIndices(keys, recordLen, &e, &indexed)
}

const maxPartitionBuckets = min(1<<maxRadixBits, maxDigitBuckets, 1<<16)

// A single distribution replays one cached classification per record, in
// input order. It performs no probe or recursive finishing.
func distribute[T any, V encoded](records []T, keys []V, digit Digit) {
	next := 0
	e := extractor[T, V]{encode: func(*T) Uint128 {
		bits := keyBits(keys[next])
		next++
		return bits
	}}
	if len(records) <= tileCapacity[T](tileBytes) {
		if observeRoutes {
			emitRoute(routeEvent{stage: routeDistribution, a: 0, digit: digit})
		}
		var scratch tileScratch[T]
		var counts [maxDigitBuckets]uint16
		countingPasses(&scratch, records, []Digit{digit}, &counts, &e)
		return
	}
	if observeRoutes {
		emitRoute(routeEvent{stage: routeDistribution, a: 1, digit: digit})
	}
	var buffers blockBuffers[T]
	size := blockSize(len(records), recordBytes[T](), 1<<digit.Width, serialDescriptorBytes())
	partitionBlocks(records, digit.Width, size, &buffers, blockClassifier[V]{digit: digit}, &e, nil)
}
