# SPDX-License-Identifier: BSD-2-Clause
"""Run two pinned small-case workers, then one exclusive large-case worker."""

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path


def partition(cases):
    """Balance API cases across two small lanes; run large cases separately."""
    small = [[], []]
    large = []
    for case in sorted(cases):
        group, _ = case.rsplit("/", 1)
        if int(group.split("/")[3]) >= 1_000_000:
            large.append(group)
        else:
            lane = min(range(2), key=lambda lane: len(small[lane]))
            small[lane].append(group)
    return small, large


def run_workers(binary, lanes, cpus, directory):
    """Capture separate outputs; reap every worker before returning or failing."""
    children = []
    try:
        for lane, groups in enumerate(lanes):
            if not groups:
                continue
            output = directory / str(lane)
            output.mkdir(parents=True)
            env = dict(
                os.environ,
                TESSERA_BENCH_GROUPS="\n".join(groups),
                CRITERION_HOME=str(output / "criterion"),
            )
            log = (output / "stdout").open("wb")
            cpu = cpus[lane]
            try:
                child = subprocess.Popen(
                    [str(binary), "--bench"],
                    env=env,
                    stdout=log,
                    preexec_fn=lambda cpu=cpu: os.sched_setaffinity(0, {cpu}),
                )
            finally:
                log.close()
            children.append((child, output))
        for child, _ in children:
            if child.wait() != 0:
                raise RuntimeError("Benchmark worker failed")
        return [output for _, output in children]
    finally:
        for child, _ in children:
            if child.poll() is None:
                child.kill()
                child.wait()


def measured_cases(outputs):
    """Read Criterion's completed IDs, rejecting duplicates across workers."""
    cases = []
    for output in outputs:
        for path in (output / "criterion").rglob("new/benchmark.json"):
            cases.append(json.loads(path.read_text())["full_id"])
    if len(set(cases)) != len(cases):
        raise RuntimeError("Duplicate benchmark results")
    return set(cases)


def main():
    binary = Path(sys.argv[1]).resolve()
    if "TESSERA_BENCH_GROUPS" in os.environ or "TESSERA_BENCH_LIST" in os.environ:
        raise RuntimeError("Worker selection variables must not be set on the supervisor")
    listed = subprocess.check_output(
        [str(binary), "--bench"],
        env=dict(os.environ, TESSERA_BENCH_LIST="1"),
        text=True,
    )
    cases = listed.splitlines()
    if not cases or len(set(cases)) != len(cases):
        raise RuntimeError("Expected distinct benchmark IDs")
    for case in cases:
        parts = case.split("/")
        if len(parts) != 6 or parts[0] != "rust" or not parts[3].isdigit() or parts[5] != "api":
            raise RuntimeError(f"Invalid benchmark ID: {case}")
    cpus = sorted(os.sched_getaffinity(0))[:2]
    if len(cpus) != 2:
        raise RuntimeError("Two available CPUs are required")
    small, large = partition(cases)
    with tempfile.TemporaryDirectory(prefix="tessera-bench-") as temporary:
        directory = Path(temporary)
        outputs = run_workers(binary, small, cpus, directory / "small")
        # Large-case preparation, validation and analysis must not overlap a
        # small worker, not just the timed sort itself.
        outputs += run_workers(binary, [large], cpus, directory / "large")
        if measured_cases(outputs) != set(cases):
            raise RuntimeError("Benchmark results differ from the declared suite")
        # Criterion's multi-line records must never interleave on stdout.
        for output in outputs:
            sys.stdout.buffer.write((output / "stdout").read_bytes())


if __name__ == "__main__":
    main()
