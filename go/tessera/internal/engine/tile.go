// SPDX-License-Identifier: BSD-2-Clause

package engine

type tileScratch[T any] struct {
	records []T
	ids     []uint16
}

func reserve[T any](storage []T, length int) []T {
	if cap(storage) < length {
		return make([]T, length)
	}
	return storage[:length]
}

// Histogram caches callback classifications. Scatter calls only sealed direct
// encoders, whose classifications cannot change between passes.
func countingPass[T, R, K any, V encoded, M sourceMode](source, destination []T, digit Digit, cursor, ids []uint16, e *extractor[T, R, K, V, M]) uint16 {
	clear(cursor)
	var mode M
	for i := range source {
		var bucket int
		if len(mode) == len(directIdentityMode{}) {
			bucket = wordBucket(any(source[i]).(V), digit)
		} else {
			bucket = keyDigit(e.get(&source[i]), digit)
		}
		if !e.direct() {
			ids[i] = uint16(bucket)
		}
		cursor[bucket]++
	}
	largest := prefixSum(cursor)
	for i := range source {
		var bucket int
		if len(mode) == len(directIdentityMode{}) {
			bucket = wordBucket(any(source[i]).(V), digit)
		} else if e.direct() {
			bucket = keyDigit(e.get(&source[i]), digit)
		} else {
			bucket = int(ids[i])
		}
		slot := cursor[bucket]
		cursor[bucket]++
		destination[slot] = source[i]
	}
	return largest
}

// Run one pass per digit, last digit first. An odd number of passes
// leaves the result in scratch, so copy it back on normal completion.
func countingPasses[T, R, K any, V encoded, M sourceMode](scratch *tileScratch[T], records []T, digits []Digit, counts *[maxDigitBuckets]uint16, e *extractor[T, R, K, V, M]) uint16 {
	scratch.records = reserve(scratch.records, len(records))
	if !e.direct() {
		scratch.ids = reserve(scratch.ids, len(records))
	}
	source, destination := records, scratch.records
	var largest uint16
	for i := len(digits) - 1; i >= 0; i-- {
		digit := digits[i]
		largest = countingPass(source, destination, digit, counts[:1<<digit.Width], scratch.ids, e)
		source, destination = destination, source
	}
	if len(digits)%2 != 0 {
		copy(records, source)
	}
	return largest
}

func sortTile[T, R, K any, V encoded, M sourceMode](scratch *tileScratch[T], records []T, varying Uint128, e *extractor[T, R, K, V, M]) {
	if len(records) < 2 || varying == (Uint128{}) {
		return
	}
	if len(records) <= insertionLimit[T]() {
		insertionLoop(records, maxInt, e)
		return
	}
	plan := planDigits(varying, tileDigitBits(len(records), recordBytes[T]()), len(records), canFinishMSD(e))
	digit := plan.digits[0]
	if !plan.msd && plan.passes == 1 && tryCountWindow(records, digit, e) {
		return
	}
	var counts [maxDigitBuckets]uint16
	largest := int(countingPasses(scratch, records, plan.digits[:plan.passes], &counts, e))
	if !plan.msd || largest <= 1 || finishMSD(records, largest, e) {
		return
	}
	// Whole-tile insertion has the same inversion count as separately
	// finishing insertion-sized buckets and skips the boundary walk.
	if largest <= insertionLimit[T]() {
		insertionLoop(records, maxInt, e)
		return
	}
	candidate := varying.andNot(digit.Mask())
	start := 0
	for _, end := range counts[:1<<digit.Width] {
		finishTileChild(scratch, records[start:int(end)], candidate, e)
		start = int(end)
	}
}

func finishTileChild[T, R, K any, V encoded, M sourceMode](scratch *tileScratch[T], records []T, candidate Uint128, e *extractor[T, R, K, V, M]) {
	if len(records) < 2 || candidate == (Uint128{}) {
		return
	}
	if len(records) <= insertionLimit[T]() {
		insertionLoop(records, maxInt, e)
		return
	}
	sortTile(scratch, records, scanVarying(records, candidate, e), e)
}

// A complete direct digit reconstructs interchangeable keys from the common
// outside bits, including any constant half omitted at root dispatch.
func tryCountWindow[T, R, K any, V encoded, M sourceMode](records []T, digit Digit, e *extractor[T, R, K, V, M]) bool {
	if !e.direct() || digit.Width < 1 || digit.Width > 8 {
		return false
	}
	var counts [256]int
	for i := range records {
		counts[keyDigit(e.get(&records[i]), digit)]++
	}
	outside := e.fullBits(&records[0]).andNot(Digit{digit.Shift + e.offset, digit.Width}.Mask())
	start := 0
	for bucket, count := range counts[:1<<digit.Width] {
		if count == 0 {
			continue
		}
		bits := outside.or(Uint128{Lo: uint64(bucket)}.lsh(digit.Shift + e.offset))
		key := e.decode(bits)
		for i := start; i < start+count; i++ {
			records[i] = key
		}
		start += count
	}
	return true
}
