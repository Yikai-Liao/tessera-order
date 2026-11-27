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
	if rank == allRanks {
		return allRanks, start < end
	}
	return rank - start, rank >= start && rank < end
}

func topBit(varying Uint128) Uint128 {
	return (Uint128{Lo: 1}).lsh(uint(varying.bitLen() - 1))
}

func refine[T, R, K any, V encoded, M sourceMode](records []T, e *extractor[T, R, K, V, M], rank, budgetBytes int) {
	if len(records) < 2 || recordBytes[T]() == 0 {
		return
	}
	tileLen := tileCapacity[T](budgetBytes)
	if rank != allRanks && len(records) > tileLen && finishCut(records, rank, e) {
		return
	}
	varying := prepare(records, e.mask, e, tileLen, true)
	if varying == (Uint128{}) {
		return
	}
	var scratch workspace[T]
	if isWide[V]() && (varying.Hi == 0 || varying.Lo == 0) {
		half := extractor[T, R, K, uint64, M]{key: e.key, codec: e.codec, source: e.source}
		if varying.Lo == 0 {
			half.offset = 64
			varying = Uint128{Lo: varying.Hi}
		}
		half.mask = e.mask.rsh(half.offset).and(lowMask(64))
		sortNode(records, varying, rank, tileLen, &scratch, &half)
		return
	}
	sortNode(records, varying, rank, tileLen, &scratch, e)
}

type order uint8

const (
	ascending order = iota
	descendingStrict
	descendingTies
	flatSample
	unordered
)

func prepare[T, R, K any, V encoded, M sourceMode](records []T, candidate Uint128, e *extractor[T, R, K, V, M], tileLen int, probeInsertion bool) Uint128 {
	sampled := probeOrder(records, e)
	if probeInsertion && sampled == unordered && insertionProbe(records, e, tileLen) {
		return Uint128{}
	}
	if sampled <= descendingTies {
		finishOrdered(records, sampled, e)
		return Uint128{}
	}
	varying := scanVarying(records, candidate, e)
	if varying == (Uint128{}) {
		return varying
	}
	if sampled == flatSample {
		confirmed := confirmOrder(records, e)
		if confirmed <= descendingTies {
			finishOrdered(records, confirmed, e)
			return Uint128{}
		}
	}
	return varying
}

// A longer input estimates its insertion budget from one tile of encoded
// keys. An ordered probe window says nothing about the rest of the input.
func insertionProbe[T, R, K any, V encoded, M sourceMode](records []T, e *extractor[T, R, K, V, M], tileLen int) bool {
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
	probe := extractor[V, V, V, V, directMode]{}
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
func probeOrder[T, R, K any, V encoded, M sourceMode](records []T, e *extractor[T, R, K, V, M]) order {
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
func confirmOrder[T, R, K any, V encoded, M sourceMode](records []T, e *extractor[T, R, K, V, M]) order {
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

func finishOrdered[T, R, K any, V encoded, M sourceMode](records []T, order order, e *extractor[T, R, K, V, M]) {
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

func sortChild[T, R, K any, V encoded, M sourceMode](records []T, candidate Uint128, rank, tileLen int, scratch *workspace[T], e *extractor[T, R, K, V, M]) {
	if len(records) < 2 || candidate == (Uint128{}) {
		return
	}
	varying := prepare(records, candidate, e, tileLen, false)
	sortNode(records, varying, rank, tileLen, scratch, e)
}

func sortNode[T, R, K any, V encoded, M sourceMode](records []T, varying Uint128, rank, tileLen int, scratch *workspace[T], e *extractor[T, R, K, V, M]) {
	if len(records) < 2 || varying == (Uint128{}) {
		return
	}
	if len(records) <= tileLen {
		sortTile(&scratch.tile, records, varying, e)
		return
	}
	if order := sourceOrderForNode(records, varying, rank, e, recordBytes[T]()); order != nil {
		applySpans(order, records)
		return
	}
	plan := planNode(records, varying, tileLen, e)
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
	} else if candidate == (Uint128{}) && tryCountWindow(records, plan.digit, e) {
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
