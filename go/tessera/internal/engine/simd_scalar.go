// SPDX-License-Identifier: BSD-2-Clause

//go:build !goexperiment.simd

package engine

func scalarFirstInversion[T any, V encoded](records []T, e *extractor[T, V]) int {
	return firstInversion(records, e)
}

func scalarConfirmAscending[T any, V encoded](records []T, e *extractor[T, V], previous V) bool {
	return confirmAscending(records, e, previous)
}
