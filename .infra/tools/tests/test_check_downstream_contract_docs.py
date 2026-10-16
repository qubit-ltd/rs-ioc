import importlib.util
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "check-downstream-contract-docs.py"
SPEC = importlib.util.spec_from_file_location("check_downstream_contract_docs", SCRIPT)
checker = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(checker)


DEPENDENCIES = (
    "qubit-ioc",
    "qubit-event-bus",
    "qubit-execution-services",
    "qubit-fs-registry",
    "qubit-fs-local",
    "qubit-fs",
)


class CheckDownstreamContractDocsTests(unittest.TestCase):
    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = Path(self.temp_dir.name)
        self.source = self.root / "SOURCE.md"
        self.overlay = self.root / "overlay" / "Cargo.toml"
        self.native = self.root / "native" / "Cargo.toml"
        self.workflow = self.root / ".github" / "workflows" / "downstream-contracts.yml"
        self.workflow.parent.mkdir(parents=True)
        self.workflow.write_text("name: downstream contracts\n", encoding="utf-8")
        self.overlay.parent.mkdir(parents=True)
        self.native.parent.mkdir(parents=True)
        self.overlay.write_text(self.manifest("0.20.1"), encoding="utf-8")
        self.native.write_text(self.manifest("0.20.1"), encoding="utf-8")
        self.source.write_text(
            "See [workflow](.github/workflows/downstream-contracts.yml) and "
            "[manifest](overlay/Cargo.toml).\n",
            encoding="utf-8",
        )

    def tearDown(self):
        self.temp_dir.cleanup()

    @staticmethod
    def manifest(event_bus_version):
        entries = []
        for dependency in DEPENDENCIES:
            version = event_bus_version if dependency == "qubit-event-bus" else "1.2.3"
            entries.append(f'[dependencies.{dependency}]\nversion = "{version}"\n')
        return "[dependencies]\n" + "\n".join(entries)

    def run_check(self):
        return checker.check(self.root, self.source, self.overlay, self.native)

    def test_normal_checkout_finds_sibling_manifest(self):
        checkout_parent = self.root / "checkout-parent"
        root = checkout_parent / "rs-ioc"
        sibling_manifest = checkout_parent / "rs-execution-services" / "tests" / "fixtures" / "ioc_application_consumer" / "Cargo.toml"
        sibling_manifest.parent.mkdir(parents=True)
        sibling_manifest.write_text("[package]\nname = \"consumer\"\n", encoding="utf-8")

        self.assertEqual(sibling_manifest, checker.default_native_path(root))

    def test_empty_markdown_link_is_ignored_and_required_links_are_reported(self):
        self.source.write_text("Empty link: [x](   )\n", encoding="utf-8")

        errors = self.run_check()

        self.assertTrue(any("missing relative Markdown link to current workflow" in error for error in errors))
        self.assertTrue(any("missing relative Markdown link to overlay manifest" in error for error in errors))

    def test_absolute_workflow_link_does_not_satisfy_relative_link_requirement(self):
        self.source.write_text(
            f"See [workflow]({self.workflow}) and [manifest](overlay/Cargo.toml).\n",
            encoding="utf-8",
        )

        errors = self.run_check()

        self.assertTrue(any("missing relative Markdown link to current workflow" in error for error in errors))

    def test_linked_worktree_finds_existing_sibling_manifest(self):
        checkout_root = self.root / "checkout"
        linked_root = checkout_root / ".worktrees" / "rs-ioc" / "branch"
        sibling_manifest = checkout_root / "rs-execution-services" / "tests" / "fixtures" / "ioc_application_consumer" / "Cargo.toml"
        sibling_manifest.parent.mkdir(parents=True)
        sibling_manifest.write_text("[package]\nname = \"consumer\"\n", encoding="utf-8")

        self.assertEqual(sibling_manifest, checker.default_native_path(linked_root))

    def test_matching_manifests_and_relative_links_pass(self):
        self.assertEqual([], self.run_check())

    def test_mismatched_event_bus_version_reports_both_values(self):
        self.native.write_text(self.manifest("0.20.0"), encoding="utf-8")

        errors = self.run_check()

        self.assertTrue(any("qubit-event-bus" in error for error in errors))
        self.assertTrue(any("0.20.1" in error and "0.20.0" in error for error in errors))

    def test_quoted_direct_package_versions_are_rejected(self):
        for statement in ('qubit-ioc = \"0.3.0\"', "qubit-event-bus version: '0.20.1'"):
            with self.subTest(statement=statement):
                self.source.write_text(
                    f"{statement}\n[workflow](.github/workflows/downstream-contracts.yml) "
                    "[manifest](overlay/Cargo.toml)\n",
                    encoding="utf-8",
                )

                errors = self.run_check()

                self.assertTrue(any(statement.split()[0] in error for error in errors))

    def test_full_sha_in_source_is_rejected(self):
        self.source.write_text("Pinned commit: " + "a" * 40 + "\n", encoding="utf-8")

        errors = self.run_check()

        self.assertTrue(any("40" in error or "SHA" in error or "sha" in error for error in errors))

    def test_missing_workflow_link_target_is_reported(self):
        self.workflow.unlink()

        errors = self.run_check()

        self.assertTrue(any("downstream-contracts.yml" in error for error in errors))


if __name__ == "__main__":
    unittest.main()
