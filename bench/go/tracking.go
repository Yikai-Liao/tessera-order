// SPDX-License-Identifier: BSD-2-Clause

package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"runtime"
	"slices"
	"time"
)

type latency struct {
	Value float64 `json:"value"`
	Low   float64 `json:"lower_value"`
	High  float64 `json:"upper_value"`
}

func absoluteInterval(values []float64) latency {
	slices.Sort(values)
	n := len(values)
	probability, tail, k := 1/float64(uint64(1)<<n), 0.0, 0
	tail = probability
	for k+1 < n/2 {
		next := probability * float64(n-k) / float64(k+1)
		if 2*(tail+next) > .01 {
			break
		}
		tail += next
		probability = next
		k++
	}
	return latency{(values[(n-1)/2] + values[n/2]) / 2, values[k], values[n-k-1]}
}

// Tracking receives concrete catalog requests. The verified batch kernel owns
// the timed boundary in both tracking and paired execution.
func track(input io.Reader, output io.Writer, verify bool) error {
	result := map[string]map[string]latency{}
	scan := bufio.NewScanner(input)
	for scan.Scan() {
		var req request
		if err := json.Unmarshal(scan.Bytes(), &req); err != nil {
			return err
		}
		if req.Threads != runtime.GOMAXPROCS(0) || req.Batch < 1 || req.Samples < 10 || req.Samples > 60 {
			return fmt.Errorf("%s: invalid tracking controls", req.ID)
		}
		slot, route := prepare(req.Case, req, false)
		if slot.run == nil {
			return fmt.Errorf("%s: %s", req.ID, route)
		}
		if verify {
			slot.restore(1)
			slot.run()
			if err := slot.check(); err != nil {
				return err
			}
			continue
		}
		if result[req.ID] != nil {
			return fmt.Errorf("duplicate tracking case %s", req.ID)
		}
		if _, _, err := sample(slot, req.Batch, time.Duration(req.WarmNS)*time.Duration(req.Samples)); err != nil {
			return err
		}
		values := make([]float64, req.Samples)
		for i := range values {
			ns, count, err := sample(slot, req.Batch, time.Duration(req.WindowNS))
			if err != nil {
				return err
			}
			values[i] = float64(ns) / float64(count)
		}
		result[req.ID] = map[string]latency{"latency": absoluteInterval(values)}
	}
	if err := scan.Err(); err != nil {
		return err
	}
	if verify {
		return nil
	}
	if len(result) == 0 {
		return fmt.Errorf("empty tracking inventory")
	}
	return json.NewEncoder(output).Encode(result)
}
