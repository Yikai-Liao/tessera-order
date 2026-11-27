// SPDX-License-Identifier: BSD-2-Clause

package engine

import (
	"math/bits"
	"slices"
)

const splitBits = 8
const keySampleLen = 128
const modeShareDenominator = 4

// Each stratum supplies one sample. A golden-ratio offset avoids aliasing
// with periodic input.
func sampleIndex(length, stratum int) int {
	stride := length / min(length, keySampleLen)
	fraction := uint64(stratum) * 0x9e37_79b9_7f4a_7c15
	offset, _ := bits.Mul64(fraction, uint64(stride))
	return stratum*stride + int(offset)
}

func compareKey[V encoded](a, b V) int {
	if less(a, b) {
		return -1
	}
	if a == b {
		return 0
	}
	return 1
}

func sampleKeys[T, R, K any, V encoded, M sourceMode](records []T, e *extractor[T, R, K, V, M], keys *[keySampleLen]V) []V {
	sample := keys[:min(len(records), keySampleLen)]
	for i := range sample {
		sample[i] = e.get(&records[sampleIndex(len(records), i)])
	}
	slices.SortFunc(sample, compareKey[V])
	return sample
}

func sampleMode[V encoded](sample []V) (V, bool) {
	var mode V
	best := 0
	for start := 0; start < len(sample); {
		end := start + 1
		for end < len(sample) && sample[start] == sample[end] {
			end++
		}
		if end-start >= best {
			best = end - start
			mode = sample[start]
		}
		start = end
	}
	return mode, len(sample) > 0 && best*modeShareDenominator >= len(sample)
}

func topDigitOccupancy[V encoded](sample []V, digit Digit) int {
	previous := -1
	occupied := 0
	for _, key := range sample {
		bucket := keyDigit(key, digit)
		if bucket != previous {
			occupied++
			previous = bucket
		}
	}
	return occupied
}

func chooseSplitDigit[V encoded](length, tileLen int, varying Uint128, sample []V) Digit {
	top := uint(varying.bitLen())
	children := max(length/(max(tileLen, 64)/2), 2)
	width := max(1, min(uint(max(bits.Len(uint(children-1)), 3)), splitBits, top))
	// Widen a concentrated digit until a quarter of its buckets are sampled.
	for width < top && width+2 <= maxRadixBits {
		if 1<<width <= 4*len(sample) && topDigitOccupancy(sample, Digit{top - width, width})*4 >= 1<<width {
			break
		}
		wider := min(width+2, top)
		if varying.and(lowMask(wider-width).lsh(top-wider)) == (Uint128{}) {
			break
		}
		width = wider
	}
	return Digit{top - width, width}
}

// A top bit already in two runs needs at most one stable rotation.
func twoRuns[T, R, K any, V encoded, M sourceMode](records []T, bit Uint128, e *extractor[T, R, K, V, M]) (int, bool) {
	mask := fromBits[V](bit)
	var zero V
	first := and(e.get(&records[0]), mask) != zero
	boundary := 1
	for boundary < len(records) && ((and(e.get(&records[boundary]), mask) != zero) == first) {
		boundary++
	}
	if boundary == len(records) {
		return 0, false
	}
	for i := boundary + 1; i < len(records); i++ {
		if (and(e.get(&records[i]), mask) != zero) == first {
			return 0, false
		}
	}
	return boundary, first
}

func completeDigit(varying Uint128) (Digit, bool) {
	low := trailingBits(varying)
	width := uint(varying.bitLen()) - low
	return Digit{low, width}, width <= splitBits
}

type nodePlan[V encoded] struct {
	digit    Digit
	equal    V
	mode     bool
	boundary int
	set      bool
}

func planNode[T, R, K any, V encoded, M sourceMode](records []T, varying Uint128, tileLen int, e *extractor[T, R, K, V, M]) nodePlan[V] {
	if digit, ok := completeDigit(varying); ok {
		return nodePlan[V]{digit: digit}
	}
	var keys [keySampleLen]V
	var sample []V
	if len(records)/4 >= tileLen {
		sample = sampleKeys(records, e, &keys)
		if equal, ok := sampleMode(sample); ok {
			return nodePlan[V]{equal: equal, mode: true}
		}
	}
	if boundary, set := twoRuns(records, topBit(varying), e); boundary != 0 {
		return nodePlan[V]{boundary: boundary, set: set}
	}
	if sample == nil {
		sample = sampleKeys(records, e, &keys)
	}
	return nodePlan[V]{digit: chooseSplitDigit(len(records), tileLen, varying, sample)}
}
