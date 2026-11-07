# SPDX-License-Identifier: BSD-2-Clause
"""Source-only metadata tests. Not executed yet."""

import copy
import unittest

from probe import build_graph, read_capture, simulate


def fixture(block, sources, *, shards=1, radix=2):
    """Make abstract prefix geometry, not claimed native scatter evidence."""
    scratch = 2 * shards * radix
    n = sum(length for _, _, length in sources)
    nodes = []
    destination = 0
    for index, (allocation, physical_index, length) in enumerate(sources):
        physical = physical_index if allocation == "scratch" else scratch + physical_index
        segment = physical_index // (2 * radix) if allocation == "scratch" else 0
        offset = (physical_index % (2 * radix) if allocation == "scratch" else physical_index) * block
        nodes.append({
            "node": index, "bucket": 0, "physical": physical,
            "allocation": allocation, "segment": segment,
            "offset_records": offset, "length": length,
            "destination": destination,
        })
        destination += length
    return {
        "schema": "native-live-prefix-v1",
        "hook": "adopted_before_permute_blocks",
        "n": n, "record_bytes": 16, "root_shards": shards,
        "workers": max(2, shards), "radix": radix, "block_records": block,
        "scratch_segment_capacities_records": [2 * radix * block] * shards,
        "semantic_ends": [n] * radix, "nodes": nodes,
    }


class GraphTests(unittest.TestCase):
    def test_identity_is_ready(self):
        result = simulate(fixture(4, [("input", 0, 4), ("input", 1, 4)]))
        self.assertTrue(result["accepted"])
        self.assertEqual(result["edges"], 0)
        self.assertEqual(result["self_overlap_records"], 8)
        self.assertEqual(result["post_scatter_copy_records"], 8)

    def test_cycle_needs_one_spill(self):
        capture = fixture(4, [("input", 1, 4), ("input", 0, 4)])
        result = simulate(capture, test_slot_cap=1)
        self.assertTrue(result["accepted"])
        self.assertEqual(result["peak_live_spill_slots"], 1)
        self.assertEqual(result["total_spill_records"], 4)
        self.assertEqual(result["post_scatter_copy_records"], 12)

    def test_low_cap_declines(self):
        capture = fixture(4, [("input", 1, 4), ("input", 0, 4)])
        result = simulate(capture, test_slot_cap=0)
        self.assertFalse(result["accepted"])
        self.assertEqual(result["final_copy_records"], 0)
        self.assertEqual(result["rejection"], "frozen_live_spill_capacity")

    def test_self_overlap_memmove(self):
        result = simulate(fixture(4, [("scratch", 0, 1), ("input", 0, 3)]))
        self.assertTrue(result["accepted"])
        self.assertEqual(result["edges"], 1)
        self.assertEqual(result["self_overlap_records"], 2)
        self.assertEqual(result["total_spill_records"], 0)

    def test_padding_is_not_live(self):
        capture = fixture(4, [("scratch", 0, 2), ("input", 0, 2), ("scratch", 1, 4)])
        geometry, nodes = read_capture(capture)
        outgoing, _, _ = build_graph(nodes, geometry["block_records"])
        self.assertEqual([(e.source, e.begin, e.end) for e in outgoing[0]], [(1, 0, 2)])
        self.assertEqual(outgoing[1], [])
        self.assertEqual(outgoing[2], [])

    def test_two_exact_dependencies(self):
        capture = fixture(4, [("scratch", 0, 2), ("scratch", 1, 4), ("input", 0, 4), ("input", 1, 4)])
        geometry, nodes = read_capture(capture)
        outgoing, _, _ = build_graph(nodes, geometry["block_records"])
        self.assertEqual([(e.source, e.begin, e.end) for e in outgoing[1]], [(2, 2, 4), (3, 4, 6)])
        self.assertTrue(simulate(capture)["accepted"])

    def test_scratch_segments_stay_separate(self):
        capture = fixture(4, [("scratch", 0, 4), ("scratch", 4, 4)], shards=2)
        self.assertTrue(simulate(capture)["accepted"])
        malformed = copy.deepcopy(capture)
        malformed["nodes"][1]["segment"] = 0
        malformed["nodes"][1]["offset_records"] = 16
        with self.assertRaisesRegex(ValueError, "separate shard"):
            read_capture(malformed)

    def test_duplicate_source_declines(self):
        with self.assertRaisesRegex(ValueError, "duplicate"):
            read_capture(fixture(4, [("input", 0, 4), ("input", 0, 4)]))

    def test_unaligned_source_declines(self):
        capture = fixture(4, [("input", 0, 4)])
        capture["nodes"][0]["offset_records"] = 1
        with self.assertRaisesRegex(ValueError, "B-aligned"):
            read_capture(capture)

    def test_wrong_hook_declines(self):
        capture = fixture(4, [("input", 0, 4)])
        capture["hook"] = "after_permute_blocks"
        with self.assertRaisesRegex(ValueError, "pre-permutation"):
            read_capture(capture)


if __name__ == "__main__":
    unittest.main()
