// SPDX-License-Identifier: BSD-2-Clause

package tessera_test

import (
	"fmt"

	"github.com/Yikai-Liao/tessera-order/go/tessera"
)

func ExampleSort() {
	keys := []int{4, 1, 3}
	tessera.Sort(keys)
	fmt.Println(keys)
	// Output: [1 3 4]
}

func ExampleSortByKey() {
	names := []string{"pear", "fig", "plum"}
	tessera.SortByKey(names, func(name *string) int { return len(*name) })
	fmt.Println(names)
	// Output: [fig pear plum]
}

func ExampleSortByCachedKey() {
	names := []string{"pear", "fig", "plum"}
	tessera.SortByCachedKey(names, func(name *string) int { return len(*name) })
	fmt.Println(names)
	// Output: [fig pear plum]
}

func ExampleArgsort() {
	fmt.Println(tessera.Argsort([]int{2, 1, 2}))
	// Output: [1 0 2]
}

func ExampleArgsortByKey() {
	names := []string{"pear", "fig", "plum"}
	fmt.Println(tessera.ArgsortByKey(names, func(name *string) int { return len(*name) }))
	// Output: [1 0 2]
}

func ExampleSelectNth() {
	before, nth, after := tessera.SelectNth([]int{4, 1, 3}, 1)
	fmt.Println(before, *nth, after)
	// Output: [1] 3 [4]
}

func ExampleSelectNthByKey() {
	names := []string{"peach", "fig", "plum"}
	_, nth, _ := tessera.SelectNthByKey(names, 1, func(name *string) int { return len(*name) })
	fmt.Println(*nth)
	// Output: plum
}

func ExamplePair() {
	scores := []tessera.PairKey[tessera.ReverseKey[int], int]{
		tessera.Pair(tessera.Reverse(2), 0),
		tessera.Pair(tessera.Reverse(5), 2),
		tessera.Pair(tessera.Reverse(5), 1),
	}
	fmt.Println(tessera.Argsort(scores))
	// Output: [2 1 0]
}

func ExampleUint128() {
	keys := []tessera.Uint128{{Hi: 1, Lo: 0}, {Hi: 0, Lo: 1 << 63}, {Hi: 0, Lo: 2}}
	tessera.Sort(keys)
	fmt.Println(keys)
	// Output: [{0 2} {0 9223372036854775808} {1 0}]
}

func ExampleInt128() {
	keys := []tessera.Int128{{Hi: 0, Lo: 1}, {Hi: -1, Lo: ^uint64(0)}, {Hi: -1, Lo: 0}}
	tessera.Sort(keys)
	fmt.Println(keys)
	// Output: [{-1 0} {-1 18446744073709551615} {0 1}]
}

func ExampleChar() {
	letters := []rune("éaZ")
	tessera.SortByKey(letters, func(letter *rune) tessera.CharKey { return tessera.Char(*letter) })
	fmt.Println(string(letters))
	// Output: Zaé
}

func ExampleCharKey() {
	keys := []tessera.CharKey{tessera.Char('b'), {}, tessera.Char('a')}
	indices := tessera.Argsort(keys)
	fmt.Println(indices)
	// Output: [1 2 0]
}
