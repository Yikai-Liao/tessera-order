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
func countingPass[T any, V encoded](source, destination []T, digit Digit, cursor, ids []uint16, e *extractor[T, V]) uint16 {
	clear(cursor)
	for i := range source {
		bucket := keyDigit(e.get(&source[i]), digit)
		if !e.direct() {
			ids[i] = uint16(bucket)
		}
		cursor[bucket]++
	}
	largest := prefixSum(cursor)
	for i := range source {
		var bucket int
		if e.direct() {
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
func countingPasses[T any, V encoded](scratch *tileScratch[T], records []T, digits []Digit, counts *[maxDigitBuckets]uint16, e *extractor[T, V]) uint16 {
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

func sortTile[T any, V encoded](scratch *tileScratch[T], records []T, varying Uint128, e *extractor[T, V]) {
	if len(records) < 2 || varying == (Uint128{}) {
		return
	}
	if len(records) <= insertionLimit[T]() {
		insertionLoop(records, maxInt, e)
		return
	}
	plan := planDigits(varying, tileDigitBits(len(records), recordBytes[T]()))
	digit := plan.digits[0]
	if observeRoutes {
		for i, digit := range plan.digits[:plan.passes] {
			emitRoute(routeEvent{stage: routeTileDigits, a: i, b: plan.passes, mask: varying, digit: digit})
		}
	}
	if !plan.msd && plan.passes == 1 && tryCountWindow(records, digit, e) {
		return
	}
	var counts [maxDigitBuckets]uint16
	largest := int(countingPasses(scratch, records, plan.digits[:plan.passes], &counts, e))
	if !plan.msd || largest <= 1 {
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

func finishTileChild[T any, V encoded](scratch *tileScratch[T], records []T, candidate Uint128, e *extractor[T, V]) {
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
func tryCountWindow[T any, V encoded](records []T, digit Digit, e *extractor[T, V]) bool {
	if !canCountWindow(digit, e) {
		return false
	}
	var counts [256]int
	histogramWindow(records, digit, e, &counts)
	if observeRoutes {
		emitRoute(routeEvent{stage: routeCountWindow, a: len(records), digit: digit})
	}
	window := Digit{digit.Shift + e.half, digit.Width}
	outside := e.encode(&records[0]).andNot(window.Mask())
	start := 0
	for bucket, count := range counts[:1<<digit.Width] {
		fillWindowRun(records[start:start+count], bucket, window.Shift, outside, e.decode)
		start += count
	}
	return true
}

func canCountWindow[T any, V encoded](digit Digit, e *extractor[T, V]) bool {
	return e.direct() && digit.Width >= 1 && digit.Width <= 8
}

func histogramWindow[T any, V encoded](records []T, digit Digit, e *extractor[T, V], counts *[256]int) {
	for i := range records {
		counts[keyDigit(e.get(&records[i]), digit)]++
	}
}

func fillWindowRun[T any](run []T, bucket int, shift uint, outside Uint128, decode func(Uint128) T) {
	if len(run) == 0 {
		return
	}
	key := decode(outside.or(Uint128{Lo: uint64(bucket)}.lsh(shift)))
	for i := range run {
		run[i] = key
	}
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
