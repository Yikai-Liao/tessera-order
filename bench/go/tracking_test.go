// SPDX-License-Identifier: BSD-2-Clause

package main

import (
	"bytes"
	"encoding/json"
	"runtime"
	"strings"
	"testing"
)

func TestTrackingNoClock(t *testing.T) {
	req := request{
		Case: inputCase{Type: "u64", Distribution: "d16", Len: 32, Operation: operation{Kind: "argsort"}},
		ID:   "go/argsort/u64/d16/32/s1/serial", Implementation: "go-tessera",
		RecordBytes: 8, Threads: runtime.GOMAXPROCS(0), Batch: 1, Samples: 30,
	}
	input, err := json.Marshal(req)
	if err != nil {
		t.Fatal(err)
	}
	var output bytes.Buffer
	if err := track(bytes.NewReader(input), &output, true); err != nil || output.Len() != 0 {
		t.Fatalf("no-clock tracking: %v, output %s", err, &output)
	}
	req.Threads++
	input, _ = json.Marshal(req)
	if err := track(bytes.NewReader(input), &output, true); err == nil {
		t.Fatal("accepted the wrong worker count")
	}
	if err := track(strings.NewReader(""), &output, false); err == nil {
		t.Fatal("accepted an empty timed inventory")
	}
	values := make([]float64, 30)
	for i := range values {
		values[i] = float64(i + 1)
	}
	if got := absoluteInterval(values); got != (latency{15.5, 8, 23}) {
		t.Fatalf("wrong interval: %+v", got)
	}
}
