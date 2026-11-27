// SPDX-License-Identifier: BSD-2-Clause

package engine

// firstInversion retains the previous key so a callback runs once per scanned
// record. No records move during this scan.
func firstInversion[T, R, K any, V encoded, M sourceMode](records []T, e *extractor[T, R, K, V, M]) int {
	if len(records) == 0 {
		return -1
	}
	previous := e.get(&records[0])
	for i := 1; i < len(records); i++ {
		current := e.get(&records[i])
		if less(current, previous) {
			return i - 1
		}
		previous = current
	}
	return -1
}

// Only candidate bits are returned, so changing keys cannot restore a parent
// digit and prevent recursion from making progress.
func scanVarying[T, R, K any, V encoded, M sourceMode](records []T, candidate Uint128, e *extractor[T, R, K, V, M]) Uint128 {
	if candidate == (Uint128{}) {
		return Uint128{}
	}
	union := e.get(&records[0])
	intersection := union
	mask := fromBits[V](candidate)
	for index, batch := 1, 64; index < len(records); {
		end := index + min(batch, len(records)-index)
		for i := index; i < end; i++ {
			key := e.get(&records[i])
			union = or(union, key)
			intersection = and(intersection, key)
		}
		if and(xor(union, intersection), mask) == mask {
			break
		}
		index = end
		batch += min(batch, max(0, len(records)-batch))
	}
	return keyBits(and(xor(union, intersection), mask))
}

// A budget exit leaves a stable permutation. The lifted record is inserted
// in its current vacancy before returning normally.
func insertionLoop[T, R, K any, V encoded, M sourceMode](records []T, budget int, e *extractor[T, R, K, V, M]) (int, bool) {
	var value T
	swaps := 0
	for start := 0; start < len(records); {
		offset := scalarFirstInversion(records[start:], e)
		if offset < 0 {
			break
		}
		index := start + offset + 1
		hole := index - 1
		value = records[index]
		records[index] = records[hole]
		swaps++
		for swaps <= budget && hole > 0 {
			previous := e.get(&records[hole-1])
			if !less(e.get(&value), previous) {
				break
			}
			records[hole] = records[hole-1]
			hole--
			swaps++
		}
		records[hole] = value
		if swaps > budget {
			return swaps, false
		}
		start = index
	}
	return swaps, true
}

// Combine each group's ordering results before rejecting. A callback may
// read the rest of a group after the first inversion.
func confirmAscending[T, R, K any, V encoded, M sourceMode](records []T, e *extractor[T, R, K, V, M], previous V) bool {
	i := 0
	for ; i+4 <= len(records); i += 4 {
		ordered := true
		for j := i; j < i+4; j++ {
			current := e.get(&records[j])
			if less(current, previous) {
				ordered = false
			}
			previous = current
		}
		if !ordered {
			return false
		}
	}
	for ; i < len(records); i++ {
		current := e.get(&records[i])
		if less(current, previous) {
			return false
		}
		previous = current
	}
	return true
}
