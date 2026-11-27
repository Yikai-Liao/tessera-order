// SPDX-License-Identifier: BSD-2-Clause

package main

import (
	"cmp"
	"math"
	"math/bits"
	"slices"
	"sort"
	"strconv"

	"github.com/Yikai-Liao/tessera-order/go/tessera"
)

type word = tessera.Uint128

type splitMix64 uint64

func (r *splitMix64) next() uint64 {
	*r += 0x9e3779b97f4a7c15
	z := uint64(*r)
	z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9
	z = (z ^ (z >> 27)) * 0x94d049bb133111eb
	return z ^ (z >> 31)
}

func floatOrder(raw uint64) uint64 {
	return raw ^ (uint64(int64(raw)>>63) | 1<<63)
}

func wordOrder(a, b word) int {
	if a.Hi != b.Hi {
		return cmp.Compare(a.Hi, b.Hi)
	}
	return cmp.Compare(a.Lo, b.Lo)
}

func smallKey(value uint64, width int, floats bool) word {
	if floats {
		return word{Lo: math.Float64bits(float64(value))}
	}
	if width == 32 {
		value = uint64(uint32(value))
	}
	return word{Lo: value}
}

// The upper bound is 2048, so the tenth power fits in 110 bits.
func tenthRoot(k uint64) uint64 {
	lo, hi := uint64(256), uint64(2048)
	target := word{Hi: k << 16}
	for lo+1 < hi {
		mid := (lo + hi) / 2
		power := word{Lo: 1}
		for range 10 {
			carry, low := bits.Mul64(power.Lo, mid)
			power = word{Hi: power.Hi*mid + carry, Lo: low}
		}
		if wordOrder(power, target) <= 0 {
			lo = mid
		} else {
			hi = mid
		}
	}
	return lo
}

// drawKeys ports bench/rust/src/case.rs, retaining raw floating-point bits.
func drawKeys(distribution string, n, width int, floats bool) []word {
	rng := splitMix64(1)
	draw := func() word {
		if width == 128 {
			return word{Hi: rng.next(), Lo: rng.next()}
		}
		raw := rng.next()
		if width == 32 {
			raw = uint64(uint32(raw))
		}
		return word{Lo: raw}
	}
	small := func(u uint64) word { return smallKey(u, width, floats) }
	high := func() word {
		if floats {
			return word{Lo: 0x7ff0000000000000 | rng.next()>>12}
		}
		value := draw()
		if width == 128 {
			value.Hi |= 1 << 63
		} else {
			value.Lo |= 1 << (width - 1)
		}
		return value
	}
	tenth := func(k uint64) word {
		switch {
		case floats:
			return word{Lo: math.Float64bits(float64(k) * math.Float64frombits(0x7fb0000000000000))}
		case width == 128:
			// (2^128 - 1) / 10 ends in repeating hexadecimal nines.
			carry, low := bits.Mul64(0x9999999999999999, k)
			return word{Hi: 0x1999999999999999*k + carry, Lo: low}
		case width == 32:
			return word{Lo: (math.MaxUint32 / 10) * k}
		default:
			return word{Lo: (math.MaxUint64 / 10) * k}
		}
	}
	values := make([]word, n)
	switch distribution {
	case "random", "finite_signed", "runs":
		for i := range values {
			if distribution == "finite_signed" && floats {
				sign := rng.next() & (1 << 63)
				exponent := 1000 + rng.next()%30
				values[i].Lo = sign | (exponent << 52) | (rng.next() >> 12)
			} else {
				values[i] = draw()
			}
		}
		if distribution == "runs" {
			order := wordOrder
			if floats {
				order = func(a, b word) int { return cmp.Compare(floatOrder(a.Lo), floatOrder(b.Lo)) }
			}
			for start := 0; start < n; {
				end := min(n, start+16+int(rng.next()%2048))
				run := values[start:end]
				slices.SortFunc(run, order)
				if rng.next()>>63 == 1 {
					slices.Reverse(run)
				}
				start = end
			}
		}
	case "d16", "d256", "d4096":
		count, _ := strconv.ParseUint(distribution[1:], 10, 64)
		for i := range values {
			values[i] = small(rng.next() % count)
		}
	case "zipf1.1", "zipf2":
		cdf := make([]uint64, n)
		var total uint64
		for i := range cdf {
			k := uint64(i + 1)
			if distribution == "zipf2" {
				total += (1 << 62) / (k * k)
			} else {
				total += (1 << 48) / (k * tenthRoot(k))
			}
			cdf[i] = total
		}
		for i := range values {
			r, _ := bits.Mul64(rng.next(), total)
			values[i] = small(uint64(sort.Search(n, func(k int) bool { return cdf[k] > r })))
		}
	case "skew99", "hot50", "two_values":
		for i := range values {
			switch distribution {
			case "skew99":
				if rng.next()%100 >= 99 {
					values[i] = tenth(1 + rng.next()%9)
				}
			case "hot50":
				if rng.next()>>63 == 1 {
					values[i] = high()
				}
			case "two_values":
				values[i] = tenth(9 * (rng.next() >> 63))
			}
		}
	case "sparse8":
		var pool [8]word
		for i := range pool {
			pool[i] = draw()
		}
		for i := range values {
			values[i] = pool[rng.next()%8]
		}
	case "sparse_digits", "bimodal":
		for i := range values {
			if distribution == "sparse_digits" {
				values[i] = small(rng.next() & 0x5555555555555555)
			} else {
				cluster := (rng.next() & 1) << 63
				values[i] = small(cluster | (rng.next() & ((1 << 40) - 1)))
			}
		}
	case "sorted", "reverse", "sorted_tail", "sorted99", "far_swaps", "equal":
		for i := range values {
			switch {
			case distribution == "reverse":
				values[i] = small(uint64(n - 1 - i))
			case distribution == "equal":
				values[i] = small(1)
			case distribution == "sorted_tail" && i >= n-n/128:
				values[i] = high()
			default:
				values[i] = small(uint64(i))
			}
		}
		switch distribution {
		case "sorted99":
			step := max(2, n/max(1, n/100))
			for i := 0; i+1 < n; i += step {
				values[i], values[i+1] = values[i+1], values[i]
			}
		case "far_swaps":
			if n > 1 {
				quarter := uint64(max(1, n/4))
				for range max(1, n/100) {
					a := int(rng.next() % quarter)
					b := n - 1 - int(rng.next()%quarter)
					values[a], values[b] = values[b], values[a]
				}
			}
		}
	default:
		panic("unknown distribution " + distribution)
	}
	return values
}
