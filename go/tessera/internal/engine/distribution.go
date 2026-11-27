// SPDX-License-Identifier: BSD-2-Clause

package engine

const maxPartitionBuckets = min(1<<maxRadixBits, maxDigitBuckets, 1<<16)

// A single distribution replays one cached classification per record, in
// input order. It performs no probe or recursive finishing.
func distribute[T any, V encoded](records []T, keys []V, digit Digit) {
	e := extractor[T, T, V, V, categoryMode]{keys: keys, digit: digit}
	if len(records) <= tileCapacity[T](tileBytes) {
		var scratch tileScratch[T]
		var counts [maxDigitBuckets]uint16
		countingPasses(&scratch, records, []Digit{{Width: digit.Width}}, &counts, &e)
		return
	}
	var buffers blockBuffers[T]
	size := blockSize(len(records), recordBytes[T](), 1<<digit.Width, serialDescriptorBytes())
	partitionBlocks(records, digit.Width, size, &buffers, blockClassifier[V]{digit: Digit{Width: digit.Width}}, &e, nil)
}
