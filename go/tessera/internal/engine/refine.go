// SPDX-License-Identifier: BSD-2-Clause

package engine

import (
	"math/bits"
	"slices"
)

// A negative rank requests every position. Movement completes every band,
// including those that don't contain the requested rank.
const allRanks = -1

func projectRank(rank, start, end int) (int, bool) {
	if observeRoutes && rank != allRanks {
		emitRoute(routeEvent{stage: routeRank, a: rank, b: start, c: end})
	}
	if rank == allRanks {
		return allRanks, start < end
	}
	return rank - start, rank >= start && rank < end
}

func topBit(varying Uint128) Uint128 {
	return (Uint128{Lo: 1}).lsh(uint(varying.bitLen() - 1))
}

func refine[T any, V encoded](records []T, e *extractor[T, V], rank, budgetBytes int) {
	if len(records) < 2 || recordBytes[T]() == 0 {
		return
	}
	tileLen := tileCapacity[T](budgetBytes)
	if rank != allRanks && len(records) > tileLen && finishCut(records, rank, e) {
		if observeRoutes {
			emitRoute(routeEvent{stage: routeCut, a: rank})
		}
		return
	}
	varying := prepare(records, e.mask, e, tileLen, true)
	if varying == (Uint128{}) {
		return
	}
	var scratch workspace[T]
	if half, narrowed, ok := narrowRoot(varying, e); ok {
		sortNode(records, narrowed, rank, tileLen, &scratch, &half)
		return
	}
	sortNode(records, varying, rank, tileLen, &scratch, e)
}

func narrowRoot[T any, V encoded](varying Uint128, e *extractor[T, V]) (extractor[T, uint64], Uint128, bool) {
	if isWide[V]() && (varying.Hi == 0 || varying.Lo == 0) {
		half := extractor[T, uint64]{encode: e.encode, decode: e.decode}
		if varying.Lo == 0 {
			half.half = 64
			varying = Uint128{Lo: varying.Hi}
		}
		half.mask = e.mask.rsh(half.half).and(lowMask(64))
		if observeRoutes {
			emitRoute(routeEvent{stage: routeNarrow, a: int(half.half), mask: varying})
		}
		return half, varying, true
	}
	return extractor[T, uint64]{}, varying, false
}

type order uint8

const (
	ascending order = iota
	descendingStrict
	descendingTies
	flatSample
	unordered
)

func probe[T any, V encoded](records []T, e *extractor[T, V]) order {
	sampled := probeOrder(records, e)
	if observeRoutes {
		emitRoute(routeEvent{stage: routeProbe, a: int(sampled), b: len(records)})
	}
	return sampled
}

func prepare[T any, V encoded](records []T, candidate Uint128, e *extractor[T, V], tileLen int, probeInsertion bool) Uint128 {
	sampled := probe(records, e)
	if probeInsertion && sampled == unordered && insertionProbe(records, e, tileLen) {
		return Uint128{}
	}
	varying, resolved := resolveVarying(records, candidate, e, sampled)
	if varying == (Uint128{}) {
		finishOrdered(records, resolved, e)
	}
	return varying
}

// Share deferred flat confirmation with the read-only argsort root.
func resolveVarying[T any, V encoded](records []T, candidate Uint128, e *extractor[T, V], sampled order) (Uint128, order) {
	if sampled <= descendingTies {
		return Uint128{}, sampled
	}
	varying := scanVarying(records, candidate, e)
	if varying == (Uint128{}) {
		return varying, ascending
	}
	if sampled == flatSample {
		confirmed := confirmOrder(records, e)
		if observeRoutes {
			emitRoute(routeEvent{stage: routeProbe, a: int(confirmed), b: len(records)})
		}
		if confirmed <= descendingTies {
			return Uint128{}, confirmed
		}
	}
	return varying, unordered
}

// A longer input estimates its insertion budget from one tile of encoded
// keys. An ordered probe window says nothing about the rest of the input.
func insertionProbe[T any, V encoded](records []T, e *extractor[T, V], tileLen int) bool {
	if len(records) <= min(insertionLimit[T](), tileLen) {
		insertionLoop(records, maxInt, e)
		return true
	}
	probeLen := min(max(tileLen, 2), len(records))
	budget := probeLen / 4
	if probeLen == len(records) {
		_, ok := insertionLoop(records, budget, e)
		return ok
	}
	keys := make([]V, probeLen)
	for i := range keys {
		keys[i] = e.get(&records[i])
	}
	probe := extractor[V, V]{encode: func(key *V) Uint128 { return keyBits(*key) }}
	swaps, ok := insertionLoop(keys, budget, &probe)
	if swaps == 0 || !ok {
		return false
	}
	hi, lo := bits.Mul(uint(swaps), uint(len(records)))
	scaled, _ := bits.Div(hi, lo, uint(probeLen))
	_, ok = insertionLoop(records, int(scaled), e)
	return ok
}

const (
	orderSamples         = 4096
	orderSampleMinStride = 128
	orderSampleMinLen    = 8192
)

// Short inputs confirm directly. Larger inputs sample adjacent pairs before
// paying for the confirming scan.
func probeOrder[T any, V encoded](records []T, e *extractor[T, V]) order {
	if len(records) < orderSampleMinLen {
		return confirmOrder(records, e)
	}
	stride := max(len(records)/orderSamples, orderSampleMinStride)
	up, down := true, true
	for i := stride; i < len(records); i += stride {
		before, after := e.get(&records[i-1]), e.get(&records[i])
		up = up && !less(after, before)
		down = down && !less(before, after)
		if !up && !down {
			return unordered
		}
	}
	if up && down {
		return flatSample
	}
	return confirmOrder(records, e)
}

// Three records are the shortest mixed-direction witness. If they tie,
// the endpoints decide which directional scan to start.
func confirmOrder[T any, V encoded](records []T, e *extractor[T, V]) order {
	first := e.get(&records[0])
	if len(records) == 1 {
		return ascending
	}
	current := e.get(&records[1])
	up, down, strict := !less(current, first), !less(first, current), first != current
	tail := records[2:]
	if len(tail) != 0 {
		after := e.get(&tail[0])
		up = up && !less(after, current)
		down = down && !less(current, after)
		strict = strict && current != after
		if !up && !down {
			return unordered
		}
		current = after
		tail = tail[1:]
	}
	if up && down && len(tail) != 0 {
		last := e.get(&tail[len(tail)-1])
		if current == last {
			middle := tail[:len(tail)-1]
			for start := 0; start < len(middle); start += 64 {
				var diff V
				for i := start; i < min(start+64, len(middle)); i++ {
					diff = or(diff, xor(e.get(&middle[i]), current))
				}
				var zero V
				if diff != zero {
					return unordered
				}
			}
			if observeRoutes {
				emitRoute(routeEvent{stage: routeConstant, a: len(records)})
			}
			return ascending
		}
		up = less(current, last)
	}
	if up {
		if scalarConfirmAscending(tail, e, current) {
			return ascending
		}
		return unordered
	}
	for i := range tail {
		after := e.get(&tail[i])
		if less(current, after) {
			return unordered
		}
		strict = strict && current != after
		current = after
	}
	if strict {
		return descendingStrict
	}
	return descendingTies
}

func finishOrdered[T any, V encoded](records []T, order order, e *extractor[T, V]) {
	if order == ascending {
		return
	}
	slices.Reverse(records)
	if order == descendingStrict {
		return
	}
	for start := 0; start < len(records); {
		key := e.get(&records[start])
		end := start + 1
		for end < len(records) && e.get(&records[end]) == key {
			end++
		}
		slices.Reverse(records[start:end])
		start = end
	}
}

func rotateLeft[T any](records []T, at int) {
	slices.Reverse(records[:at])
	slices.Reverse(records[at:])
	slices.Reverse(records)
}

func sortChild[T any, V encoded](records []T, candidate Uint128, rank, tileLen int, scratch *workspace[T], e *extractor[T, V]) {
	if len(records) < 2 || candidate == (Uint128{}) {
		return
	}
	varying := prepare(records, candidate, e, tileLen, false)
	sortNode(records, varying, rank, tileLen, scratch, e)
}

func sortNode[T any, V encoded](records []T, varying Uint128, rank, tileLen int, scratch *workspace[T], e *extractor[T, V]) {
	if len(records) < 2 || varying == (Uint128{}) {
		return
	}
	if len(records) <= tileLen {
		if observeRoutes {
			emitRoute(routeEvent{stage: routeTile, a: len(records)})
		}
		sortTile(&scratch.tile, records, varying, e)
		return
	}
	if order := sourceOrderForNode(records, varying, rank, e, recordBytes[T]()); order != nil {
		if observeRoutes {
			emitRoute(routeEvent{stage: routeSpans, a: len(records)})
		}
		applySpans(order, records)
		return
	}
	plan := planNode(records, varying, tileLen, e)
	executeNode(records, varying, rank, tileLen, scratch, e, plan)
}

func executeNode[T any, V encoded](records []T, varying Uint128, rank, tileLen int, scratch *workspace[T], e *extractor[T, V], plan nodePlan[V]) {
	traceNodePlan(plan, varying)
	if plan.boundary != 0 {
		boundary := plan.boundary
		if plan.set {
			rotateLeft(records, boundary)
			boundary = len(records) - boundary
		}
		candidate := varying.andNot(topBit(varying))
		if child, ok := projectRank(rank, 0, boundary); ok {
			sortChild(records[:boundary], candidate, child, tileLen, scratch, e)
		}
		if child, ok := projectRank(rank, boundary, len(records)); ok {
			sortChild(records[boundary:], candidate, child, tileLen, scratch, e)
		}
		return
	}

	radix, cl, candidate, finished := splitNode(plan, varying)
	if !plan.mode && candidate == (Uint128{}) && tryCountWindow(records, plan.digit, e) {
		return
	}
	// Finish requested tile-size bands while their final ranges are cache hot.
	visit := func(bucket, start, end int, band []T) {
		if len(band) <= tileLen && bucket != finished {
			if _, ok := projectRank(rank, start, end); ok {
				finishTileChild(&scratch.tile, band, candidate, e)
			}
		}
	}
	size := blockSize(len(records), recordBytes[T](), 1<<radix, serialDescriptorBytes())
	if observeRoutes {
		emitRoute(routeEvent{stage: routeBlock, a: len(records), b: int(radix), c: size})
	}
	ends := partitionBlocks(records, radix, size, &scratch.block, cl, e, visit)
	start := 0
	for bucket, end := range ends {
		if end-start > tileLen && bucket != finished {
			if child, ok := projectRank(rank, start, end); ok {
				sortChild(records[start:end], candidate, child, tileLen, scratch, e)
			}
		}
		start = end
	}
}

// Both executors consume the same classifier and remaining candidate bits.
func splitNode[V encoded](plan nodePlan[V], varying Uint128) (uint, blockClassifier[V], Uint128, int) {
	radix := plan.digit.Width
	cl := blockClassifier[V]{digit: plan.digit}
	candidate := varying.andNot(plan.digit.Mask())
	finished := -1
	if plan.mode {
		bit := topBit(varying)
		radix = 2
		candidate = varying.andNot(bit)
		split := keyBits(plan.equal).andNot(lowMask(uint(varying.bitLen()))).or(bit)
		cl = blockClassifier[V]{rank: true, equal: plan.equal, split: fromBits[V](split)}
		finished = cl.bucket(plan.equal)
	}
	return radix, cl, candidate, finished
}

func traceNodePlan[V encoded](plan nodePlan[V], varying Uint128) {
	if observeRoutes {
		kind := 0
		if plan.mode {
			kind = 1
		}
		if plan.boundary != 0 {
			kind = 2
		}
		emitRoute(routeEvent{stage: routeNodeDigit, a: kind, b: plan.boundary, digit: plan.digit, mask: varying})
	}
}
