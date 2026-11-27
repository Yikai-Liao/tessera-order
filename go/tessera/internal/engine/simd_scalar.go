// SPDX-License-Identifier: BSD-2-Clause

package engine

func scalarFirstInversion[T any, V encoded](records []T, e *extractor[T, V]) int {
	return firstInversion(records, e)
}

func scalarConfirmAscending[T any, V encoded](records []T, e *extractor[T, V], previous V) bool {
	return confirmAscending(records, e, previous)
}

// A counting pass bounds the total by the cursor limit.
func prefixSum(counts []uint16) uint16 {
	var offset, largest uint16
	for i, count := range counts {
		largest = max(largest, count)
		counts[i] = offset
		offset += count
	}
	return largest
}

func canFinishMSD[T any, V encoded](_ *extractor[T, V]) bool {
	return false
}

func finishMSD[T any, V encoded](_ []T, _ int, _ *extractor[T, V]) bool {
	return false
}
