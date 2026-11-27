// SPDX-License-Identifier: BSD-2-Clause

package engine

func scalarFirstInversion[T, R, K any, V encoded, M sourceMode](records []T, e *extractor[T, R, K, V, M]) int {
	return firstInversion(records, e)
}

func scalarConfirmAscending[T, R, K any, V encoded, M sourceMode](records []T, e *extractor[T, R, K, V, M], previous V) bool {
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

func canFinishMSD[T, R, K any, V encoded, M sourceMode](_ *extractor[T, R, K, V, M]) bool {
	return false
}

func finishMSD[T, R, K any, V encoded, M sourceMode](_ []T, _ int, _ *extractor[T, R, K, V, M]) bool {
	return false
}
