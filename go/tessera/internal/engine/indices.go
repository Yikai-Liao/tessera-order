// SPDX-License-Identifier: BSD-2-Clause

package engine

const indexBands = 1 << 8

// Fill the output permutation, or describe source spans that the caller can
// expand into that same buffer or apply to its records directly.
func orderIndices[T, R, K any, V encoded, M, I sourceMode](values []T, recordLen int, e *extractor[T, R, K, V, M], indexed *extractor[int, R, K, V, I]) ([]int, sourceOrder) {
	length := len(values)
	indices := make([]int, length)
	for i := range indices {
		indices[i] = i
	}
	if length < 2 {
		return indices, nil
	}

	tileLen := tileCapacity[int](tileBytes)
	varying := prepare(indices, e.mask, indexed, tileLen, true)
	if varying == (Uint128{}) {
		return indices, nil
	}
	var scratch workspace[int]
	if length <= tileLen {
		sortTile(&scratch.tile, indices, varying, indexed)
		return indices, nil
	}
	if order := sourceOrderForNode(values, varying, allRanks, e, recordLen); order != nil {
		return indices, order
	}
	low := trailingBits(varying)
	digitShift := uint(max(varying.bitLen()-8, int(low)))
	lower := varying.and(lowMask(digitShift))
	indexBits := uint((Uint128{Lo: uint64(length - 1)}).bitLen())
	indexMask := int(uint(1)<<indexBits - 1)
	// Dense high-byte bands amortize sorting a packed prefix before
	// extracting more source keys for the remaining ties.
	var prefixBits uint
	if (length-1)/indexBands+1 > indexBands {
		prefixBits = min(indexBits, wordBits-indexBits)
	}
	prefixShift := uint(max(lower.bitLen()-int(prefixBits), int(low)))
	prefixMask := lower.rsh(prefixShift)
	ends := scatterIndices(values, indices, e, digitShift, prefixShift, indexBits, prefixMask != (Uint128{}))
	if lower == (Uint128{}) {
		return indices, nil
	}
	remaining := lower.and(lowMask(prefixShift))
	// Index prefixes occupy a machine word. Casting the packed word to uint
	// before shifting preserves its high bit when Go's int is signed.
	prefix := extractor[int, int, uint, uint64, prefixIndexMode]{shift: indexBits}
	start := 0
	for _, end := range ends {
		if end == start {
			continue
		}
		band := indices[start:end]
		start = end
		if prefixMask == (Uint128{}) {
			sortChild(band, remaining, allRanks, tileLen, &scratch, indexed)
			continue
		}
		sortChild(band, prefixMask, allRanks, tileLen, &scratch, &prefix)
		for at := 0; at < len(band); {
			word := uint(band[at]) >> indexBits
			stop := at + 1
			for stop < len(band) && uint(band[stop])>>indexBits == word {
				stop++
			}
			ties := band[at:stop]
			for i := range ties {
				ties[i] &= indexMask
			}
			sortChild(ties, remaining, allRanks, tileLen, &scratch, indexed)
			at = stop
		}
	}
	return indices, nil
}

func shiftedWord[V encoded](key V, shift uint) uint {
	if isWide[V]() {
		return uint(any(key).(Uint128).rsh(shift).Lo)
	}
	return uint(lowWord(key) >> shift)
}

// Changed extractor results may invalidate the first-pass counts. A
// mismatched directory panics while the index output is still private.
func scatterIndices[T, R, K any, V encoded, M sourceMode](records []T, words []int, e *extractor[T, R, K, V, M], shift, prefixShift, indexBits uint, pack bool) [indexBands]int {
	digit := Digit{shift, 8}
	var counts, ends, next [indexBands]int
	for i := range records {
		counts[keyDigit(e.get(&records[i]), digit)]++
	}
	start := 0
	for bucket, count := range counts {
		next[bucket] = start
		start += count
		ends[bucket] = start
	}
	for i := range records {
		key := e.get(&records[i])
		bucket := keyDigit(key, digit)
		slot := next[bucket]
		if slot >= ends[bucket] {
			panic("tessera: inconsistent radix partition")
		}
		word := i
		if pack {
			word |= int(shiftedWord(key, prefixShift) << indexBits)
		}
		words[slot] = word
		next[bucket]++
	}
	if next != ends {
		panic("tessera: inconsistent radix partition")
	}
	return ends
}

// Mark finished positions so the cycles take linear time together.
func applyCycles[T any](records []T, indices []int) {
	for start := range indices {
		position := start
		for {
			source := indices[position]
			indices[position] = position
			if source == start {
				break
			}
			records[position], records[source] = records[source], records[position]
			position = source
		}
	}
}
