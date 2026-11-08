# SPDX-License-Identifier: BSD-2-Clause
"""Extract raw guest measurements from an authenticated Bencher job response."""

import json
import sys
from pathlib import Path


def payloads(value):
    """Walk the response without depending on its output-wrapper schema."""
    if isinstance(value, str):
        for line in value.splitlines():
            if line.startswith("EXPERIMENT_DATA="):
                yield json.loads(line.removeprefix("EXPERIMENT_DATA="))
    elif isinstance(value, dict):
        for item in value.values():
            yield from payloads(item)
    elif isinstance(value, list):
        for item in value:
            yield from payloads(item)


def main():
    source, destination = map(Path, sys.argv[1:])
    data = list(payloads(json.loads(source.read_text())))
    if len(data) != 1:
        raise RuntimeError(f"Expected one raw guest payload, got {len(data)}")
    destination.write_text(json.dumps(data[0]))
    print(f"Collected batch {data[0]['batch']}, round {data[0]['round']}")


if __name__ == "__main__":
    main()
