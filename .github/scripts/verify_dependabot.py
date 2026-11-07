# SPDX-License-Identifier: BSD-2-Clause
"""Authenticate a Dependabot commit before granting its DCO exception."""

import json
import os
import re
import sys
import urllib.error
import urllib.request


def is_dependabot_update(event, sha, repository):
    """Require an authenticated bot update of its same-repository PR head."""
    sender = event.get("sender") or {}
    pull = event.get("pull_request") or {}
    author = pull.get("user") or {}
    source = event.get("repository") or {}
    head = pull.get("head") or {}
    base = pull.get("base") or {}
    repository_id = source.get("id")
    return (
        event.get("action") in {"opened", "synchronize"}
        and (event.get("action") != "synchronize" or event.get("after") == sha)
        and sender.get("id") == 49699333
        and sender.get("login") == "dependabot[bot]"
        and sender.get("type") == "Bot"
        and author.get("id") == 49699333
        and author.get("login") == "dependabot[bot]"
        and author.get("type") == "Bot"
        and source.get("full_name") == repository
        and isinstance(repository_id, int)
        and not isinstance(repository_id, bool)
        and repository_id > 0
        and (head.get("repo") or {}).get("id") == repository_id
        and (base.get("repo") or {}).get("id") == repository_id
        and head.get("sha") == sha
    )


def is_verified_dependabot(commit, sha):
    """Check the exact commit's attribution and GitHub-verified signature."""
    author = commit.get("author") or {}
    committer = commit.get("committer") or {}
    metadata = commit.get("commit") or {}
    verification = metadata.get("verification") or {}
    # A web-flow signature alone does not prove the attributed author pushed
    # the commit. The authenticated update event is checked separately.
    return (
        commit.get("sha") == sha
        and author.get("id") == 49699333
        and author.get("login") == "dependabot[bot]"
        and author.get("type") == "Bot"
        and committer.get("id") == 19864447
        and committer.get("login") == "web-flow"
        and (metadata.get("author") or {}).get("email")
        == "49699333+dependabot[bot]@users.noreply.github.com"
        and (metadata.get("committer") or {}).get("email") == "noreply@github.com"
        and verification.get("verified") is True
        and verification.get("reason") == "valid"
    )


def verify_commit(sha):
    """Read GitHub's verification for the exact SHA, failing closed on errors."""
    repository = os.environ.get("GITHUB_REPOSITORY", "")
    token = os.environ.get("GH_TOKEN", "")
    if not (
        re.fullmatch(r"[0-9a-f]{40}", sha)
        and re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository)
        and token
        and os.environ.get("GITHUB_EVENT_NAME") == "pull_request"
        and os.environ.get("GITHUB_ACTOR_ID") == "49699333"
    ):
        return False
    try:
        with open(os.environ.get("GITHUB_EVENT_PATH", "")) as source:
            if not is_dependabot_update(json.load(source), sha, repository):
                return False
    except (OSError, ValueError, AttributeError, TypeError):
        return False
    request = urllib.request.Request(
        f"https://api.github.com/repos/{repository}/commits/{sha}",
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=15) as response:
            return is_verified_dependabot(json.load(response), sha)
    except (urllib.error.URLError, OSError, ValueError, AttributeError, TypeError):
        # Do not print the request or credentials. An unavailable API grants
        # no exception, so the caller still enforces the ordinary DCO rule.
        return False


if __name__ == "__main__":
    sys.exit(0 if len(sys.argv) == 2 and verify_commit(sys.argv[1]) else 1)
