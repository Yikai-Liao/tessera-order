// SPDX-License-Identifier: BSD-2-Clause

package main

import (
	"bytes"
	"cmp"
	"encoding/json"
	"math"
	"os"
	"runtime"
	"slices"
	"strings"
	"testing"
)

func TestInputVectors(t *testing.T) {
	data, err := os.ReadFile("../../conformance/vectors/benchmark-inputs.json")
	if err != nil {
		t.Fatal(err)
	}
	var fixture struct {
		Schema  int
		Seed    int
		Lengths []int
		Types   map[string]struct {
			RecordBytes   int `json:"record_bytes"`
			Distributions map[string][]string
		}
	}
	if err := json.Unmarshal(data, &fixture); err != nil || fixture.Schema != 2 || fixture.Seed != 1 ||
		len(fixture.Lengths) == 0 || len(fixture.Types) == 0 {
		t.Fatalf("invalid input vectors: %v", err)
	}
	for ty, spec := range fixture.Types {
		for distribution, hashes := range spec.Distributions {
			if len(hashes) != len(fixture.Lengths) {
				t.Fatalf("%s/%s: expected a digest per length", ty, distribution)
			}
			for i, length := range fixture.Lengths {
				c := inputCase{Type: ty, Distribution: distribution, Len: length, Operation: operation{Kind: "sort"}}
				p, route := prepare(c, request{Implementation: "go-tessera", Threads: 1, RecordBytes: spec.RecordBytes}, false)
				if p.digest == nil {
					t.Fatalf("unsupported fixture %s/%s/%d: %s", ty, distribution, length, route)
				}
				if actual := p.digest(); actual != hashes[i] {
					t.Errorf("%s/%s/%d: got %s, expected %s", ty, distribution, length, actual, hashes[i])
				}
			}
		}
	}
}

func TestDeclines(t *testing.T) {
	for _, tc := range []struct {
		name, implementation, ty, op, reason string
		threads, width                       int
	}{
		{"parallel-operation", "go-tessera", "u64", "select_nth", "operation has no parallel route", 2, 8},
		{"serial-implementation", "go-sort-stable", "u64", "sort", "implementation is serial", 2, 8},
		{"direct-operation", "go-slices-sort", "u32", "argsort", "only direct sorting is supported", 1, 4},
		{"direct-type", "go-pdqsort", "f64", "sort", "requires direct unsigned keys", 1, 8},
		{"radix-workers", "go-radsort", "u64", "sort", "radsort caps parallel execution at eight workers", 9, 8},
		{"radix-width", "go-radsort", "u128", "sort", "radsort requires keys of at most 64 bits", 1, 16},
		{"record-width", "go-tessera", "u64", "sort", "record width differs from the catalog", 1, 4},
		{"unknown-type", "go-tessera", "invalid", "sort", "unsupported record type", 1, 8},
		{"unknown-implementation", "invalid", "u64", "sort", "unknown Go implementation", 1, 8},
		{"unknown-operation", "go-tessera", "u64", "invalid", "unknown operation", 1, 8},
	} {
		t.Run(tc.name, func(t *testing.T) {
			c := inputCase{Type: tc.ty, Operation: operation{Kind: tc.op}}
			req := request{Implementation: tc.implementation, Threads: tc.threads, Parallel: tc.threads > 1, RecordBytes: tc.width}
			p, reason := prepare(c, req, true)
			if p.bytes != 0 || reason != tc.reason {
				t.Fatalf("got %d bytes and %q, expected decline %q", p.bytes, reason, tc.reason)
			}
		})
	}
}

func TestVerifyWithoutTiming(t *testing.T) {
	old := runtime.GOMAXPROCS(1)
	defer runtime.GOMAXPROCS(old)
	c := inputCase{Type: "row_u64", Distribution: "d16", Len: 32, Operation: operation{Kind: "partition-buckets", Buckets: 17}}
	req := request{Case: c, ID: "go/partition-buckets/row_u64/d16/32/s1/17/serial",
		Implementation: "go-tessera", Threads: 1, RecordBytes: 16}
	var source, replies bytes.Buffer
	for _, action := range []string{"list", "verify"} {
		req.Action = action
		if err := json.NewEncoder(&source).Encode(req); err != nil {
			t.Fatal(err)
		}
	}
	if err := serve(&source, &replies); err != nil {
		t.Fatal(err)
	}
	for _, line := range strings.Split(strings.TrimSpace(replies.String()), "\n") {
		if !strings.Contains(line, req.ID) || !strings.Contains(line, "SortByCachedKey") || strings.Contains(line, "elapsed_ns") {
			t.Errorf("unexpected verification reply: %s", line)
		}
	}
}

func TestVerifyGates(t *testing.T) {
	for _, tc := range []struct {
		name, input, want string
	}{
		{"wrong-workers", `{"action":"verify","threads":0}`, "GOMAXPROCS differs"},
		{"unprepared-batch", `{"action":"batch","batch":1}`, "batch requires"},
		{"unprepared-paired", "warm", "paired request requires"},
		{"unknown-action", `{"action":"other"}`, "unknown worker request"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			err := serve(strings.NewReader(tc.input+"\n"), &bytes.Buffer{})
			if err == nil || !strings.Contains(err.Error(), tc.want) {
				t.Fatalf("got %v, expected %q", err, tc.want)
			}
		})
	}
}

func TestPrepareDigestIsOptIn(t *testing.T) {
	old := runtime.GOMAXPROCS(1)
	defer runtime.GOMAXPROCS(old)
	req := request{
		Action: "prepare", Case: inputCase{Type: "u64", Distribution: "random", Len: 1000, Operation: operation{Kind: "sort"}},
		Implementation: "go-tessera", RecordBytes: 8, Threads: 1,
	}
	for _, include := range []bool{false, true} {
		req.IncludeDigest = include
		var source, replies bytes.Buffer
		if err := json.NewEncoder(&source).Encode(req); err != nil {
			t.Fatal(err)
		}
		if err := serve(&source, &replies); err != nil {
			t.Fatal(err)
		}
		var reply map[string]json.RawMessage
		if err := json.Unmarshal(replies.Bytes(), &reply); err != nil {
			t.Fatal(err)
		}
		if digest, present := reply["digest"]; present != include || include && len(digest) != 18 {
			t.Fatalf("include_digest=%t: unexpected reply %s", include, replies.Bytes())
		}
	}
}

func TestArgsortVerifierRejectsSourceMutation(t *testing.T) {
	for _, mutate := range []bool{false, true} {
		values := []float64{math.Float64frombits(0x7ff8000000000001), math.Copysign(0, -1), 0}
		input := inputCase{Type: "f64", Len: len(values), Operation: operation{Kind: "argsort"}}
		slot, route := assemble(input, request{Implementation: "go-tessera", RecordBytes: 8}, records[float64, float64]{
			values: values,
			order:  func(v float64) word { return word{Lo: floatOrder(math.Float64bits(v))} },
			direct: directCalls[float64]{argsort: func(source []float64) []int {
				if mutate {
					source[1] = 0
				}
				return []int{1, 2, 0}
			}},
		})
		if slot.run == nil {
			t.Fatal(route)
		}
		err := verifyPrepared(slot)
		if mutate && (err == nil || err.Error() != "argsort modified the source records") {
			t.Fatalf("correct indices with a modified source returned %v", err)
		}
		if !mutate && err != nil {
			t.Fatalf("unchanged source returned %v", err)
		}
	}
}

func TestVerifierRejectsDamage(t *testing.T) {
	original := []row32{{4, 0}, {4, 1}, {4, 2}, {6, 3}}
	ranks := []int{0, 1, 2, 3}
	order := func(a, b row32) int { return cmp.Compare(a.key, b.key) }
	same := func(a, b row32) bool { return a == b }
	for _, kind := range []string{"sort", "select_nth", "partition-buckets"} {
		for _, pair := range [][2]int{{0, 1}, {0, 2}, {0, 3}} {
			wrong := slices.Clone(original)
			wrong[pair[0]], wrong[pair[1]] = wrong[pair[1]], wrong[pair[0]]
			if err := checkRecords(wrong, original, ranks, operation{Kind: kind, Index: 1}, order, same); err == nil {
				t.Errorf("%s accepted wrong membership/order at %v", kind, pair)
			}
		}
	}
	want := []uint64{0xfff8000000000001, 0xfff0000000000000, 1 << 63, 0, 0x7ff0000000000000, 0x7ff8000000000001}
	floats := make([]float64, len(want))
	for i, raw := range want {
		floats[len(want)-1-i] = math.Float64frombits(raw)
	}
	orderFloat := func(v float64) word { return word{Lo: floatOrder(math.Float64bits(v))} }
	call, _ := chooseCall(inputCase{Type: "f64", Operation: operation{Kind: "sort"}},
		request{Implementation: "go-radsort", Threads: 1}, records[float64, float64]{order: orderFloat})
	call(floats)
	for i, v := range floats {
		if math.Float64bits(v) != want[i] {
			t.Errorf("float rank %d: got %x, want %x", i, math.Float64bits(v), want[i])
		}
	}
}

func TestOneWorkerIsParallel(t *testing.T) {
	c := inputCase{Type: "u32", Operation: operation{Kind: "sort"}}
	req := request{Implementation: "go-tessera", Threads: 1, RecordBytes: 4}
	_, serial := prepare(c, req, true)
	req.Parallel = true
	_, parallel := prepare(c, req, true)
	if serial != "Sort" || parallel != "ParSort" {
		t.Fatalf("serial = %q, t1 = %q", serial, parallel)
	}
}

// The bucket contract reduces the full encoded key: 2^64 mod 17 is 1, which a
// low-word-only classifier would put in bucket 0.
func TestCategoryUsesTheFullKey(t *testing.T) {
	for _, tc := range []struct {
		key   word
		count int
		want  int
	}{
		{word{Hi: 1}, 17, 1},
		{word{}, 17, 0},
		{word{Hi: 1, Lo: 16}, 17, 0},
		{word{Hi: math.MaxUint64, Lo: math.MaxUint64}, 1000, 455},
	} {
		if got := category(tc.key, tc.count); got != tc.want {
			t.Errorf("category(%v, %d) = %d, want %d", tc.key, tc.count, got, tc.want)
		}
	}
}
