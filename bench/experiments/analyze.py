# SPDX-License-Identifier: BSD-2-Clause
"""Summarize paired A/A controls and concurrent timings; not an engine ranking."""

import json
import statistics
import sys
from pathlib import Path


def summarize(paths):
    rows = {}
    durations = {"serial": [], "w2": [], "w4": []}
    hashes = set()
    slots = set()
    for path in paths:
        trial = json.loads(path.read_text())
        slot = (trial["round"], trial["batch"])
        if slot in slots:
            raise ValueError(f"Duplicate trial {slot}")
        slots.add(slot)
        hashes.add(trial["binary_sha256"])
        phases = trial["phases"]
        names = set(phases["a1"]["cases"])
        if any(set(phase["cases"]) != names for phase in phases.values()):
            raise ValueError("Phase case sets differ")
        durations["serial"].append((phases["a1"]["seconds"] + phases["a2"]["seconds"]) / 2)
        for workers in ("w2", "w4"):
            durations[workers].append(phases[workers]["seconds"])
        for name in names:
            values = {key: phase["cases"][name]["value"] for key, phase in phases.items()}
            baseline = (values["a1"] + values["a2"]) / 2
            row = rows.setdefault(name, {"aa": [], "w2": [], "w4": []})
            row["aa"].extend([values["a1"] / baseline, values["a2"] / baseline])
            for workers in ("w2", "w4"):
                row[workers].append(values[workers] / baseline)
    if len(hashes) != 1:
        raise ValueError(f"Expected one timing executable, got {len(hashes)}")
    lines = [
        "# Single-job timing concurrency experiment",
        "",
        f"Trials: {len(slots)}; cases: {len(rows)}; executable SHA-256: {next(iter(hashes))}.",
        "",
        "Ratios use the mean of the bracketing serial estimates on the same CPU.",
        "A/A is the min/max of both controls divided by that mean, across trials.",
        "Samples retain the production Criterion warm-up, duration and sample count.",
        "",
        "| Workers | Total phase seconds | Wall-time ratio to serial | Cases with median shift >5% |",
        "|---|---:|---:|---:|",
    ]
    serial = sum(durations["serial"])
    lines.append(f"| 1 | {serial:.2f} | 1.000 | — |")
    for workers in ("w2", "w4"):
        elapsed = sum(durations[workers])
        changed = sum(abs(statistics.median(r[workers]) - 1) > 0.05 for r in rows.values())
        lines.append(
            f"| {workers[1:]} | {elapsed:.2f} | {elapsed / serial:.3f} | {changed}/{len(rows)} |"
        )
    lines += [
        "",
        "| Case | Repeats | A/A range | 2 workers median (range) | 4 workers median (range) |",
        "|---|---:|---:|---:|---:|",
    ]
    for name, row in sorted(rows.items()):

        def span(values):
            return f"{min(values):.3f}..{max(values):.3f}"

        lines.append(
            f"| {name} | {len(row['w2'])} | {span(row['aa'])} | "
            f"{statistics.median(row['w2']):.3f} ({span(row['w2'])}) | "
            f"{statistics.median(row['w4']):.3f} ({span(row['w4'])}) |"
        )
    lines += [
        "",
        "Phase wall time includes preparation, validation and serial Criterion analysis.",
        "This measures the recorded fixed co-runner plan, not all possible pairings.",
        "Guest CPU topology is recorded but does not prove the host physical mapping.",
        "This is an interference experiment; no engine code or production baseline changed.",
    ]
    return "\n".join(lines)


def main():
    paths = sorted(Path(sys.argv[1]).glob("*.raw.json"))
    if not paths:
        print("No complete remote trial was collected.")
        return
    print(summarize(paths))


if __name__ == "__main__":
    main()
