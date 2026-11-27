// SPDX-License-Identifier: BSD-2-Clause

package engine

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
