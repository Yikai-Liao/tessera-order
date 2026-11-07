# SPDX-License-Identifier: BSD-2-Clause
"""Select language checks from a Git diff; shared files affect every implemented language."""

import argparse
import json
from pathlib import Path
import subprocess


# The documentation site's tooling; `docs.yml` builds it, and no language
# depends on it.
DOCS_SITE = ("docs/.vitepress/", "docs/public/")
DOCS_SITE_FILES = {"package.json", "package-lock.json", ".github/workflows/docs.yml"}
# No language build reads Markdown outside its own directory or images under
# `assets/`; the crate's README symlink is packaged, but `cargo package` only
# needs the file to exist. The conformance rules stay shared because every
# language's tests implement them.
PROSE_DIRS = ("assets/",)
SHARED_PROSE_DIRS = ("conformance/",)


def select(paths):
    rust = cpp = python = False
    for path in paths:
        if path.startswith(DOCS_SITE) or path in DOCS_SITE_FILES:
            continue
        if path.startswith(("rust/", "bench/rust/")) or path in {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "rustfmt.toml"}:
            rust = True
            # The binding embeds the Rust core; validate that dependency too.
            python = True
        elif path.startswith(("python/", "bench/python/")):
            python = True
        elif path.startswith(("cpp/", "bench/cpp/")) or path in {"CMakeLists.txt", "CMakePresets.json"}:
            cpp = True
        elif path.startswith(PROSE_DIRS) or (path.endswith(".md") and not path.startswith(SHARED_PROSE_DIRS)):
            continue
        else:
            # Shared semantics, vectors, reports, license and CI changes
            # validate every implemented language, including future additions.
            rust = cpp = python = True
    return {"rust": rust, "cpp": cpp, "python": python}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base")
    parser.add_argument("--head", default="HEAD")
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    if not args.base or set(args.base) == {"0"}:
        result = {"rust": True, "cpp": True, "python": True}
    else:
        paths = subprocess.check_output(
            ["git", "diff", "--no-renames", "--name-only", "-z", args.base, args.head]
        ).decode().split("\0")
        result = select(path for path in paths if path)
    result["cpp_present"] = Path("cpp/CMakeLists.txt").is_file()
    result["python_present"] = all(Path(p).is_file() for p in ("python/pyproject.toml", "python/Cargo.toml"))
    with open(args.out, "a") as output:
        for name, enabled in result.items():
            output.write(f"{name}={str(enabled).lower()}\n")
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
