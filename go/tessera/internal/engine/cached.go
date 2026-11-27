// SPDX-License-Identifier: BSD-2-Clause

package engine

import "math/bits"

// Cache one encoded key per record in source order. Key panics propagate
// before any movement starts.
func sortCached[T, R, K any, V encoded, M sourceMode](records []T, e *extractor[T, R, K, V, M]) {
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
		return
	}
	varying := keyBits(differences)
	low := trailingBits(varying)
	width := uint(varying.bitLen()) - low
	if width <= uint(bits.Len(uint(maxPartitionBuckets))-1) {
		distribute(records, keys, Digit{low, width})
		return
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
	e := extractor[V, V, V, V, directMode]{mask: mask}
	indexed := extractor[int, V, V, V, cachedIndexMode]{keys: keys, mask: mask}
	return orderIndices(keys, recordLen, &e, &indexed)
}
