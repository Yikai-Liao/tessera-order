# SPDX-License-Identifier: BSD-2-Clause

"""Fetch the native benchmark sources pinned in rust/native/sources.toml."""

import hashlib
import io
import json
import shutil
import tarfile
import tempfile
import tomllib
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent


def tree_hash(root: Path) -> str:
    """Hash file names and contents in a fetched source tree."""
    digest = hashlib.sha256()
    for path in sorted(root.rglob("*")):
        if path.is_file() and path.name != ".tessera-source.json":
            digest.update(path.relative_to(root).as_posix().encode() + b"\0")
            digest.update(hashlib.sha256(path.read_bytes()).digest())
    return digest.hexdigest()


def main() -> None:
    """Download, verify and atomically install each pinned source archive."""
    sources = tomllib.loads((ROOT / "rust/native/sources.toml").read_text())["sources"]
    vendor = ROOT / "vendor"
    vendor.mkdir(exist_ok=True)
    for name, source in sources.items():
        target = vendor / name
        stamp = target / ".tessera-source.json"
        if stamp.exists():
            state = json.loads(stamp.read_text())
            if state["sha256"] == source["sha256"] and state["tree_sha256"] == tree_hash(target):
                print(f"{name}: verified {source['commit']}")
                continue
        repository = source["repository"].removeprefix("https://github.com/")
        url = f"https://codeload.github.com/{repository}/tar.gz/{source['commit']}"
        with urllib.request.urlopen(url, timeout=120) as response:
            data = response.read()
        if hashlib.sha256(data).hexdigest() != source["sha256"]:
            raise ValueError(f"{name}: source archive SHA-256 mismatch")
        with tempfile.TemporaryDirectory(dir=vendor) as temporary:
            unpacked = Path(temporary)
            with tarfile.open(fileobj=io.BytesIO(data)) as archive:
                archive.extractall(unpacked, filter="data")
            (tree,) = unpacked.iterdir()
            for license_path in source["license_files"]:
                if not (tree / license_path).is_file():
                    raise ValueError(f"{name}: missing license {license_path}")
            state = {**source, "tree_sha256": tree_hash(tree)}
            (tree / stamp.name).write_text(json.dumps(state, indent=2) + "\n")
            if target.exists():
                shutil.rmtree(target)
            tree.rename(target)
        print(f"{name}: fetched {source['commit']} ({source['license']})")


if __name__ == "__main__":
    main()
