# SPDX-License-Identifier: BSD-2-Clause

import re
import unittest
from pathlib import Path

from ci_changes import select

ROOT = Path(__file__).resolve().parents[2]


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
        for path in [
            "conformance/semantics.md",
            "conformance/vectors/a.json",
            "bench/report/report.py",
            "LICENSE",
            ".github/workflows/release.yml",
            ".github/workflows/rust-ci.yml",
        ]:
            with self.subTest(path=path):
                self.assertEqual(select([path]), {"rust": True, "cpp": True, "python": True})

    def test_docs_site_tooling_selects_no_language(self):
        for path in [
            "docs/.vitepress/config.ts",
            "docs/.vitepress/theme/sim/digits.ts",
            "docs/public/logo.svg",
            "docs/development/[workflow].paths.ts",
            "package.json",
            "pnpm-lock.yaml",
            ".github/workflows/docs.yml",
        ]:
            with self.subTest(path=path):
                self.assertEqual(select([path]), {"rust": False, "cpp": False, "python": False})

    def test_prose_selects_no_language(self):
        for path in [
            "AGENTS.md",
            "README.md",
            "RELEASING.md",
            "ROADMAP.md",
            "docs/explain/keys.md",
            ".agents/skills/tessera-engine/SKILL.md",
            ".agents/skills/tessera-measure/SKILL.md",
            ".agents/skills/tessera-pr-performance/SKILL.md",
            "bench/README.md",
            "assets/logo.svg",
        ]:
            with self.subTest(path=path):
                self.assertEqual(select([path]), {"rust": False, "cpp": False, "python": False})
        # Markdown inside a language directory belongs to that language.
        self.assertEqual(
            select(["rust/tessera-sort/README.md"]), {"rust": True, "cpp": False, "python": True}
        )

    def test_ci_scripts_select_no_language(self):
        for path in [
            ".github/scripts/ci_changes.py",
            ".github/scripts/check-dco.sh",
            "biome.json",
            "ruff.toml",
        ]:
            with self.subTest(path=path):
                self.assertEqual(select([path]), {"rust": False, "cpp": False, "python": False})

    def test_rust_sources_include_only_rust_paths(self):
        # Skipping prose is sound only while no Rust build reads it, for example
        # through `#![doc = include_str!("../../README.md")]`.
        pattern = re.compile(r'include_(?:str|bytes)!\(\s*"([^"]+)"')
        for source in ROOT.glob("**/*.rs"):
            if "node_modules" in source.parts or "target" in source.parts:
                continue
            for target in pattern.findall(source.read_text()):
                path = (source.parent / target).resolve().relative_to(ROOT).as_posix()
                with self.subTest(source=source.relative_to(ROOT).as_posix(), target=path):
                    self.assertTrue(select([path])["rust"])

    def test_changes_accumulate_across_languages(self):
        self.assertEqual(
            select(["rust/a.rs", "cpp/b.hpp"]), {"rust": True, "cpp": True, "python": True}
        )


if __name__ == "__main__":
    unittest.main()
