// SPDX-License-Identifier: BSD-2-Clause

package engine

func refineParallel[T any, V encoded](records []T, e extractor[T, V], workers int) {
	tileLen := tileCapacity[T](tileBytes)
	varying := prepare(records, e.mask, &e, tileLen, true)
	if varying == (Uint128{}) {
		return
	}
	var scratch workspace[T]
	if half, narrowed, ok := narrowRoot(varying, &e); ok {
		sortParallelRoot(records, narrowed, tileLen, &scratch, &half, workers)
		return
	}
	sortParallelRoot(records, varying, tileLen, &scratch, &e, workers)
}

// A parallel root consumes the same node plan once. Existing-run splits
// still finish through the serial executor.
func sortParallelRoot[T any, V encoded](records []T, varying Uint128, tileLen int, scratch *workspace[T], e *extractor[T, V], workers int) {
	if len(records) < 4*tileLen {
		if observeRoutes {
			emitRoute(routeEvent{stage: routeParallelRoot, a: 0})
		}
		sortNode(records, varying, allRanks, tileLen, scratch, e)
		return
	}
	if observeRoutes {
		emitRoute(routeEvent{stage: routeParallelRoot, a: 1})
	}
	plan := planNode(records, varying, tileLen, e)
	if plan.boundary != 0 {
		executeNode(records, varying, allRanks, tileLen, scratch, e, plan)
		return
	}
	traceNodePlan(plan, varying)
	radix, classifier, candidate, finished := splitNode(plan, varying)
	if !plan.mode && candidate == (Uint128{}) && countParallelWindow(records, plan.digit, e, workers) {
		return
	}
	// Rust uses a saturating byte product for the root task budget.
	recordLen := recordBytes[T]()
	shards := min(workers, max(1, min(len(records), maxInt/recordLen)*recordLen/minTaskBytes))
	finishers := make([]workspace[T], min(workers, 1<<radix))
	partitionParallel(records, radix, &scratch.block, classifier, e, shards, workers, func(worker, bucket int, band []T) {
		if candidate == (Uint128{}) || bucket == finished {
			return
		}
		capacity := parallelTileCapacity[T](len(band), tileLen)
		sortChild(band, candidate, allRanks, capacity, &finishers[worker], e)
	})
}

// Enlarge the tile only when the whole child fits the byte and cursor budgets.
func parallelTileCapacity[T any](length, ordinary int) int {
	if length <= tileCapacity[T](finishTileBytes) {
		return max(ordinary, length)
	}
	return ordinary
}

func countParallelWindow[T any, V encoded](records []T, digit Digit, e *extractor[T, V], workers int) bool {
	if !canCountWindow(digit, e) {
		return false
	}
	if observeRoutes {
		emitRoute(routeEvent{stage: routeCountWindow, a: len(records), b: workers, digit: digit})
	}
	chunk := ceilDiv(len(records), workers)
	shards := ceilDiv(len(records), chunk)
	local := make([][256]int, shards)
	runTasks(workers, shards, func(_, shard int) {
		start := shard * chunk
		histogramWindow(records[start:start+min(chunk, len(records)-start)], digit, e, &local[shard])
	})
	buckets := 1 << digit.Width
	counts := local[0]
	for i := 1; i < len(local); i++ {
		for bucket, count := range local[i][:buckets] {
			counts[bucket] += count
		}
	}

	window := Digit{digit.Shift + e.half, digit.Width}
	outside := e.encode(&records[0]).andNot(window.Mask())
	var ends [256]int
	end := 0
	for bucket, count := range counts[:buckets] {
		end += count
		ends[bucket] = end
	}
	runTasks(workers, buckets, func(_, bucket int) {
		end := ends[bucket]
		fillWindowRun(records[end-counts[bucket]:end], bucket, window.Shift, outside, e.decode)
	})
	return true
}
