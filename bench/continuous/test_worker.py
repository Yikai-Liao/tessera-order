# SPDX-License-Identifier: BSD-2-Clause
"""Tests for the continuous suite's isolated hybrid scheduler."""

import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from worker import main, measured_cases, partition, run_workers


class SchedulerTests(unittest.TestCase):
    def test_partition_balances_small_cases_and_excludes_large_cases(self):
        cases = [
            "rust/u64/random/65536/s1/api",
            "rust/u64/equal/65536/s1/api",
            "rust/u64/reverse/65536/s1/api",
            "rust/u32/random/65536/s1/api",
            "rust/u128/random/65536/s1/api",
            "rust/u64/random/1000000/s1/api",
            "rust/u64/random/10000000/s1/api",
        ]
        small, large = partition(cases)
        self.assertEqual(partition(list(reversed(cases))), (small, large))
        self.assertEqual(large, ["rust/u64/random/1000000/s1", "rust/u64/random/10000000/s1"])
        self.assertEqual(
            small,
            [
                [
                    "rust/u128/random/65536/s1",
                    "rust/u64/equal/65536/s1",
                    "rust/u64/reverse/65536/s1",
                ],
                ["rust/u32/random/65536/s1", "rust/u64/random/65536/s1"],
            ],
        )
        all_groups = [group for lane in small for group in lane] + large
        self.assertEqual(len(all_groups), len(set(all_groups)))
        self.assertEqual(set(all_groups), {case.rsplit("/", 1)[0] for case in cases})

    def test_supervisor_rejects_invalid_ids_before_spawning_workers(self):
        case = "rust/u64/random/65536/s1/api"
        for listed in ["", f"{case}\n{case}", case.replace("/api", "/unstable")]:
            with (
                patch("worker.sys.argv", ["worker", "/unused"]),
                patch("worker.subprocess.check_output", return_value=listed),
                patch("worker.run_workers") as workers,
                self.assertRaises(RuntimeError),
            ):
                main()
            workers.assert_not_called()

    def test_missing_and_duplicate_measurements(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            self.assertEqual(measured_cases([directory]), set())
            for worker in ("0", "1"):
                path = directory / worker / "criterion/case/new/benchmark.json"
                path.parent.mkdir(parents=True)
                path.write_text(json.dumps({"full_id": "same-case"}))
            with self.assertRaises(RuntimeError):
                measured_cases([directory / "0", directory / "1"])

    def test_failed_worker_is_reaped_and_sibling_is_killed(self):
        class Child:
            def __init__(self, status):
                self.status = status
                self.killed = False

            def wait(self):
                return self.status

            def poll(self):
                return self.status

            def kill(self):
                self.killed = True
                self.status = -9

        first, second = Child(1), Child(None)
        with (
            tempfile.TemporaryDirectory() as temporary,
            patch("worker.subprocess.Popen", side_effect=[first, second]),
            self.assertRaises(RuntimeError),
        ):
            run_workers(Path("/unused"), [["a"], ["b"]], [0, 1], Path(temporary))
        self.assertTrue(second.killed)


if __name__ == "__main__":
    unittest.main()
