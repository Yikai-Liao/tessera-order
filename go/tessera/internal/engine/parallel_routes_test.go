// SPDX-License-Identifier: BSD-2-Clause

//go:build tessera_trace

package engine

import (
	"math/rand/v2"
	"slices"
	"testing"
)

func TestParallelRootRoutes(t *testing.T) {
	events := observe(t)
	e := extractor[uint64, uint64]{encode: func(key *uint64) Uint128 { return Uint128{Lo: *key} }, mask: lowMask(64)}
	for _, test := range []struct {
		name    string
		length  int
		tileLen int
		route   int
		shards  int
		plan    int
		direct  bool
		makeKey func(int, uint64) uint64
	}{
		{"below four tiles", 255, 64, 0, 0, 0, false, func(_ int, key uint64) uint64 { return key }},
		{"four tiles", 256, 64, 1, 1, 0, false, func(_ int, key uint64) uint64 { return key }},
		{"below second task", 2*minTaskBytes/8 - 1, 64, 1, 1, 0, false, func(_ int, key uint64) uint64 { return key }},
		{"at second task", 2 * minTaskBytes / 8, 64, 1, 2, 0, false, func(_ int, key uint64) uint64 { return key }},
		{"existing runs", 256, 64, 1, 0, 2, false, func(i int, key uint64) uint64 { return key>>1 | uint64(1-i/128)<<63 }},
		{"equality", 512, 64, 1, 1, 1, false, func(i int, key uint64) uint64 {
			if i%8 != 0 {
				return 1 << 63
			}
			return key
		}},
		{"count full direct digit", 256, 64, 1, 0, 0, true, func(_ int, key uint64) uint64 { return key & 63 }},
		{"scatter full keyed digit", 256, 64, 1, 1, 0, false, func(_ int, key uint64) uint64 { return key & 63 }},
	} {
		t.Run(test.name, func(t *testing.T) {
			*events = nil
			rng := rand.New(rand.NewPCG(27, 51))
			records := make([]uint64, test.length)
			for i := range records {
				records[i] = test.makeKey(i, rng.Uint64())
			}
			want := slices.Clone(records)
			slices.Sort(want)
			extract := e
			if test.direct {
				extract.decode = func(key Uint128) uint64 { return key.Lo }
			}
			varying := scanVarying(records, e.mask, &e)
			var scratch workspace[uint64]
			sortParallelRoot(records, varying, test.tileLen, &scratch, &extract, 4)
			if !slices.Equal(records, want) {
				t.Fatal("parallel root changed direct key order")
			}
			if !recorded(*events, routeParallelRoot, test.route) {
				t.Fatalf("missing parallel root route %d", test.route)
			}
			if test.route == 1 && !recorded(*events, routeNodeDigit, test.plan) {
				t.Fatalf("missing root plan kind %d", test.plan)
			}
			var shards int
			var counted bool
			for _, event := range *events {
				if event.stage == routeShards {
					shards = event.a
				}
				counted = counted || event.stage == routeCountWindow && event.b == 4
			}
			if shards != test.shards || counted != test.direct {
				t.Fatalf("shards=%d counted=%v, want %d/%v", shards, counted, test.shards, test.direct)
			}
		})
	}
}

func TestParallelShardClamp(t *testing.T) {
	events := observe(t)
	records := make([]uint64, blockBytesFloor/8+1)
	for i := range records {
		records[i] = uint64(i & 1)
	}
	e := extractor[uint64, uint8]{encode: func(key *uint64) Uint128 { return Uint128{Lo: *key} }}
	var buffers blockBuffers[uint64]
	partitionParallel(records, 1, &buffers, blockClassifier[uint8]{digit: Digit{0, 1}}, &e, 4, 4, func(_, _ int, _ []uint64) {})
	if !slices.IsSorted(records) {
		t.Fatal("partition changed key order")
	}
	for _, event := range *events {
		if event.stage == routeShards && event.a == 1 && event.c == 0 {
			return
		}
	}
	t.Fatal("one full block should use serial scatter and completion")
}

func TestParallelArgsortTieRoutes(t *testing.T) {
	events := observe(t)
	const length = 1 << 16
	input := make([]Uint128, length)
	e := extractor[Uint128, Uint128]{encode: func(key *Uint128) Uint128 { return *key }, mask: lowMask(128)}
	rng := rand.New(rand.NewPCG(61, 3))
	for _, test := range []struct {
		name    string
		route   int
		huge    bool
		makeKey func(int, uint64) Uint128
	}{
		{"ordered", 0, false, func(i int, _ uint64) Uint128 { return Uint128{Lo: uint64(i / 2)} }},
		{"packed only", 1, false, func(_ int, key uint64) Uint128 { return Uint128{Lo: key & 127} }},
		{"prefix ties", 2, false, func(_ int, key uint64) Uint128 { return Uint128{Hi: key & 0xffff, Lo: key >> 16} }},
		{"huge prefix ties", 2, true, func(i int, key uint64) Uint128 {
			// The first 64 tied words don't vary. A later wave must
			// discover the low-word variation and the high bit.
			if i < 512 {
				key = 0
			}
			return Uint128{Hi: uint64(i&1) << 63, Lo: key}
		}},
	} {
		t.Run(test.name, func(t *testing.T) {
			*events = nil
			for i := range input {
				input[i] = test.makeKey(i, rng.Uint64())
			}
			got := argsortParallel(input, e, 4)
			want := Direct(input, Indices, allRanks)
			if !slices.Equal(got, want) {
				t.Fatal("parallel argsort changed stable order")
			}
			for _, event := range *events {
				if event.stage == routeParallelTies && event.a == test.route && (event.b > 0) == test.huge {
					return
				}
			}
			t.Fatalf("missing tie route %d with huge=%v", test.route, test.huge)
		})
	}
}
