# SPDX-License-Identifier: BSD-2-Clause

import unittest
from ci_changes import select


class LanguageSelection(unittest.TestCase):
    def test_language_sources_and_manifests_are_independent(self):
        for paths, expected in [
            (["rust/tessera-sort/src/lib.rs"], {"rust": True, "cpp": False, "python": True}),
            (["Cargo.lock"], {"rust": True, "cpp": False, "python": True}),
            (["rustfmt.toml"], {"rust": True, "cpp": False, "python": True}),
            (["bench/rust/main.rs"], {"rust": True, "cpp": False, "python": True}),
            (["cpp/include/sort.hpp"], {"rust": False, "cpp": True, "python": False}),
            (["CMakeLists.txt"], {"rust": False, "cpp": True, "python": False}),
            (["python/src/lib.rs"], {"rust": False, "cpp": False, "python": True}),
            ([], {"rust": False, "cpp": False, "python": False}),
        ]:
            with self.subTest(paths=paths):
                self.assertEqual(select(paths), expected)

    def test_shared_changes_validate_both(self):
        for path in ["conformance/semantics.md", "conformance/vectors/a.json",
                     "docs/design.md", "bench/report/report.py", "LICENSE",
                     ".github/workflows/release.yml"]:
            with self.subTest(path=path):
                self.assertEqual(select([path]), {"rust": True, "cpp": True, "python": True})

    def test_docs_site_tooling_selects_no_language(self):
        for path in ["docs/.vitepress/config.ts", "docs/.vitepress/theme/sim/digits.ts",
                     "docs/public/logo.svg", "package.json", "package-lock.json",
                     ".github/workflows/docs.yml"]:
            with self.subTest(path=path):
                self.assertEqual(select([path]), {"rust": False, "cpp": False, "python": False})
        # Pages remain shared documentation.
        self.assertEqual(select(["docs/explain/keys.md"]), {"rust": True, "cpp": True, "python": True})

    def test_changes_accumulate_across_languages(self):
        self.assertEqual(select(["rust/a.rs", "cpp/b.hpp"]),
                         {"rust": True, "cpp": True, "python": True})


if __name__ == "__main__":
    unittest.main()
