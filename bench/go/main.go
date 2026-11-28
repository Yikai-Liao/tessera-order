// SPDX-License-Identifier: BSD-2-Clause

package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"runtime"
	"runtime/debug"
	"time"
)

type operation struct {
	Kind    string `json:"kind"`
	Index   int    `json:"index"`
	Buckets int    `json:"bucket_count"`
}

type inputCase struct {
	Type         string    `json:"ty"`
	Distribution string    `json:"distribution"`
	Len          int       `json:"len"`
	Operation    operation `json:"operation"`
}

type request struct {
	Action         string    `json:"action"`
	Case           inputCase `json:"case"`
	ID             string    `json:"id"`
	Implementation string    `json:"implementation"`
	RecordBytes    int       `json:"record_bytes"`
	Threads        int       `json:"threads"`
	Parallel       bool      `json:"parallel"`
	Batch          int       `json:"batch"`
	WarmNS         int64     `json:"warm_ns"`
	WindowNS       int64     `json:"window_ns"`
	Samples        int       `json:"samples"`
}

type capability struct {
	ID      string `json:"id"`
	Route   string `json:"route,omitempty"`
	Decline string `json:"decline,omitempty"`
}

func main() {
	if err := runWorker(); err != nil {
		json.NewEncoder(os.Stdout).Encode(map[string]string{"error": err.Error()})
		os.Exit(1)
	}
}

func runWorker() error {
	if len(os.Args) == 2 && (os.Args[1] == "--track" || os.Args[1] == "--verify-tracking") {
		return track(os.Stdin, os.Stdout, os.Args[1] == "--verify-tracking")
	}
	return serve(os.Stdin, os.Stdout)
}

// One process lists capabilities or prepares one slot. Stdout is protocol data.
func serve(input io.Reader, output io.Writer) error {
	scan := bufio.NewScanner(input)
	write := json.NewEncoder(output).Encode
	var slot prepared
	var config request
	var warmed, sampled bool
	for scan.Scan() {
		switch string(scan.Bytes()) {
		case "warm", "sample", "stop":
			if slot.run == nil || config.Batch < 1 || config.Samples < 1 {
				return fmt.Errorf("paired request requires a prepared case and sampling settings")
			}
			switch string(scan.Bytes()) {
			case "warm":
				if warmed {
					return fmt.Errorf("duplicate warm request")
				}
				if config.WarmNS > 0 {
					if _, _, err := sample(slot, config.Batch, time.Duration(config.WarmNS)); err != nil {
						return err
					}
				}
				warmed = true
				if err := write(map[string]any{"id": config.ID, "samples": config.Samples}); err != nil {
					return err
				}
			case "sample":
				if !warmed || sampled {
					return fmt.Errorf("unexpected sample request")
				}
				elapsed, sorts, err := sample(slot, config.Batch, time.Duration(config.WindowNS))
				if err != nil {
					return err
				}
				sampled = true
				if err := write(map[string]uint64{"elapsed_ns": elapsed, "sorts": sorts}); err != nil {
					return err
				}
			case "stop":
				if !sampled {
					return fmt.Errorf("unexpected stop request")
				}
				return write(struct{}{})
			}
			continue
		}
		var req request
		if err := json.Unmarshal(scan.Bytes(), &req); err != nil {
			return err
		}
		switch req.Action {
		case "info":
			info, ok := debug.ReadBuildInfo()
			if !ok {
				return fmt.Errorf("missing Go build information")
			}
			if err := write(info); err != nil {
				return err
			}
		case "list":
			probe, route := prepare(req.Case, req, true)
			entry := capability{ID: req.ID, Route: route}
			if probe.bytes == 0 {
				entry.Route, entry.Decline = "", route
			}
			if err := write(entry); err != nil {
				return err
			}
		case "digest":
			input, route := prepare(req.Case, req, false)
			if input.digest == nil {
				return fmt.Errorf("%s: %s", req.ID, route)
			}
			if err := write(map[string]string{"id": req.ID, "digest": input.digest()}); err != nil {
				return err
			}
		case "prepare", "verify":
			if slot.run != nil || req.Threads != runtime.GOMAXPROCS(0) {
				return fmt.Errorf("unexpected prepare or GOMAXPROCS differs from the requested workers")
			}
			var route string
			slot, route = prepare(req.Case, req, false)
			if slot.run == nil || slot.bytes < 1 {
				return fmt.Errorf("%s: %s", req.ID, route)
			}
			config = req
			result := map[string]any{"id": req.ID, "buffer_bytes": slot.bytes, "route": route}
			if req.Action == "verify" {
				if err := verifyPrepared(slot); err != nil {
					return err
				}
				result["digest"] = slot.digest()
			}
			if err := write(result); err != nil {
				return err
			}
		case "batch":
			if slot.run == nil || req.Batch < 1 {
				return fmt.Errorf("batch requires a prepared case and positive count")
			}
			elapsed, err := measureBatch(slot, req.Batch)
			if err != nil {
				return err
			}
			if err := write(map[string]uint64{"elapsed_ns": elapsed, "sorts": uint64(req.Batch)}); err != nil {
				return err
			}
		default:
			return fmt.Errorf("unknown worker request %q", req.Action)
		}
	}
	return scan.Err()
}

func measureBatch(slot prepared, count int) (uint64, error) {
	slot.restore(count)
	start := time.Now()
	slot.run()
	elapsed := time.Since(start)
	return uint64(max(1, elapsed.Nanoseconds())), slot.check()
}

func sample(slot prepared, batch int, budget time.Duration) (elapsed, sorts uint64, err error) {
	start := time.Now()
	for {
		ns, err := measureBatch(slot, batch)
		if err != nil {
			return 0, 0, err
		}
		elapsed += ns
		sorts += uint64(batch)
		if time.Since(start) >= budget {
			return elapsed, sorts, nil
		}
	}
}

func verifyPrepared(slot prepared) error {
	slot.restore(1)
	slot.run()
	return slot.check()
}
