# SPDX-License-Identifier: BSD-2-Clause
"""Regression tests for the isolated concurrency experiment."""

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

from collect import payloads
from concurrency import estimates, plan

spec = importlib.util.spec_from_file_location("analysis", Path(__file__).with_name("analyze.py"))
analysis = importlib.util.module_from_spec(spec)
spec.loader.exec_module(analysis)


class ExperimentTests(unittest.TestCase):
    def test_whole_groups_have_one_fixed_lane(self):
        cases = [
            f"rust/u64/random/{length}/s1/{mode}"
            for length in (49, 8193, 65536, 1000000, 10000000)
            for mode in ("api", "workspace")
        ]
        batches = plan(cases)
        groups = [group for batch in batches for lane in batch for group in lane]
        self.assertEqual(len(groups), len(set(groups)))
        self.assertEqual(set(groups), {case.rsplit("/", 1)[0] for case in cases})
        self.assertEqual(batches, plan(list(reversed(cases))))

    def test_missing_estimates_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory, self.assertRaises(RuntimeError):
            estimates(Path(directory), ["missing"])

    def test_collect_nested_stderr(self):
        self.assertEqual(
            list(payloads({"output": {"stderr": 'log\nEXPERIMENT_DATA={"batch":2}\n'}})),
            [{"batch": 2}],
        )

    def test_paired_denominator_and_aa_range(self):
        def phase(value, seconds):
            return {"seconds": seconds, "cases": {"case": {"value": value}}}

        trial = {
            "round": 0,
            "batch": 0,
            "binary_sha256": "same",
            "phases": {
                "a1": phase(90, 20),
                "a2": phase(110, 20),
                "w2": phase(120, 12),
                "w4": phase(150, 8),
            },
        }
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "trial.raw.json"
            path.write_text(json.dumps(trial))
            result = analysis.summarize([path])
            self.assertIn("0.900..1.100", result)
            self.assertIn("1.200 (1.200..1.200)", result)
            self.assertIn("1.500 (1.500..1.500)", result)
            with self.assertRaises(ValueError):
                analysis.summarize([path, path])


if __name__ == "__main__":
    unittest.main()
