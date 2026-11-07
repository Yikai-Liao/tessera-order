# SPDX-License-Identifier: BSD-2-Clause
"""Check that a pull request title follows the Conventional Commits rules in CONTRIBUTING.md."""

import os
import re
import sys

TYPES = ("feat", "fix", "perf", "refactor", "docs", "test", "ci", "build", "chore", "revert")
SCOPES = ("engine", "block", "key", "simd", "site", "bench", "fuzz", "deps")
# The squash merge appends " (#1234)", and the commit summary must stay within
# 72 characters.
MAX_TITLE_LEN = 64

TITLE = re.compile(r"(?P<type>[a-z]+)(?:\((?P<scope>[a-z]+)\))?!?: (?P<summary>\S.*)")


def title_errors(title):
    """Return the rules `title` breaks, or an empty list if it follows them all."""
    match = TITLE.fullmatch(title)
    if not match:
        return ["expected `type(scope)!: summary`, with the scope and `!` optional"]
    errors = []
    if match["type"] not in TYPES:
        errors.append(f"type `{match['type']}` is not one of {', '.join(TYPES)}")
    if match["scope"] and match["scope"] not in SCOPES:
        errors.append(f"scope `{match['scope']}` is not one of {', '.join(SCOPES)}")
    if title.endswith("."):
        errors.append("the summary ends with a period")
    if len(title) > MAX_TITLE_LEN:
        errors.append(f"{len(title)} characters, over {MAX_TITLE_LEN}")
    return errors


def main():
    title = os.environ["PR_TITLE"]
    errors = title_errors(title)
    for error in errors:
        print(f"::error::Pull request title {title!r}: {error}")
    sys.exit(1 if errors else 0)


if __name__ == "__main__":
    main()
