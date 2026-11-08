# SPDX-License-Identifier: BSD-2-Clause
"""Run bracketed serial/parallel Criterion trials inside one Bencher job."""

import argparse
import hashlib
import json
import os
import subprocess
import sys
import time
from pathlib import Path


def plan(case_ids, batches=4):
    """Balance whole API/workspace groups over fixed CPU lanes and batches."""
    groups = {}
    for case in sorted(case_ids):
        group = case.rsplit("/", 1)[0]
        weight = 5 if int(case.split("/")[3]) >= 1_000_000 else 2
        groups.setdefault(group, []).append((case, weight))
    bins = [[] for _ in range(batches * 4)]
    weights = [0] * len(bins)
    for group, cases in sorted(
        groups.items(), key=lambda item: (-sum(w for _, w in item[1]), item[0])
    ):
        slot = min(range(len(bins)), key=lambda i: (weights[i], i))
        bins[slot].append(group)
        weights[slot] += sum(w for _, w in cases)
    return [bins[i : i + 4] for i in range(0, len(bins), 4)]


def estimates(directory, expected):
    """Read full-precision estimates and raw samples, rejecting missing cases."""
    results = {}
    for path in directory.rglob("new/estimates.json"):
        benchmark = json.loads((path.parent / "benchmark.json").read_text())
        name = benchmark["full_id"]
        data = json.loads(path.read_text())
        estimate = data.get("slope") or data["mean"]
        results[name] = {
            "value": estimate["point_estimate"],
            "lower_value": estimate["confidence_interval"]["lower_bound"],
            "upper_value": estimate["confidence_interval"]["upper_bound"],
            "samples": json.loads((path.parent / "sample.json").read_text()),
        }
    if set(results) != set(expected):
        raise RuntimeError(
            f"Wrong result set: missing={set(expected) - set(results)}, "
            f"unexpected={set(results) - set(expected)}"
        )
    return results


def phase(binary, groups, cases, cpus, workers, directory):
    """Keep each group's CPU fixed while changing only simultaneous lanes."""
    results = {}
    children = []
    started = time.monotonic()
    try:
        for offset in range(0, 4, workers):
            children = []
            for lane in range(offset, offset + workers):
                output = directory / str(lane)
                output.mkdir(parents=True)
                env = dict(
                    os.environ,
                    TESSERA_EXPERIMENT_GROUPS="\n".join(groups[lane]),
                    CRITERION_HOME=str(output / "criterion"),
                )
                log = (output / "child.log").open("w")
                cpu = cpus[lane]
                child = subprocess.Popen(
                    [str(binary), "--bench"],
                    env=env,
                    stdout=log,
                    stderr=subprocess.STDOUT,
                    preexec_fn=lambda cpu=cpu: os.sched_setaffinity(0, {cpu}),
                )
                children.append((child, log, output, lane))
            for child, log, output, lane in children:
                status = child.wait(timeout=220)
                log.close()
                if status:
                    raise RuntimeError(
                        f"Lane {lane} exited {status}: {(output / 'child.log').read_text()}"
                    )
                expected = [case for case in cases if case.rsplit("/", 1)[0] in groups[lane]]
                lane_results = estimates(output / "criterion", expected)
                if results.keys() & lane_results.keys():
                    raise RuntimeError("Duplicate benchmark IDs")
                results.update(lane_results)
        return {"seconds": time.monotonic() - started, "cases": results}
    finally:
        for child, log, _, _ in children:
            if child.poll() is None:
                child.kill()
                child.wait()
            log.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--batch", type=int, required=True, choices=range(4))
    parser.add_argument("--round", type=int, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve()
    env = dict(os.environ, TESSERA_EXPERIMENT_LIST="1")
    listed = subprocess.check_output([str(binary), "--bench"], env=env, text=True)
    cases = sorted(line for line in listed.splitlines() if line.startswith("rust/"))
    if len(cases) != 56 or len(set(cases)) != 56:
        raise RuntimeError(f"Expected 56 distinct cases, got {len(cases)}")
    lanes = plan(cases)[args.batch]
    selected_groups = {group for lane in lanes for group in lane}
    selected = [case for case in cases if case.rsplit("/", 1)[0] in selected_groups]
    cpus = sorted(os.sched_getaffinity(0))[:4]
    if len(cpus) != 4:
        raise RuntimeError(f"Need four available CPUs, got {cpus}")
    metadata = {
        "schema": 1,
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "round": args.round,
        "batch": args.batch,
        "cpus": cpus,
        "lanes": lanes,
        "cpuinfo": Path("/proc/cpuinfo").read_text(),
        "topology": {
            str(cpu): {
                key: (Path(f"/sys/devices/system/cpu/cpu{cpu}/topology") / key).read_text().strip()
                for key in ("physical_package_id", "core_id", "thread_siblings_list")
                if (Path(f"/sys/devices/system/cpu/cpu{cpu}/topology") / key).exists()
            }
            for cpu in cpus
        },
        "phases": {},
    }
    order = [2, 4] if args.round % 2 == 0 else [4, 2]
    phases = [("a1", 1), *((f"w{w}", w) for w in order), ("a2", 1)]
    for label, workers in phases:
        print(f"Starting {label}: {len(selected)} cases, {workers} workers", file=sys.stderr)
        metadata["phases"][label] = phase(
            binary, lanes, selected, cpus, workers, Path("/results") / label
        )
    # Bencher captures stderr separately from the BMF on stdout. The trusted
    # workflow archives this payload; no credentials are sent into the guest.
    print("EXPERIMENT_DATA=" + json.dumps(metadata, separators=(",", ":")), file=sys.stderr)
    bmf = {}
    for label, result in metadata["phases"].items():
        bmf[f"experiment/phase/{label}"] = {"wall_seconds": {"value": result["seconds"]}}
        for case, estimate in result["cases"].items():
            bmf[f"experiment/{case}/{label}"] = {
                "latency": {k: v for k, v in estimate.items() if k != "samples"}
            }
    print(json.dumps(bmf))


if __name__ == "__main__":
    main()
