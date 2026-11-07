# SPDX-License-Identifier: BSD-2-Clause

import unittest

from check_pr_title import title_errors


class PullRequestTitle(unittest.TestCase):
    def test_valid_titles(self):
        for title in [
            "refactor: clean up engine naming, documentation and tooling",
            "perf(engine): cap tile buckets at 4 per record above 8 bytes",
            "feat!: rename Sorter::sort_by_key to sort_by",
            "build(deps): bump serde from 1.0.0 to 1.0.1",
            "docs(site): explain the wide-record cursor cap",
        ]:
            with self.subTest(title=title):
                self.assertEqual(title_errors(title), [])

    def test_invalid_titles(self):
        for title, error in [
            ("Clean up engine naming", "expected"),
            ("refactor:clean up", "expected"),
            ("Refactor: clean up", "expected"),
            ("refactor(): clean up", "expected"),
            ("feature: add a sort", "type `feature`"),
            ("fix(lib): keep records", "scope `lib`"),
            ("fix: keep records.", "period"),
            ("docs: " + "x" * 60, "over 64"),
        ]:
            with self.subTest(title=title):
                errors = title_errors(title)
                self.assertTrue(any(error in e for e in errors), errors)


if __name__ == "__main__":
    unittest.main()
