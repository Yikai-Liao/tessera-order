// SPDX-License-Identifier: BSD-2-Clause

package engine

import (
	"math"
	"slices"
	"sort"
)

// A stable permutation of isolated records and runs from one ordered stream.
type sourceSpan struct{ source, end int }
type sourceOrder []sourceSpan
type sourceRun struct{ start, end int }
type sourceEntry[V encoded] struct {
	key   V
	index int
}

func compareEntry[V encoded](a, b sourceEntry[V]) int {
	if byKey := compareKey(a.key, b.key); byKey != 0 {
		return byKey
	}
	if a.index < b.index {
		return -1
	}
	if a.index > b.index {
		return 1
	}
	return 0
}

func exceptionLimit[V encoded](length, recordLen int) int {
	// A tile of metadata per square root of the number of input tiles.
	entryBytes := recordBytes[sourceEntry[V]]() + recordBytes[sourceRun]() + 3*(recordBytes[sourceSpan]()+recordLen)
	tile := max(tileBytes/entryBytes, 1)
	tiles := (length-1)/tile + 1
	return tile * int(math.Sqrt(float64(tiles)))
}

func finishCut[T any, V encoded](records []T, rank int, e *extractor[T, V]) bool {
	pivot := e.get(&records[rank])
	mask := fromBits[V](e.mask)
	pivotVarying := and(pivot, mask)
	firstHigh, lastLow := -1, -1
	// Bits outside the key mask are constant across valid keys.
	if pivotVarying != mask {
		for i := 0; i < rank; i++ {
			if less(pivot, e.get(&records[i])) {
				firstHigh = i
				break
			}
		}
	}
	var zero V
	if pivotVarying != zero {
		for i := len(records) - 1; i > rank; i-- {
			if less(e.get(&records[i]), pivot) {
				lastLow = i
				break
			}
		}
	}
	limit := exceptionLimit[V](len(records), recordBytes[T]())
	var skipped []int
	ok := false
	switch {
	case firstHigh < 0 && lastLow < 0:
		return true
	case firstHigh < 0 && rank == 0:
		placeExtreme(records[:lastLow+1], false, pivot, e)
		return true
	case lastLow < 0 && rank == len(records)-1:
		placeExtreme(records[firstHigh:], true, pivot, e)
		return true
	case lastLow < 0:
		skipped, ok = prefixHoles(firstHigh, len(records), 1, rank+1-firstHigh, min(limit, len(records)-rank-1), func(i int) bool {
			return !less(pivot, e.get(&records[i]))
		})
	case firstHigh < 0:
		skipped, ok = prefixHoles(lastLow, -1, -1, lastLow+1-rank, min(limit, rank), func(i int) bool {
			return !less(e.get(&records[i]), pivot)
		})
		slices.Reverse(skipped)
	}
	if ok {
		if firstHigh >= 0 {
			applySpans(aroundBand(len(records), skipped, skipped, 0, sourceRun{0, rank + 1}), records)
		} else {
			applySpans(aroundBand(len(records), skipped, skipped, len(skipped), sourceRun{rank, len(records)}), records)
		}
		return true
	}
	end := sourceRun{0, rank + 1}
	if rank >= len(records)/2 {
		end = sourceRun{rank, len(records)}
	}
	if end.end-end.start > limit {
		return false
	}
	applySpans(orderedEnd(records, end, e), records)
	return true
}

func prefixHoles(start, end, step, needed, limit int, keep func(int) bool) ([]int, bool) {
	var holes []int
	for i := start; i != end; i += step {
		if keep(i) {
			needed--
			if needed == 0 {
				return holes, true
			}
		} else {
			if len(holes) == limit {
				return nil, false
			}
			holes = append(holes, i)
		}
	}
	return nil, false
}

// Prefer the first minimum and the last maximum. With a peer of the
// displaced endpoint, rotation preserves its source order.
func placeExtreme[T any, V encoded](records []T, last bool, anchor V, e *extractor[T, V]) {
	start, end, endpoint := 1, len(records), 0
	if last {
		start, end, endpoint = 0, len(records)-1, len(records)-1
	}
	winner := start
	best := e.get(&records[start])
	hasPeer := best == anchor
	for i := start + 1; i < end; i++ {
		key := e.get(&records[i])
		hasPeer = hasPeer || key == anchor
		if (!last && less(key, best)) || (last && !less(key, best)) {
			winner, best = i, key
		}
	}
	if !hasPeer {
		records[endpoint], records[winner] = records[winner], records[endpoint]
	} else if last {
		rotateLeft(records[winner:], 1)
	} else {
		rotateLeft(records[:winner+1], winner)
	}
}

func sourceOrderForNode[T any, V encoded](records []T, varying Uint128, rank int, e *extractor[T, V], recordLen int) sourceOrder {
	length := len(records)
	if length < keySampleLen {
		return nil
	}
	if _, complete := completeDigit(varying); complete {
		return nil
	}
	limit := exceptionLimit[V](length, recordLen)
	allowance := (limit*keySampleLen + length - 1) / length
	pivot := e.get(&records[length/2])
	inversions, exceptions := 0, 0
	for j := 0; j < keySampleLen; j++ {
		i := min(sampleIndex(length, j), length-2)
		key := e.get(&records[i])
		if key != pivot {
			exceptions++
		}
		if less(e.get(&records[i+1]), key) {
			inversions++
		}
		if inversions > allowance && exceptions > allowance {
			return nil
		}
	}
	if exceptions <= allowance {
		return dominantOrder(records, pivot, rank, limit, e)
	}
	// Radix selection discards most buckets, which beats a complete order.
	if rank != allRanks {
		return nil
	}
	var runs []sourceRun
	var removed []sourceEntry[V]
	for index := 0; index < length; {
		previous := e.get(&records[index])
		if n := len(runs); n != 0 {
			last := &runs[n-1]
			source := last.end - 1
			held := e.get(&records[source])
			if less(previous, held) {
				if len(removed)+2 > limit {
					return nil
				}
				removed = append(removed, sourceEntry[V]{held, source}, sourceEntry[V]{previous, index})
				last.end--
				if last.start == last.end {
					runs = runs[:n-1]
				}
				index++
				continue
			}
		}
		start := index
		index++
		for ; index < length; index++ {
			next := e.get(&records[index])
			if less(next, previous) {
				break
			}
			previous = next
		}
		// Changing keys may erase an inversion between probes. Coalesce
		// adjacent intervals even when that happens.
		if n := len(runs); n != 0 && runs[n-1].end == start {
			runs[n-1].end = index
		} else {
			runs = append(runs, sourceRun{start, index})
		}
	}
	slices.SortFunc(removed, compareEntry[V])
	return mergeSpans(runs, removed, func(run sourceRun, entry sourceEntry[V], _ int) int {
		// An exception lies outside this run. One source-position comparison
		// breaks every equal-key tie in the run.
		precedes := func(i int) bool {
			return compareEntry(sourceEntry[V]{e.get(&records[run.start+i]), run.start}, entry) < 0
		}
		length := run.end - run.start
		if !precedes(0) {
			return 0
		}
		if precedes(length - 1) {
			return length
		}
		return sort.Search(length, func(i int) bool { return !precedes(i) })
	})
}

func dominantOrder[T any, V encoded](records []T, pivot V, rank, limit int, e *extractor[T, V]) sourceOrder {
	var positions []int
	var entries []sourceEntry[V]
	lower := 0
	for i := range records {
		key := e.get(&records[i])
		if key == pivot {
			continue
		}
		if len(entries) == limit {
			return nil
		}
		if less(key, pivot) {
			lower++
		}
		positions = append(positions, i)
		entries = append(entries, sourceEntry[V]{key, i})
	}
	band := sourceRun{lower, len(records) - (len(entries) - lower)}
	if rank >= band.start && rank < band.end {
		if lower != 0 && lower != len(entries) {
			slices.SortStableFunc(entries, func(a, b sourceEntry[V]) int {
				left, right := less(a.key, pivot), less(b.key, pivot)
				if left == right {
					return 0
				}
				if left {
					return -1
				}
				return 1
			})
		}
		band = sourceRun{rank, rank + 1}
	} else {
		slices.SortFunc(entries, compareEntry[V])
	}
	ordered := make([]int, len(entries))
	for i, entry := range entries {
		ordered[i] = entry.index
	}
	return aroundBand(len(records), positions, ordered, lower, band)
}

// Complement both fields so an upper end prefers later source positions
// on ties. Reduce a full batch before accepting more candidates.
func orderedEnd[T any, V encoded](records []T, end sourceRun, e *extractor[T, V]) sourceOrder {
	count := end.end - end.start
	upper := end.start != 0
	invert := fromBits[V](lowMask(128))
	orient := func(key V, index int) sourceEntry[V] {
		if upper {
			return sourceEntry[V]{xor(key, invert), ^index}
		}
		return sourceEntry[V]{key, index}
	}
	kept := make([]sourceEntry[V], 0, 2*count)
	var ceiling sourceEntry[V]
	for i := end.start; i < end.end; i++ {
		entry := orient(e.get(&records[i]), i)
		if i == end.start || compareEntry(ceiling, entry) < 0 {
			ceiling = entry
		}
		kept = append(kept, entry)
	}
	var original []V
	var differences V
	first := kept[0].key
	if recordBytes[T]() > recordBytes[sourceEntry[V]]() {
		original = make([]V, len(kept))
		for i, entry := range kept {
			original[i] = entry.key
			differences = or(differences, xor(entry.key, first))
		}
	}
	fixed := e.mask.andNot(keyBits(differences))
	lookups := len(records) / count
	disjoint := len(original) != 0 && fixed != (Uint128{})
	start, stop := count, len(records)
	if upper {
		start, stop = 0, end.start
	}
	for i := start; i < stop; i++ {
		entry := orient(e.get(&records[i]), i)
		if compareEntry(entry, ceiling) < 0 {
			kept = append(kept, entry)
			if len(kept) == 2*count {
				ceiling = reduceEnd(kept, count-1)
				kept = kept[:count]
			}
		}
		// Common fixed bits usually reject a possible tie outright.
		if disjoint && keyBits(xor(entry.key, first)).and(fixed) == (Uint128{}) {
			disjoint = lookups != 0 && !slices.Contains(original, entry.key)
			lookups = max(lookups-1, 0)
		}
	}
	slices.SortFunc(kept, compareEntry[V])
	kept = kept[:count]
	if upper {
		slices.Reverse(kept)
	}
	selected := make([]int, count)
	for i, key := range kept {
		selected[i] = key.index
		if upper {
			selected[i] = ^key.index
		}
	}
	sources := slices.Clone(selected)
	slices.Sort(sources)
	if disjoint {
		displaced := make([]int, 0, count)
		for i := end.start; i < end.end; i++ {
			if _, found := slices.BinarySearch(sources, i); !found {
				displaced = append(displaced, i)
			}
		}
		var ordered []int
		if upper {
			ordered = append(displaced, selected...)
		} else {
			ordered = append(selected, displaced...)
		}
		for i := end.start; i < end.end; i++ {
			sources = append(sources, i)
		}
		slices.Sort(sources)
		sources = slices.Compact(sources)
		return aroundBand(len(records), sources, ordered, 0, sourceRun{})
	}
	if upper {
		return aroundBand(len(records), sources, selected, 0, sourceRun{0, end.start})
	}
	return aroundBand(len(records), sources, selected, count, sourceRun{count, len(records)})
}

// Select the batch ceiling in place. Go's slices package supplies sorting
// but has no nth operation. Bound adverse pivot sequences with a sort of
// the remaining subrange.
func reduceEnd[V encoded](entries []sourceEntry[V], rank int) sourceEntry[V] {
	start, end := 0, len(entries)
	for budget := 2 * wordBits; end-start > 1 && budget > 0; budget-- {
		middle := start + (end-start)/2
		pivot := entries[middle]
		entries[middle], entries[end-1] = entries[end-1], entries[middle]
		slot := start
		for i := start; i < end-1; i++ {
			if compareEntry(entries[i], pivot) < 0 {
				entries[slot], entries[i] = entries[i], entries[slot]
				slot++
			}
		}
		entries[slot], entries[end-1] = entries[end-1], entries[slot]
		if rank == slot {
			return pivot
		}
		if rank < slot {
			end = slot
		} else {
			start = slot + 1
		}
	}
	slices.SortFunc(entries[start:end], compareEntry[V])
	return entries[rank]
}

type positionedSource struct{ slot, index int }

// Clamp exceptional slots to the sides of the reserved background band.
func aroundBand(length int, sources, ordered []int, before int, band sourceRun) sourceOrder {
	runs := make([]sourceRun, 0, len(sources)+1)
	exceptions := make([]positionedSource, len(sources))
	start := 0
	for i, source := range sources {
		runs = append(runs, sourceRun{start, source})
		start = source + 1
		slot := source
		if i < before {
			slot = min(slot, band.start-(before-i))
		} else {
			slot = max(slot, band.end+i-before)
		}
		exceptions[i] = positionedSource{slot, ordered[i]}
	}
	runs = append(runs, sourceRun{start, length})
	return mergeSpans(runs, exceptions, func(run sourceRun, entry positionedSource, written int) int {
		return min(entry.slot-written, run.end-run.start)
	})
}

func (entry sourceEntry[V]) getSource() int   { return entry.index }
func (entry positionedSource) getSource() int { return entry.index }

// Merge the exceptions into the disjoint runs of the ordered source stream.
func mergeSpans[E interface{ getSource() int }](runs []sourceRun, exceptions []E, prefix func(sourceRun, E, int) int) sourceOrder {
	order := make(sourceOrder, 0, len(runs)+len(exceptions))
	at := 0
	for _, entry := range exceptions {
		for at < len(runs) {
			run := &runs[at]
			if run.start < run.end {
				length := prefix(*run, entry, order.length())
				if length > 0 {
					order.push(run.start, length)
					run.start += length
				}
			}
			if run.start != run.end {
				break
			}
			at++
		}
		order.push(entry.getSource(), 1)
	}
	for _, run := range runs[at:] {
		if run.start < run.end {
			order.push(run.start, run.end-run.start)
		}
	}
	return order
}

func (order sourceOrder) length() int {
	if len(order) == 0 {
		return 0
	}
	return order[len(order)-1].end
}

func (order *sourceOrder) push(source, length int) {
	*order = append(*order, sourceSpan{source, order.length() + length})
}

func (order sourceOrder) writeIndices(output []int) {
	start := 0
	for _, span := range order {
		for i := start; i < span.end; i++ {
			output[i] = span.source + i - start
		}
		start = span.end
	}
}

// Save isolated records first. All longer spans belong to one monotone
// subsequence, so move leftward spans forward and rightward spans backward.
func applySpans[T any](order sourceOrder, records []T) {
	if order.length() != len(records) {
		panic("tessera: incomplete source spans")
	}
	start, singles := 0, 0
	for _, span := range order {
		if span.end-start == 1 && span.source != start {
			singles++
		}
		start = span.end
	}
	saved := make([]T, 0, singles)
	start = 0
	for _, span := range order {
		if span.end-start == 1 && span.source != start {
			saved = append(saved, records[span.source])
		}
		start = span.end
	}
	start = 0
	for _, span := range order {
		length := span.end - start
		if length > 1 && start < span.source {
			copy(records[start:span.end], records[span.source:span.source+length])
		}
		start = span.end
	}
	for i := len(order) - 1; i >= 0; i-- {
		span := order[i]
		start = 0
		if i > 0 {
			start = order[i-1].end
		}
		length := span.end - start
		if length > 1 && start > span.source {
			copy(records[start:span.end], records[span.source:span.source+length])
		}
	}
	start, slot := 0, 0
	for _, span := range order {
		if span.end-start == 1 && span.source != start {
			records[start] = saved[slot]
			slot++
		}
		start = span.end
	}
}
