# SPDX-License-Identifier: BSD-2-Clause
"""Replay both source patches in a disposable export, without building or timing."""

import argparse
import hashlib
import io
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile


def sha256(value):
    return hashlib.sha256(value).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("repository", help="Git repository containing the declared public commit")
    parser.add_argument("--output", help="Write a small relative-path verification report")
    arguments = parser.parse_args()
    packet = Path(__file__).resolve().parent
    source_map = json.loads((packet / "source-map.json").read_text())
    repository = Path(arguments.repository).resolve()
    commit = source_map["public_commit"]
    resolved = subprocess.check_output(
        ["git", "-C", str(repository), "rev-parse", f"{commit}^{{commit}}"], text=True
    ).strip()
    assert resolved == commit
    for name, expected in source_map["patches"].items():
        assert sha256((packet / name).read_bytes()) == expected, name
    archive = subprocess.check_output([
        "git", "-C", str(repository), "archive", commit,
        "Cargo.toml", "Cargo.lock", "LICENSE", "README.md", "rust/tessera-sort",
    ])
    checked = []
    with tempfile.TemporaryDirectory(prefix="tessera-source-replay-") as temporary:
        replay = Path(temporary)
        with tarfile.open(fileobj=io.BytesIO(archive)) as bundle:
            bundle.extractall(replay, filter="data")
        for stage, patch in [
            ("public", None),
            ("combined", "patches/01-public-to-combined.patch"),
            ("draining", "patches/02-combined-to-draining.patch"),
        ]:
            if patch:
                for options in [["--check"], []]:
                    subprocess.run(
                        ["git", "-C", str(replay), "apply", *options, str(packet / patch)],
                        check=True, capture_output=True,
                    )
            actual = {
                str(path.relative_to(replay)): sha256(path.read_bytes())
                for path in (replay / "rust/tessera-sort").rglob("*.rs")
            }
            expected = source_map["stages"][stage]
            assert actual == expected, f"Rust byte map differs at stage {stage}"
            for path, checksum in source_map["public_metadata"].items():
                assert sha256((replay / path).read_bytes()) == checksum, path
            checked.append({"stage": stage, "rust_files": len(actual), "exact": True})
    report = {
        "public_commit": commit,
        "patches": source_map["patches"],
        "source_map_sha256": sha256((packet / "source-map.json").read_bytes()),
        "stages": checked,
        "public_manifests_unchanged": True,
        "builds_tests_or_timing_run": False,
        "scope": "Disposable source export only; original repository is read-only.",
    }
    text = json.dumps(report, indent=2) + "\n"
    if arguments.output:
        Path(arguments.output).write_text(text)
    print(text, end="")


if __name__ == "__main__":
    main()
