# SPDX-License-Identifier: BSD-2-Clause
"""Validate an immutable native TSV capture and run the metadata-only probe."""

import hashlib
import json
from pathlib import Path
import sys

from probe import read_capture, simulate


def convert(path: Path) -> dict:
    geometry = None
    allocations = {}
    ends = []
    nodes = []
    seen = set()
    source = None
    verified = False
    logical_expected = 0
    bucket = 0
    with path.open(encoding="utf-8") as file:
        for line in file:
            row = line.rstrip("\n").split("\t")
            if row[0] == "format":
                if row[1] != "tessera-adopted-v1":
                    raise ValueError("unrecognized TSV format")
            elif row[0] == "source":
                source = row[1]
            elif row[0] == "geometry":
                n, width, shards, radix, block, head, active_end, count = map(int, row[1:])
                if head != shards * radix or count != n // block + 2 * head:
                    raise ValueError("native geometry identity failed")
                geometry = (n, width, shards, radix, block, head, active_end, count)
            elif row[0] == "allocation":
                ordinal, kind, length, capacity = int(row[1]), row[2], int(row[3]), int(row[4])
                if ordinal in allocations or capacity < length:
                    raise ValueError("invalid allocation record")
                allocations[ordinal] = (kind, length, capacity)
            elif row[0] == "bucket_end":
                if int(row[1]) != len(ends):
                    raise ValueError("bucket ends out of order")
                ends.append(int(row[2]))
            elif row[0] == "input_tail":
                if geometry is None or tuple(map(int, row[1:])) != (n // block * block, n % block):
                    raise ValueError("invalid original-tail bounds")
            elif row[0] == "block":
                if geometry is None:
                    raise ValueError("block before geometry")
                logical, physical, allocation, offset, live, destination, active, partial = map(int, row[1:])
                if logical != logical_expected or physical in seen or not 0 <= physical < count:
                    raise ValueError("not a complete physical permutation")
                logical_expected += 1
                seen.add(physical)
                if bool(active) != (head <= logical < active_end):
                    raise ValueError("active range differs from geometry")
                if live < 0 or live > block or (not active and live):
                    raise ValueError("invalid live prefix")
                expected_allocation = physical // (2 * radix) + 1 if physical < 2 * head else 0
                expected_offset = physical % (2 * radix) * block if physical < 2 * head else (physical - 2 * head) * block
                if (allocation, offset) != (expected_allocation, expected_offset):
                    raise ValueError("physical allocation translation differs from source")
                if allocation not in allocations or offset + block > allocations[allocation][1]:
                    raise ValueError("full block not inside its real allocation")
                if live:
                    while bucket < len(ends) and destination >= ends[bucket]:
                        bucket += 1
                    if bucket >= len(ends):
                        raise ValueError("live prefix beyond semantic end")
                    nodes.append({
                        "node": len(nodes), "bucket": bucket, "physical": physical,
                        "allocation": "input" if allocation == 0 else "scratch",
                        "segment": 0 if allocation == 0 else allocation - 1,
                        "offset_records": offset, "length": live,
                        "destination": destination,
                    })
            elif row[0] == "directory_verified":
                verified = row[1] == "1"
            else:
                raise ValueError(f"unknown capture row {row[0]}")
    if geometry is None or not verified or logical_expected != count or len(seen) != count:
        raise ValueError("incomplete capture")
    if len(allocations) != shards + 1 or allocations[0][:2] != ("input", n):
        raise ValueError("allocation count/input identity differs")
    capture = {
        "schema": "native-live-prefix-v1", "hook": "adopted_before_permute_blocks",
        "n": n, "record_bytes": width, "root_shards": shards, "workers": shards,
        "radix": radix, "block_records": block,
        "scratch_segment_capacities_records": [allocations[i][2] for i in range(1, shards + 1)],
        "semantic_ends": ends, "nodes": nodes,
        "native_source_id": source,
        "capture_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        "capture_path": str(path),
        "worker_budget_note": "This capture is the explicitly requested W=P8 diagnostic; TSV geometry itself only records root P.",
    }
    read_capture(capture)
    return capture


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("usage: run_native_capture.py source.tsv result.json")
    path = Path(sys.argv[1])
    capture = convert(path)
    result = simulate(capture)
    result["capture_sha256"] = capture["capture_sha256"]
    result["native_source_id"] = capture["native_source_id"]
    result["capture_path"] = str(path)
    destination = Path(sys.argv[2])
    with destination.open("x", encoding="utf-8") as file:
        json.dump(result, file, sort_keys=True)
        file.write("\n")
    concise = {key: value for key, value in result.items() if key not in (
        "edge_intersections", "events", "bucket_ready_copy_work", "prefix_length_histogram",
        "frontier_sizes", "frontier_records", "frontier_worker_record_loads",
    )}
    concise["frontier_count"] = len(result["frontier_sizes"])
    concise["frontier_peak_nodes"] = max(result["frontier_sizes"], default=0)
    concise["frontier_peak_records"] = max(result["frontier_records"], default=0)
    print(json.dumps(concise, sort_keys=True, indent=2))
