# SPDX-License-Identifier: BSD-2-Clause
"""Pure metadata probe for native pre-permutation live-prefix relocation.

Source-only until the coordinator authorizes a shared CPU window. No records,
addresses, unsafe code, sorting kernel, or per-record arrays are used. CLI:
    python3 probe.py capture.json > result.json
Use test_probe.py only after that same authorization.
"""

from __future__ import annotations

import heapq
import json
import math
import sys
from dataclasses import dataclass


@dataclass(frozen=True)
class Node:
    """A unique original prefix and its immutable final stable interval."""

    index: int
    bucket: int
    physical: int
    allocation: str
    segment: int
    offset: int
    length: int
    destination: int


@dataclass(frozen=True)
class Edge:
    """An exact live-record overlap, never full-capacity padding."""

    writer: int
    source: int
    begin: int
    end: int


def nonnegative(value: object, name: str) -> int:
    """Require an integer rather than accepting JSON booleans as integers."""
    if type(value) is not int or value < 0:
        raise ValueError(f"{name} must be a nonnegative integer")
    return value


def read_capture(capture: dict) -> tuple[dict, list[Node]]:
    """Validate the concrete native physical-ID and allocation mapping."""
    if capture.get("schema") != "native-live-prefix-v1":
        raise ValueError("wrong schema")
    if capture.get("hook") != "adopted_before_permute_blocks":
        raise ValueError("probe requires native adopted pre-permutation metadata")
    geometry = {
        key: nonnegative(capture[key], key)
        for key in ("n", "record_bytes", "root_shards", "workers", "radix", "block_records")
    }
    n, block = geometry["n"], geometry["block_records"]
    shards, radix = geometry["root_shards"], geometry["radix"]
    if not all(geometry[key] for key in geometry if key != "n"):
        raise ValueError("nonzero geometry required")
    if shards > geometry["workers"]:
        raise ValueError("root_shards exceeds worker budget")
    head = shards * radix
    scratch_blocks = 2 * head
    full = n // block
    capacities = capture["scratch_segment_capacities_records"]
    if len(capacities) != shards:
        raise ValueError("one scratch allocation per native shard required")
    for index, capacity in enumerate(capacities):
        if nonnegative(capacity, f"capacity[{index}]") < 2 * radix * block:
            raise ValueError("scratch capacity shorter than the native segment")
    ends = [nonnegative(x, "bucket end") for x in capture["semantic_ends"]]
    if len(ends) != radix or ends != sorted(ends) or ends[-1] != n:
        raise ValueError("semantic ends do not cover the final output")
    nodes = []
    seen = set()
    destination = 0
    previous_bucket = 0
    for index, raw in enumerate(capture["nodes"]):
        if nonnegative(raw["node"], "node") != index:
            raise ValueError("node IDs must follow stable logical-prefix order")
        node = Node(
            index,
            nonnegative(raw["bucket"], "bucket"),
            nonnegative(raw["physical"], "physical"),
            raw["allocation"],
            nonnegative(raw["segment"], "segment"),
            nonnegative(raw["offset_records"], "offset_records"),
            nonnegative(raw["length"], "length"),
            nonnegative(raw["destination"], "destination"),
        )
        if node.physical in seen or node.physical >= scratch_blocks + full:
            raise ValueError("duplicate or invalid physical source block")
        seen.add(node.physical)
        if not 0 < node.length <= block:
            raise ValueError("each exported node must be a nonempty live prefix <= B")
        if node.destination != destination:
            raise ValueError("destinations must partition final output in stable order")
        if not previous_bucket <= node.bucket < radix:
            raise ValueError("semantic bucket order violated")
        lower = ends[node.bucket - 1] if node.bucket else 0
        if node.destination < lower or node.destination + node.length > ends[node.bucket]:
            raise ValueError("node crosses a semantic bucket boundary")
        previous_bucket = node.bucket
        if node.physical < scratch_blocks:
            expected_segment = node.physical // (2 * radix)
            expected_offset = (node.physical % (2 * radix)) * block
            if (node.allocation, node.segment, node.offset) != (
                "scratch", expected_segment, expected_offset
            ):
                raise ValueError("scratch must preserve separate shard allocation mapping")
            if node.offset + node.length > capacities[node.segment]:
                raise ValueError("source exceeds its scratch allocation")
        else:
            expected_offset = (node.physical - scratch_blocks) * block
            if (node.allocation, node.segment, node.offset) != ("input", 0, expected_offset):
                raise ValueError("input source must use original B-aligned global input mapping")
            if node.offset + block > full * block:
                raise ValueError("original input tail is not a full authoritative source")
        nodes.append(node)
        destination += node.length
    if destination != n or len(nodes) > full + head:
        raise ValueError("prefix count/length exceeds native directory bound")
    geometry["scratch_capacity_records"] = sum(capacities)
    geometry["nominal_root_scratch_records"] = scratch_blocks * block
    geometry["semantic_ends"] = ends
    return geometry, nodes


def build_graph(nodes: list[Node], block: int) -> tuple[list[list[Edge]], list[list[Edge]], int]:
    """Build <=2 external edges per node using exact live intersections."""
    originals = {node.offset // block: node for node in nodes if node.allocation == "input"}
    outgoing: list[list[Edge]] = [[] for _ in nodes]
    incoming: list[list[Edge]] = [[] for _ in nodes]
    self_overlap = 0
    for node in nodes:
        first = node.destination // block
        last = (node.destination + node.length - 1) // block
        if last - first > 1:
            raise ValueError("destination unexpectedly spans more than two input blocks")
        for physical_input in range(first, last + 1):
            source = originals.get(physical_input)
            if source is None:
                continue
            begin = max(node.destination, source.offset)
            end = min(node.destination + node.length, source.offset + source.length)
            if begin >= end:
                continue
            if node.index == source.index:
                self_overlap += end - begin
                continue
            edge = Edge(node.index, source.index, begin, end)
            outgoing[node.index].append(edge)
            incoming[source.index].append(edge)
    if sum(map(len, outgoing)) > 2 * len(nodes):
        raise AssertionError("external graph bound violated")
    return outgoing, incoming, self_overlap


def component_summary(outgoing: list[list[Edge]], incoming: list[list[Edge]]) -> dict:
    """Compute diagnostic SCC sizes with iterative O(V+E) traversals."""
    visited = bytearray(len(outgoing))
    finish = []
    for start in range(len(outgoing)):
        if visited[start]:
            continue
        visited[start] = 1
        stack = [(start, 0)]
        while stack:
            node, cursor = stack[-1]
            if cursor == len(outgoing[node]):
                finish.append(node)
                stack.pop()
                continue
            stack[-1] = (node, cursor + 1)
            successor = outgoing[node][cursor].source
            if not visited[successor]:
                visited[successor] = 1
                stack.append((successor, 0))
    visited = bytearray(len(outgoing))
    sizes = []
    for start in reversed(finish):
        if visited[start]:
            continue
        visited[start] = 1
        stack = [start]
        size = 0
        while stack:
            node = stack.pop()
            size += 1
            for edge in incoming[node]:
                if not visited[edge.writer]:
                    visited[edge.writer] = 1
                    stack.append(edge.writer)
        sizes.append(size)
    sizes.sort(reverse=True)
    return {
        "count": len(sizes),
        "largest_sizes": sizes[:20],
        "nontrivial_count": sum(size > 1 for size in sizes),
        "vertices_in_nontrivial_components": sum(size for size in sizes if size > 1),
        "scope": "diagnostic_only_not_a_linear_spill_lower_bound",
    }


def simulate(capture: dict, *, test_slot_cap: int | None = None) -> dict:
    """Plan metadata frontiers; reject before any possible record movement."""
    geometry, nodes = read_capture(capture)
    block, workers, n = geometry["block_records"], geometry["workers"], geometry["n"]
    ceil_sqrt = math.isqrt(n) + int(math.isqrt(n) ** 2 != n)
    frozen_cap_records = workers * ceil_sqrt
    slot_cap = frozen_cap_records // block
    if test_slot_cap is not None:
        slot_cap = nonnegative(test_slot_cap, "test_slot_cap")
    outgoing, incoming, self_overlap = build_graph(nodes, block)
    edge_count = sum(map(len, outgoing))
    # A concrete, conservative 64-bit Rust structure-of-arrays plan. This is
    # capacity accounting for a possible later kernel, NOT Python heap usage.
    # Source/destination:16V, reverse offsets:4(V+1), state:6V,
    # two frontier queues:8V, victims:4V, exact edges+reverse IDs:16E.
    # Construction source map:4(F+2PR), slot state:8K, bucket state:12R,
    # worker state:16P, and twenty 24-byte Vec headers. No event plan retained.
    radix = geometry["radix"]
    physical = n // block + 2 * geometry["root_shards"] * radix
    planned_metadata_bytes = (
        38 * len(nodes) + 16 * edge_count + 4 + 4 * physical
        + 8 * slot_cap + 12 * radix + 16 * workers + 480
    )
    total_aux_cap = 32 * workers * ceil_sqrt * geometry["record_bytes"]
    baseline_bytes = capture.get("root_and_retained_aux_capacity_bytes")
    if baseline_bytes is not None:
        baseline_bytes = nonnegative(baseline_bytes, "root_and_retained_aux_capacity_bytes")
        if baseline_bytes < geometry["scratch_capacity_records"] * geometry["record_bytes"]:
            raise ValueError("root capacity ledger omits live scratch allocations")
    planned_total_bytes = None if baseline_bytes is None else (
        baseline_bytes + planned_metadata_bytes + slot_cap * block * geometry["record_bytes"]
    )
    minimum_total_bytes = (
        geometry["scratch_capacity_records"] * geometry["record_bytes"]
        + planned_metadata_bytes + slot_cap * block * geometry["record_bytes"]
    )
    space_status = "unknown_root_capacity" if planned_total_bytes is None else (
        "fits_declared_root_phase_capacity" if planned_total_bytes <= total_aux_cap
        else "reject_total_aux_capacity"
    )
    if minimum_total_bytes > total_aux_cap:
        space_status = "reject_even_scratch_only_lower_bound"
    remaining = [len(edges) for edges in outgoing]
    original_live = [node.allocation == "input" for node in nodes]
    spilled = [False] * len(nodes)
    completed = [False] * len(nodes)
    ready = [index for index, count in enumerate(remaining) if count == 0]
    initial_ready_records = sum(nodes[index].length for index in ready)
    # A live source's incoming edges are released together, so its score stays
    # unchanged until release. No repeated SCC computation or dynamic heap.
    victims = sorted(range(len(nodes)), key=lambda i: (-len(incoming[i]), nodes[i].destination))
    victim_cursor = 0
    active_spills = peak_spills = total_spill_records = final_records = 0
    done_count = 0
    events = []
    frontier_sizes = []
    frontier_records = []
    frontier_loads = []
    pending_by_bucket = [0] * geometry["radix"]
    for node in nodes:
        pending_by_bucket[node.bucket] += 1
    ready_at_work = [0 if count == 0 else None for count in pending_by_bucket]
    copy_work = 0
    rejected = None

    def release_source(index: int) -> None:
        if not original_live[index]:
            return
        original_live[index] = False
        for edge in incoming[index]:
            if remaining[edge.writer] <= 0:
                raise AssertionError("dependency released twice")
            remaining[edge.writer] -= 1
            if remaining[edge.writer] == 0:
                ready.append(edge.writer)

    while done_count < len(nodes):
        if ready:
            frontier, ready = ready, []
            frontier_set = set(frontier)
            if len(frontier_set) != len(frontier):
                raise AssertionError("duplicate ready node")
            loads = [(0, worker) for worker in range(workers)]
            heapq.heapify(loads)
            for index in frontier:
                if completed[index] or remaining[index]:
                    raise AssertionError("nonready node entered frontier")
                if any(original_live[e.source] for e in outgoing[index]):
                    raise AssertionError("frontier would overwrite a live external source")
                if any(e.source in frontier_set for e in outgoing[index] if original_live[e.source]):
                    raise AssertionError("frontier cross-dependency")
                load, worker = heapq.heappop(loads)
                heapq.heappush(loads, (load + nodes[index].length, worker))
            moved = sum(nodes[index].length for index in frontier)
            copy_work += moved
            final_records += moved
            for index in frontier:
                completed[index] = True
                if spilled[index]:
                    active_spills -= 1
                    spilled[index] = False
                release_source(index)
                bucket = nodes[index].bucket
                pending_by_bucket[bucket] -= 1
                if pending_by_bucket[bucket] == 0:
                    ready_at_work[bucket] = copy_work
            done_count += len(frontier)
            events.append({"kind": "frontier", "nodes": frontier})
            frontier_sizes.append(len(frontier))
            frontier_records.append(moved)
            frontier_loads.append([load for load, _ in sorted(loads, key=lambda x: x[1])])
            continue
        if active_spills >= slot_cap:
            rejected = "frozen_live_spill_capacity"
            break
        while victim_cursor < len(victims) and not original_live[victims[victim_cursor]]:
            victim_cursor += 1
        if victim_cursor == len(victims):
            raise AssertionError("blocked graph has no live original source")
        index = victims[victim_cursor]
        victim_cursor += 1
        if completed[index] or spilled[index] or not incoming[index]:
            raise AssertionError("invalid evacuation victim")
        spilled[index] = True
        active_spills += 1
        peak_spills = max(peak_spills, active_spills)
        total_spill_records += nodes[index].length
        copy_work += nodes[index].length
        events.append({"kind": "evacuate", "node": index})
        release_source(index)

    if rejected is None and (final_records != n or active_spills != 0):
        raise AssertionError("successful schedule failed exact record completion")
    bucket_weights = []
    previous = 0
    for end in geometry["semantic_ends"]:
        bucket_weights.append(end - previous)
        previous = end
    quantiles = {}
    if rejected is None and n:
        ordered = sorted((ready_at_work[b], bucket_weights[b]) for b in range(len(bucket_weights)))
        for percent in (10, 25, 50, 75, 90, 99, 100):
            cumulative = 0
            for work, weight in ordered:
                cumulative += weight
                if cumulative * 100 >= percent * n:
                    quantiles[str(percent)] = work / copy_work
                    break
    lengths = {}
    for node in nodes:
        lengths[node.length] = lengths.get(node.length, 0) + 1
    return {
        "schema": "native-live-prefix-probe-v1",
        "accepted": rejected is None,
        "acceptance_scope": "dependency_policy_only_not_total_memory_or_speed",
        "rejection": rejected,
        "geometry": geometry,
        "frozen_extra_record_cap": frozen_cap_records,
        "spill_slot_cap": slot_cap,
        "test_cap_override": test_slot_cap is not None,
        "total_aux_capacity_cap_bytes": total_aux_cap,
        "planned_metadata_capacity_bytes": planned_metadata_bytes,
        "root_and_retained_aux_capacity_bytes": baseline_bytes,
        "planned_root_phase_aux_capacity_bytes": planned_total_bytes,
        "planned_aux_minimum_bytes_with_only_existing_scratch": minimum_total_bytes,
        "space_status": space_status,
        "eligible_for_kernel": rejected is None and space_status == "fits_declared_root_phase_capacity" and test_slot_cap is None,
        "vertices": len(nodes),
        "edges": edge_count,
        "strong_components": component_summary(outgoing, incoming),
        "prefix_length_histogram": lengths,
        "edge_intersections": [[e.writer, e.source, e.begin, e.end] for edges in outgoing for e in edges],
        "self_overlap_records": self_overlap,
        "initial_ready_records": initial_ready_records,
        "peak_live_spill_slots": peak_spills,
        "peak_live_spill_capacity_records": peak_spills * block,
        "total_spill_records": total_spill_records,
        "final_copy_records": final_records,
        "post_scatter_copy_records": copy_work,
        "remaining_nodes": len(nodes) - done_count,
        "remaining_live_spills": active_spills,
        "frontier_sizes": frontier_sizes,
        "frontier_records": frontier_records,
        "frontier_worker_record_loads": frontier_loads,
        "bucket_ready_copy_work": ready_at_work,
        "record_weighted_ready_copy_fraction": quantiles,
        "events": events,
        "limitations": [
            "Copy work is not time, DRAM traffic, or achieved parallelism.",
            "No records are moved; no unsafe or owned-panic proof is supplied.",
            "A same-metadata baseline schedule replay is not yet implemented.",
            "Real native export identity and stable ordering remain capture-side obligations.",
            "The pure-metadata Python object heap is diagnostic, not a Rust allocation bound.",
            "The fixed SoA ledger is a proposed allocation plan, not measured Rust Vec capacities.",
            "Concurrent-child and later-stage peaks still need the same total cap and same-core heap controls.",
        ],
    }


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: probe.py native-capture.json")
    with open(sys.argv[1], encoding="utf-8") as source:
        result = simulate(json.load(source))
    json.dump(result, sys.stdout, sort_keys=True)
    sys.stdout.write("\n")
