// SPDX-License-Identifier: BSD-2-Clause

package engine

type routeStage uint8

const (
	routeProbe routeStage = iota
	routeConstant
	routeCountWindow
	routeNarrow
	routeTile
	routeBlock
	routeTileDigits
	routeNodeDigit
	routeDistribution
	routeCached
	routeIndices
	routeRank
	routeCut
	routeSpans
	routeScatter
	routeSource
	routeTransfer
	routeBand
	routeParallelEntry
	routeParallelRoot
	routeShards
	routeParallelTies
)

type routeEvent struct {
	stage   routeStage
	a, b, c int
	mask    Uint128
	digit   Digit
}
