// SPDX-License-Identifier: BSD-2-Clause

package engine

import "sort"

// Pack prefixes into spare index bits, then refine the remaining prefix ties
// in the same output buffer.
func argsortParallel[T any, V encoded](values []T, e extractor[T, V], workers int) []int {
	length := len(values)
	tileLen := tileCapacity[int](tileBytes)
	indexed := extractor[int, V]{encode: func(index *int) Uint128 { return e.encode(&values[*index]) }}
	varying, resolved := resolveVarying(values, e.mask, &e, probe(values, &e))
	words := make([]int, length)
	chunk := ceilDiv(length, workers)
	if varying == (Uint128{}) {
		if observeRoutes {
			emitRoute(routeEvent{stage: routeParallelTies, a: 0})
		}
		runTasks(workers, ceilDiv(length, chunk), func(_, task int) {
			start := task * chunk
			for i := range words[start : start+min(chunk, length-start)] {
				words[start+i] = start + i
			}
		})
		finishOrdered(words, resolved, &indexed)
		return words
	}

	indexBits := uint((Uint128{Lo: uint64(length - 1)}).bitLen())
	spareBits := wordBits - indexBits
	high := uint(varying.bitLen())
	prefixBits := min(high-trailingBits(varying), spareBits)
	prefixShift := high - prefixBits
	indexMask := int(^uint(0) >> spareBits)
	prefixMask := uint(1)<<prefixBits - 1
	remaining := varying.and(lowMask(prefixShift))
	runTasks(workers, ceilDiv(length, chunk), func(_, task int) {
		start := task * chunk
		for i := range words[start : start+min(chunk, length-start)] {
			at := start + i
			prefix := shiftedWord(e.get(&values[at]), prefixShift) & prefixMask
			words[at] = int(prefix<<indexBits) | at
		}
	})
	prefix := extractor[int, uint64]{encode: func(word *int) Uint128 { return Uint128{Lo: uint64(uint(*word) >> indexBits)} }}
	var scratch workspace[int]
	sortParallelRoot(words, varying.rsh(prefixShift), tileLen, &scratch, &prefix, workers)
	if remaining == (Uint128{}) {
		if observeRoutes {
			emitRoute(routeEvent{stage: routeParallelTies, a: 1})
		}
		unmaskIndices(words, indexMask, workers)
		return words
	}

	// Give each task roughly one chunk without cutting any equal-prefix
	// group. Reserve groups at least that large for their own root.
	bands := make([][]int, 0, workers)
	var huge [][]int
	rest := words
	for len(rest) > chunk {
		key := uint(rest[chunk-1]) >> indexBits
		end := chunk + sort.Search(len(rest)-chunk, func(i int) bool { return uint(rest[chunk+i])>>indexBits != key })
		start := sort.Search(chunk, func(i int) bool { return uint(rest[i])>>indexBits >= key })
		if end-start >= chunk {
			huge = append(huge, rest[start:end:end])
		} else {
			start = end
		}
		bands = append(bands, rest[:start:start])
		rest = rest[end:]
	}
	bands = append(bands, rest)
	if observeRoutes {
		emitRoute(routeEvent{stage: routeParallelTies, a: 2, b: len(huge)})
	}

	finishers := make([]workspace[int], min(workers, len(bands)))
	runTasks(workers, len(bands), func(worker, task int) {
		band := bands[task]
		for start := 0; start < len(band); {
			end := start + 1
			key := uint(band[start]) >> indexBits
			for end < len(band) && uint(band[end])>>indexBits == key {
				end++
			}
			ties := band[start:end:end]
			for i := range ties {
				ties[i] &= indexMask
			}
			sortChild(ties, remaining, allRanks, tileLen, &finishers[worker], &indexed)
			start = end
		}
	})
	for _, ties := range huge {
		sub := unmaskAndScanVarying(ties, indexMask, remaining, &indexed, workers)
		if sub != (Uint128{}) {
			sortParallelRoot(ties, sub, tileLen, &scratch, &indexed, workers)
		}
	}
	return words
}

func unmaskIndices(words []int, indexMask, workers int) {
	if len(words) == 0 {
		return
	}
	chunk := ceilDiv(len(words), workers)
	runTasks(workers, ceilDiv(len(words), chunk), func(_, task int) {
		start := task * chunk
		part := words[start : start+min(chunk, len(words)-start)]
		for i := range part {
			part[i] &= indexMask
		}
	})
}

type indexVariation struct {
	union, intersection Uint128
}

func scanIndexChunk[V encoded](words []int, indexMask int, e *extractor[int, V]) indexVariation {
	words[0] &= indexMask
	first := keyBits(e.get(&words[0]))
	summary := indexVariation{first, first}
	for i := 1; i < len(words); i++ {
		words[i] &= indexMask
		key := keyBits(e.get(&words[i]))
		summary.union = summary.union.or(key)
		summary.intersection = summary.intersection.and(key)
	}
	return summary
}

// After the serial probe, check variation between waves. This caps extra
// key reads after the serial early exit at workers*waveChunk - 1 per wave.
func unmaskAndScanVarying[V encoded](words []int, indexMask int, remaining Uint128, e *extractor[int, V], workers int) Uint128 {
	const probeLen = 64
	const waveChunk = 8192
	probeEnd := min(len(words), probeLen)
	summary := scanIndexChunk(words[:probeEnd], indexMask, e)
	tail := words[probeEnd:]
	varying := summary.union.xor(summary.intersection).and(remaining)
	if varying == remaining {
		unmaskIndices(tail, indexMask, workers)
		return varying
	}
	partials := make([]indexVariation, min(workers, ceilDiv(len(tail), waveChunk)))
	for len(tail) != 0 {
		tasks := min(workers, ceilDiv(len(tail), waveChunk))
		runTasks(workers, tasks, func(_, task int) {
			start := task * waveChunk
			partials[task] = scanIndexChunk(tail[start:start+min(waveChunk, len(tail)-start)], indexMask, e)
		})
		for _, part := range partials[:tasks] {
			summary.union = summary.union.or(part.union)
			summary.intersection = summary.intersection.and(part.intersection)
		}
		tail = tail[min(tasks*waveChunk, len(tail)):]
		varying = summary.union.xor(summary.intersection).and(remaining)
		if varying == remaining {
			unmaskIndices(tail, indexMask, workers)
			return varying
		}
	}
	return varying
}
