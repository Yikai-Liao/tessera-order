# SPDX-License-Identifier: BSD-2-Clause
"""Test the per-commit exception and keep ordinary DCO checks intact."""

import copy
import io
import json
import os
import subprocess
import tempfile
import unittest
import urllib.error
from pathlib import Path
from unittest.mock import patch

from verify_dependabot import is_dependabot_update, is_verified_dependabot, verify_commit

SHA = "a" * 40
BOT_EMAIL = "49699333+dependabot[bot]@users.noreply.github.com"
VERIFIED = {
    "sha": SHA,
    "author": {"id": 49699333, "login": "dependabot[bot]", "type": "Bot"},
    "committer": {"id": 19864447, "login": "web-flow"},
    "commit": {
        "author": {"email": BOT_EMAIL},
        "committer": {"email": "noreply@github.com"},
        "verification": {"verified": True, "reason": "valid"},
    },
}
EVENT = {
    "action": "synchronize",
    "after": SHA,
    "sender": {"id": 49699333, "login": "dependabot[bot]", "type": "Bot"},
    "repository": {"id": 42, "full_name": "owner/repo"},
    "pull_request": {
        "user": {"id": 49699333, "login": "dependabot[bot]", "type": "Bot"},
        "head": {"sha": SHA, "repo": {"id": 42}},
        "base": {"repo": {"id": 42}},
    },
}
SCRIPTS = Path(__file__).resolve().parent


class UpdateProvenance(unittest.TestCase):
    def test_bot_updates_pass(self):
        for action in ["opened", "synchronize"]:
            with self.subTest(action=action):
                self.assertTrue(is_dependabot_update(EVENT | {"action": action}, SHA, "owner/repo"))

    def test_untrusted_updates_fail(self):
        for path, replacement in [
            (("action",), "reopened"),
            (("after",), "b" * 40),
            (("after",), None),
            (("sender",), None),
            (("sender", "id"), 123),
            (("sender", "login"), "someone"),
            (("sender", "type"), "User"),
            (("repository",), None),
            (("repository", "id"), 43),
            (("repository", "full_name"), "attacker/repo"),
            (("pull_request", "user"), None),
            (("pull_request", "user", "id"), 123),
            (("pull_request", "user", "login"), "someone"),
            (("pull_request", "user", "type"), "User"),
            (("pull_request", "head", "repo"), None),
            (("pull_request", "head", "repo", "id"), 43),
            (("pull_request", "base", "repo", "id"), 43),
            (("pull_request", "head", "sha"), "b" * 40),
        ]:
            with self.subTest(path=path, replacement=replacement):
                event = copy.deepcopy(EVENT)
                field = event
                for key in path[:-1]:
                    field = field[key]
                field[path[-1]] = replacement
                self.assertFalse(is_dependabot_update(event, SHA, "owner/repo"))

    def test_earlier_commit_fails(self):
        self.assertFalse(is_dependabot_update(EVENT, "b" * 40, "owner/repo"))


class CommitIdentity(unittest.TestCase):
    def test_verified_bot_passes(self):
        self.assertTrue(is_verified_dependabot(VERIFIED, SHA))

    def test_spoofed_identity_fails(self):
        for path, replacement in [
            (("sha",), "b" * 40),
            (("author",), None),
            (("author", "id"), 123),
            (("author", "login"), "renovate[bot]"),
            (("author", "type"), "User"),
            (("committer",), None),
            (("committer", "id"), 123),
            (("committer", "login"), "someone"),
            (("commit", "author", "email"), "someone@example.com"),
            (("commit", "committer", "email"), "someone@example.com"),
            (("commit", "verification"), None),
            (("commit", "verification", "verified"), False),
            (("commit", "verification", "verified"), "true"),
            (("commit", "verification", "reason"), "unsigned"),
            (("commit", "verification", "reason"), "bad_email"),
        ]:
            with self.subTest(path=path, replacement=replacement):
                commit = copy.deepcopy(VERIFIED)
                field = commit
                for key in path[:-1]:
                    field = field[key]
                field[path[-1]] = replacement
                self.assertFalse(is_verified_dependabot(commit, SHA))

    def test_empty_response_fails(self):
        self.assertFalse(is_verified_dependabot({}, SHA))


class VerificationRequest(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.event_path = Path(directory.name) / "event.json"
        self.event_path.write_text(json.dumps(EVENT))
        self.environment = patch.dict(
            os.environ,
            {
                "GITHUB_REPOSITORY": "owner/repo",
                "GH_TOKEN": "test-token",
                "GITHUB_EVENT_PATH": str(self.event_path),
                "GITHUB_EVENT_NAME": "pull_request",
                "GITHUB_ACTOR_ID": "49699333",
            },
            clear=True,
        )
        self.environment.start()
        self.addCleanup(self.environment.stop)

    def test_exact_commit_requested(self):
        with patch(
            "urllib.request.urlopen", return_value=io.StringIO(json.dumps(VERIFIED))
        ) as fetch:
            self.assertTrue(verify_commit(SHA))
        request = fetch.call_args.args[0]
        self.assertEqual(request.full_url, f"https://api.github.com/repos/owner/repo/commits/{SHA}")
        self.assertEqual(request.get_header("Authorization"), "Bearer test-token")
        self.assertEqual(fetch.call_args.kwargs["timeout"], 15)

    def test_api_failure_denies(self):
        for error in [
            urllib.error.URLError("unavailable"),
            urllib.error.HTTPError("https://api.github.com", 403, "forbidden", {}, None),
            TimeoutError("timeout"),
        ]:
            with self.subTest(error=error), patch("urllib.request.urlopen", side_effect=error):
                self.assertFalse(verify_commit(SHA))

    def test_invalid_response_denies(self):
        for response in ["not json", "null", "[]", "{}", '{"commit":[]}']:
            with (
                self.subTest(response=response),
                patch("urllib.request.urlopen", return_value=io.StringIO(response)),
            ):
                self.assertFalse(verify_commit(SHA))

    def test_missing_credentials_denies(self):
        with patch.dict(os.environ, {"GH_TOKEN": ""}), patch("urllib.request.urlopen") as fetch:
            self.assertFalse(verify_commit(SHA))
            fetch.assert_not_called()

    def test_invalid_destination_denies(self):
        for repository in ["", "owner/repo/../../other", "https://example.com", "owner/repo?token"]:
            with (
                self.subTest(repository=repository),
                patch.dict(os.environ, {"GITHUB_REPOSITORY": repository}),
                patch("urllib.request.urlopen") as fetch,
            ):
                self.assertFalse(verify_commit(SHA))
                fetch.assert_not_called()

    def test_invalid_commit_denies(self):
        for sha in ["main", "../main", SHA + "?", ""]:
            with self.subTest(sha=sha), patch("urllib.request.urlopen") as fetch:
                self.assertFalse(verify_commit(sha))
                fetch.assert_not_called()

    def test_invalid_event_denies(self):
        for event in ["not json", "null", "[]", "{}", '{"sender":"bot"}']:
            self.event_path.write_text(event)
            with self.subTest(event=event), patch("urllib.request.urlopen") as fetch:
                self.assertFalse(verify_commit(SHA))
                fetch.assert_not_called()

    def test_missing_event_denies(self):
        self.event_path.unlink()
        with patch("urllib.request.urlopen") as fetch:
            self.assertFalse(verify_commit(SHA))
            fetch.assert_not_called()

    def test_initial_actor_required(self):
        for environment in [
            {"GITHUB_ACTOR_ID": ""},
            {"GITHUB_ACTOR_ID": "123", "GITHUB_TRIGGERING_ACTOR": "dependabot[bot]"},
            {"GITHUB_EVENT_NAME": "workflow_dispatch"},
        ]:
            with (
                self.subTest(environment=environment),
                patch.dict(os.environ, environment),
                patch("urllib.request.urlopen") as fetch,
            ):
                self.assertFalse(verify_commit(SHA))
                fetch.assert_not_called()

    def test_human_rerun_allowed(self):
        with (
            patch.dict(os.environ, {"GITHUB_TRIGGERING_ACTOR": "maintainer"}),
            patch("urllib.request.urlopen", return_value=io.StringIO(json.dumps(VERIFIED))),
        ):
            self.assertTrue(verify_commit(SHA))

    def test_misattributed_commit_denied(self):
        event = copy.deepcopy(EVENT)
        event["sender"] = {"id": 123, "login": "attacker", "type": "User"}
        self.event_path.write_text(json.dumps(event))
        with patch(
            "urllib.request.urlopen", return_value=io.StringIO(json.dumps(VERIFIED))
        ) as fetch:
            self.assertFalse(verify_commit(SHA))
            fetch.assert_not_called()


class DcoIntegration(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.repository = Path(directory.name)
        self.environment = os.environ | {"GH_TOKEN": "", "GITHUB_REPOSITORY": ""}
        self.git("init", "-q")
        self.git("config", "user.name", "Contributor")
        self.git("config", "user.email", "contributor@example.com")
        self.git("-c", "commit.gpgsign=false", "commit", "--allow-empty", "-qm", "base")
        self.base = self.git("rev-parse", "HEAD").strip()

    def git(self, *args):
        return subprocess.check_output(
            ["git", *args], cwd=self.repository, env=self.environment, text=True
        )

    def commit(self, message, author=None):
        args = ["-c", "commit.gpgsign=false", "commit", "--allow-empty", "-qm", message]
        if author:
            args.extend(["--author", author])
        self.git(*args)

    def check(self):
        return subprocess.run(
            ["bash", str(SCRIPTS / "check-dco.sh"), self.base, "HEAD"],
            cwd=self.repository,
            env=self.environment,
            text=True,
            capture_output=True,
            check=False,
        )

    def test_human_signoff_passes(self):
        self.commit("change\n\nSigned-off-by: Contributor <contributor@example.com>")
        self.assertEqual(self.check().returncode, 0)

    def test_unsigned_human_fails(self):
        self.commit("unsigned change")
        self.assertNotEqual(self.check().returncode, 0)

    def test_later_signoff_insufficient(self):
        self.commit("unsigned change")
        self.commit("signed change\n\nSigned-off-by: Contributor <contributor@example.com>")
        self.assertNotEqual(self.check().returncode, 0)

    def test_forged_bot_fails(self):
        self.commit(
            "dependency change\n\nSigned-off-by: dependabot[bot] <support@github.com>",
            f"dependabot[bot] <{BOT_EMAIL}>",
        )
        self.assertNotEqual(self.check().returncode, 0)

    def test_other_bot_fails(self):
        self.commit("dependency change", "renovate[bot] <renovate@example.com>")
        self.assertNotEqual(self.check().returncode, 0)

    def test_exception_is_per_commit(self):
        # Replace only the external verification call. Its authentication and
        # fail-closed behavior are covered above; this checks the shell loop.
        verifier = self.repository / "python3"
        verifier.write_text("#!/bin/sh\nexit 0\n")
        verifier.chmod(0o755)
        self.environment["PATH"] = f"{self.repository}:{os.environ['PATH']}"
        self.commit("dependency change", f"dependabot[bot] <{BOT_EMAIL}>")
        self.assertEqual(self.check().returncode, 0)
        self.commit("unsigned human change")
        self.assertNotEqual(self.check().returncode, 0)


if __name__ == "__main__":
    unittest.main()
