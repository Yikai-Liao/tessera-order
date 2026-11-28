// SPDX-License-Identifier: BSD-2-Clause

package engine

import (
	"math/bits"
	"unsafe"
)

const (
	wordBits       = bits.UintSize
	maxInt         = int(^uint(0) >> 1)
	insertionMax   = 48
	tileBytes      = 64 * 1024
	maxTileRecords = 1<<16 - 1
	maxCursorBytes = 32 * 1024
)

func recordBytes[T any]() int {
	var record T
	// Sizeof reads the type's layout without accessing record storage.
	return int(unsafe.Sizeof(record))
}

func insertionLimit[T any]() int {
	const comparisonBytes = 16
	return max(16, insertionMax*comparisonBytes/(comparisonBytes+recordBytes[T]()))
}

func tileCapacity[T any](bytes int) int {
	return max(1, min(bytes/max(recordBytes[T](), 1), maxTileRecords))
}
