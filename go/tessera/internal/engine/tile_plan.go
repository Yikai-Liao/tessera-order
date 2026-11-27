// SPDX-License-Identifier: BSD-2-Clause

package engine

import "math/bits"

const (
	maxTileDigitBits = 14
	minTileDigitBits = 6
	maxDigitBuckets  = maxCursorBytes / 2
)

type tilePlan struct {
	digits [2]Digit
	passes int
	msd    bool
}

func trailingBits(value Uint128) uint {
	if value.Lo != 0 {
		return uint(bits.TrailingZeros64(value.Lo))
	}
	return 64 + uint(bits.TrailingZeros64(value.Hi))
}

func topWindow(remaining Uint128, width uint) (Digit, Uint128) {
	high := uint(remaining.bitLen() - 1)
	window := lowMask(128).lsh(uint(max(0, int(high+1)-int(width))))
	low := trailingBits(remaining.and(window))
	return Digit{Shift: low, Width: high - low + 1}, window
}

// One or two flat passes minimize table slots. Longer plans inspect actual
// children instead of predicting their work.
func planDigits(varying Uint128, width uint, len int, simdMSD bool) tilePlan {
	high, window := topWindow(varying, width)
	remaining := varying.andNot(window)
	if remaining == (Uint128{}) {
		return tilePlan{digits: [2]Digit{high}, passes: 1}
	}
	low, _ := topWindow(remaining, 128)
	if low.Width > width {
		return tilePlan{digits: [2]Digit{high}, passes: 1, msd: true}
	}
	digits := [2]Digit{high, low}
	slots := (1 << high.Width) + (1 << low.Width)
	for highWidth := uint(1); highWidth < high.Width; highWidth++ {
		upper, window := topWindow(varying, highWidth)
		lower, _ := topWindow(varying.andNot(window), 128)
		if lower.Width <= width {
			cost := (1 << upper.Width) + (1 << lower.Width)
			if cost < slots {
				digits = [2]Digit{upper, lower}
				slots = cost
			}
		}
	}
	msdSlots := 1 << high.Width
	if simdMSD && msdSlots >= len && slots > msdSlots {
		return tilePlan{digits: [2]Digit{high}, passes: 1, msd: true}
	}
	return tilePlan{digits: digits, passes: 2}
}

// Cap the table by both the cursor budget and four buckets per record.
func tileDigitBits(length, recordBytes int) uint {
	widest := bits.Len(uint(max(1, length*recordBytes/2))) - 1
	byRecords := bits.Len(uint(max(length, 1))) + 1
	return uint(max(min(widest, byRecords, maxTileDigitBits), minTileDigitBits))
}
