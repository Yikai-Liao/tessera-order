// SPDX-License-Identifier: BSD-2-Clause

package tessera

import "github.com/Yikai-Liao/tessera-order/go/tessera/internal/engine"

// ParSort stably sorts supported Tessera keys using up to GOMAXPROCS workers.
// It uses O(P√n) workspace for fixed key and record widths, with P workers.
func ParSort[K any](records []K) {
	engine.DirectParallel(records, engine.Refine)
}

// ParSortByKey stably sorts records by key using up to GOMAXPROCS workers.
// The callback may run repeatedly and concurrently and its record pointer is
// valid for the duration of the call. Workspace is bounded as in ParSort.
// A key panic propagates
// after workers exit and leaves the slice contents unspecified.
func ParSortByKey[T, K any](records []T, key func(*T) K) {
	engine.KeyedParallel(records, key, engine.Refine)
}

// ParArgsort returns source indices in stable key order using up to GOMAXPROCS
// workers. It allocates the index output in addition to ParSort's workspace bound.
func ParArgsort[K any](records []K) []int {
	return engine.DirectParallel(records, engine.Indices)
}

// ParArgsortByKey returns source indices in stable key order without rearranging
// records. The callback may run repeatedly and concurrently and its record
// pointer is valid for the duration of the call. A key panic propagates after
// workers exit. Workspace and output are bounded as in ParArgsort.
func ParArgsortByKey[T, K any](records []T, key func(*T) K) []int {
	return engine.KeyedParallel(records, key, engine.Indices)
}
