import importlib.util
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "check-doc-versions.py"
checker = None
if SCRIPT.is_file():
    spec = importlib.util.spec_from_file_location("check_doc_versions", SCRIPT)
    checker = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(checker)


DOCS = {
    "macros/README.md": """This checkout pairs qubit-ioc 0.4.0 with qubit-ioc-macros 0.3.0. Applications should use the runtime facade, which selects its macro dependency; the two packages do not need identical version numbers.\n\n[dependencies]\nqubit-ioc = { version = \"0.4\", features = [\"macros\"] }\n\nThis checkout pairs qubit-ioc 0.4.0 with qubit-ioc-macros 0.3.0. Applications should use the runtime facade, which selects its macro dependency; the two packages do not need identical version numbers.\n""",
    "macros/README.zh_CN.md": """本 checkout 使用 qubit-ioc 0.4.0 与 qubit-ioc-macros 0.3.0。应用应通过运行时 facade 使用宏，由其依赖声明选择宏包；两个包不要求版本号相同。\n\n[dependencies]\nqubit-ioc = { version = \"0.4\", features = [\"macros\"] }\n\n本 checkout 使用 qubit-ioc 0.4.0 与 qubit-ioc-macros 0.3.0。应用应通过运行时 facade 使用宏，由其依赖声明选择宏包；两个包不要求版本号相同。\n""",
    "doc/user_guide.md": "This guide describes qubit-ioc 0.4.0.\n",
    "doc/user_guide.zh_CN.md": "本手册面向使用 qubit-ioc 0.4.0 的 Rust 应用。\n",
    "doc/complete-design.md": "This document describes qubit-ioc 0.4.0.\n",
    "doc/complete-design.zh_CN.md": "本文描述 qubit-ioc 0.4.0 当前实现。\n",
    "doc/lifecycle.md": "This guide describes qubit-ioc 0.4.0. Historical migration target: runtime and macro packages both use 0.3.0. Current checkout uses runtime 0.4.0 with macros 0.3.0.\n\n## Migration from 0.2 to 0.3\n",
    "doc/lifecycle.zh_CN.md": "本文适用于 qubit-ioc 0.4.0。当年的迁移目标要求运行时和宏都使用 0.3.0。当前 checkout 使用 runtime 0.4.0 与 macros 0.3.0。\n\n## 从 0.2 迁移到 0.3\n",
    "README.md": 'Install with qubit-ioc = "0.4".\n',
    "README.zh_CN.md": '安装 qubit-ioc = "0.4"。\n',
    "CHANGELOG.md": "# Changelog\n\n## 0.4.0\n\nCurrent compatibility record.\n\n## 0.3.0\n\nHistorical changes.\n",
}


class CheckDocVersionsTests(unittest.TestCase):
    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = Path(self.temp_dir.name)
        (self.root / "macros").mkdir()
        (self.root / "doc").mkdir()
        (self.root / "Cargo.toml").write_text(
            '[package]\nname = "qubit-ioc"\nversion = "0.4.0"\n'
            '[dependencies]\nqubit-ioc-macros = { path = "macros", version = "0.3.0", optional = true }\n',
            encoding="utf-8",
        )
        (self.root / "macros/Cargo.toml").write_text(
            '[package]\nname = "qubit-ioc-macros"\nversion = "0.3.0"\n',
            encoding="utf-8",
        )
        for name, content in DOCS.items():
            (self.root / name).write_text(content, encoding="utf-8")

    def tearDown(self):
        self.temp_dir.cleanup()

    def run_check(self):
        self.assertIsNotNone(checker, "check-doc-versions.py implementation is missing")
        return checker.check(self.root)

    def write(self, name, content):
        (self.root / name).write_text(content, encoding="utf-8")

    def test_matching_current_versions_and_historical_text_pass(self):
        self.write("doc/lifecycle.md", DOCS["doc/lifecycle.md"] + "Example from 0.2.0 and migration to 0.3.0.\n")
        self.assertEqual([], self.run_check())

    def test_lifecycle_migration_headings_are_required(self):
        for name, heading in (
            ("doc/lifecycle.md", "## Migration from 0.2 to 0.3"),
            ("doc/lifecycle.zh_CN.md", "## 从 0.2 迁移到 0.3"),
        ):
            with self.subTest(name=name):
                original = DOCS[name]
                self.write(name, original.replace(heading, "## Renamed historical section", 1))
                errors = self.run_check()
                self.assertTrue(any(name in error and "migration section" in error for error in errors))
                self.write(name, original.replace(heading + "\n", "", 1))
                errors = self.run_check()
                self.assertTrue(any(name in error and "migration section" in error for error in errors))
                self.write(name, original)

    def test_stale_chinese_current_version_is_reported(self):
        self.write("doc/user_guide.zh_CN.md", "本手册适用于 qubit-ioc 0.3.0。\n")
        self.assertTrue(any("user_guide.zh_CN.md" in error for error in self.run_check()))

    def test_changed_runtime_and_macro_manifest_versions_must_match_docs(self):
        self.write("Cargo.toml", '[package]\nname = "qubit-ioc"\nversion = "0.5.0"\n[dependencies]\nqubit-ioc-macros = { path = "macros", version = "0.4.0" }\n')
        self.write("macros/Cargo.toml", '[package]\nname = "qubit-ioc-macros"\nversion = "0.4.0"\n')
        errors = self.run_check()
        self.assertTrue(any("0.5.0" in error for error in errors))
        self.assertTrue(any("0.4.0" in error for error in errors))

    def test_installation_minor_drift_is_reported(self):
        self.write("macros/README.md", DOCS["macros/README.md"].replace('version = "0.4"', 'version = "0.3"', 1))
        self.assertTrue(any("macros/README.md" in error for error in self.run_check()))

    def test_root_readme_minor_drift_is_reported(self):
        self.write("README.zh_CN.md", '安装 qubit-ioc = "0.3"。\n')
        self.assertTrue(any("README.zh_CN.md" in error for error in self.run_check()))

    def test_runtime_macro_dependency_mismatch_is_reported(self):
        self.write("Cargo.toml", '[package]\nname = "qubit-ioc"\nversion = "0.4.0"\n[dependencies]\nqubit-ioc-macros = { path = "macros", version = "0.2.0" }\n')
        self.assertTrue(any("dependency" in error.lower() or "macro" in error.lower() for error in self.run_check()))

    def test_missing_document_and_declaration_are_reported(self):
        (self.root / "doc/user_guide.md").unlink()
        self.write("doc/complete-design.md", "No current version declaration.\n")
        errors = self.run_check()
        self.assertTrue(any("user_guide.md" in error for error in errors))
        self.assertTrue(any("complete-design.md" in error for error in errors))

    def test_invalid_toml_is_reported(self):
        self.write("macros/Cargo.toml", "[package\nversion = ???\n")
        self.assertTrue(any("TOML" in error for error in self.run_check()))

    def test_missing_manifest_is_reported(self):
        (self.root / "Cargo.toml").unlink()
        self.assertTrue(any("Cargo.toml" in error for error in self.run_check()))


if __name__ == "__main__":
    unittest.main()
