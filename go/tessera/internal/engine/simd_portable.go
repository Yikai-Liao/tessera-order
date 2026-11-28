// SPDX-License-Identifier: BSD-2-Clause

//go:build goexperiment.simd

package engine

import (
	"cmp"
	"simd"
)

const simdMin = 64
const scanVectors = 4
const maxVectorBytes = 64

func scalarFirstInversion[T any, V encoded](records []T, e *extractor[T, V]) int {
	if index, ok := nativeFirstInversion(records, e.direct()); ok {
		return index
	}
	return firstInversion(records, e)
}

func scalarConfirmAscending[T any, V encoded](records []T, e *extractor[T, V], previous V) bool {
	if len(records) >= simdMin && e.direct() && less(e.get(&records[0]), previous) {
		return false
	}
	if index, ok := nativeFirstInversion(records, e.direct()); ok {
		return index < 0
	}
	return confirmAscending(records, e, previous)
}

func nativeFirstInversion[T any](records []T, direct bool) (int, bool) {
	if len(records) < simdMin || !direct {
		return -1, false
	}
	switch values := any(records).(type) {
	case []uint8:
		return scanPairs(values, 1, simd.LoadUint8s, greaterUint8, expand8), true
	case []uint16:
		return scanPairs(values, 2, simd.LoadUint16s, simd.Uint16s.Greater, expand16), true
	case []uint32:
		return scanPairs(values, 4, simd.LoadUint32s, simd.Uint32s.Greater, expand32), true
	case []uint64:
		return scanPairs(values, 8, simd.LoadUint64s, simd.Uint64s.Greater, expand64), true
	case []int8:
		return scanPairs(values, 1, simd.LoadInt8s, simd.Int8s.Greater, expand8), true
	case []int16:
		return scanPairs(values, 2, simd.LoadInt16s, simd.Int16s.Greater, expand16), true
	case []int32:
		return scanPairs(values, 4, simd.LoadInt32s, simd.Int32s.Greater, expand32), true
	case []int64:
		return scanPairs(values, 8, simd.LoadInt64s, simd.Int64s.Greater, expand64), true
	default:
		return -1, false
	}
}

func scanPairs[W cmp.Ordered, S any, M interface{ Or(M) M }](
	values []W, laneBytes int, load func([]W) S, greater func(S, S) M, expand func(M) simd.Uint64s,
) int {
	const peel = 4
	for i := range peel {
		if values[i] > values[i+1] {
			return i
		}
	}
	lanes := simd.VectorBitSize() / (8 * laneBytes)
	index := peel
	for index+lanes < len(values) {
		vectors := min(scanVectors, (len(values)-1-index)/lanes)
		end := index + vectors*lanes
		// The full overlapping loads compare adjacent records. Combine up to
		// four comparison masks before inspecting a stored vector.
		mask := greater(load(values[index:]), load(values[index+1:]))
		for pair := index + lanes; pair < end; pair += lanes {
			mask = mask.Or(greater(load(values[pair:]), load(values[pair+1:])))
		}
		if anyMatch(expand(mask)) {
			break
		}
		index = end
	}
	// Locate the first inversion in a hit, or scan the final partial vector.
	for ; index+1 < len(values); index++ {
		if values[index] > values[index+1] {
			return index
		}
	}
	return -1
}

// Reinterpret the mask as 64-bit lanes to share one stack buffer across
// key widths. The search only needs to know whether any bit was set.
func anyMatch(mask simd.Uint64s) bool {
	var lanes [maxVectorBytes / 8]uint64
	mask.Store(lanes[:])
	for _, lane := range lanes {
		if lane != 0 {
			return true
		}
	}
	return false
}

func expand8(mask simd.Mask8s) simd.Uint64s   { return mask.ToInt8s().ToBits().ReshapeToUint64s() }
func expand16(mask simd.Mask16s) simd.Uint64s { return mask.ToInt16s().ToBits().ReshapeToUint64s() }
func expand32(mask simd.Mask32s) simd.Uint64s { return mask.ToInt32s().ToBits().ReshapeToUint64s() }
func expand64(mask simd.Mask64s) simd.Uint64s { return mask.ToInt64s().ToBits() }

// Uint8s exposes Min and NotEqual, which together test unsigned descent.
func greaterUint8(left, right simd.Uint8s) simd.Mask8s {
	return left.Min(right).NotEqual(left)
}
