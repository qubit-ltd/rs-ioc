import importlib.util
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "check-downstream-contract-docs.py"
SPEC = importlib.util.spec_from_file_location("check_downstream_contract_docs", SCRIPT)
checker = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(checker)


DEPENDENCIES = checker.DEPENDENCIES


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
        self.native.write_text(self.manifest("0.20.1") + '\n[package]\nautotests = true\n', encoding="utf-8")
        (self.native.parent / "tests").mkdir()
        (self.native.parent / "tests" / "smoke.rs").write_text("#[test] fn smoke() {}\n", encoding="utf-8")
        self.overlay.write_text(
            self.manifest("0.20.1")
            + '\n[[test]]\nname = "smoke"\npath = "../native/tests/smoke.rs"\n',
            encoding="utf-8",
        )
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

    def set_event_bus_versions(self, overlay_version, native_version):
        for path, version in ((self.overlay, overlay_version), (self.native, native_version)):
            path.write_text(self.manifest(version), encoding="utf-8")

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

    def test_native_test_target_must_be_registered_in_wrapper(self):
        (self.native.parent / "tests" / "mixed_domain_rollback_tests.rs").write_text("", encoding="utf-8")

        errors = self.run_check()

        self.assertTrue(any("mixed_domain_rollback_tests" in error and "tests/" in error for error in errors))

        self.overlay.write_text(
            self.manifest("0.20.1")
            + '\n[[test]]\nname = "smoke"\npath = "../native/tests/smoke.rs"\n'
            + '\n[[test]]\nname = "mixed_domain_rollback_tests"\n'
            + 'path = "../native/tests/mixed_domain_rollback_tests.rs"\n',
            encoding="utf-8",
        )
        self.assertEqual([], self.run_check())

    def test_discovery_rules_and_target_errors(self):
        tests_dir = self.native.parent / "tests"
        (tests_dir / "group").mkdir()
        (tests_dir / "group" / "main.rs").write_text("", encoding="utf-8")
        (tests_dir / "support").mkdir()
        (tests_dir / "support" / "helper.rs").write_text("", encoding="utf-8")
        self.overlay.write_text(
            self.manifest("0.20.1")
            + '\n[[test]]\nname = "smoke"\npath = "../native/tests/smoke.rs"\n'
            + '\n[[test]]\nname = "group"\npath = "../native/tests/group/main.rs"\n',
            encoding="utf-8",
        )
        self.assertEqual([], self.run_check())

        self.overlay.write_text(
            self.manifest("0.20.1")
            + '\n[[test]]\nname = "smoke"\npath = "../native/tests/missing.rs"\n',
            encoding="utf-8",
        )
        errors = self.run_check()
        self.assertTrue(any("source does not exist" in error for error in errors))
        self.assertTrue(any("missing native test target 'group'" in error for error in errors))

        self.overlay.write_text(
            self.manifest("0.20.1")
            + '\n[[test]]\nname = "smoke"\npath = "../native/tests/smoke.rs"\n'
            + '\n[[test]]\nname = "smoke"\npath = "../native/tests/group/main.rs"\n',
            encoding="utf-8",
        )
        self.assertTrue(any("duplicate wrapper test target name 'smoke'" in error for error in self.run_check()))

        self.overlay.write_text(
            self.manifest("0.20.1")
            + '\n[[test]]\nname = "smoke"\npath = "../native/tests/group/main.rs"\n'
            + '\n[[test]]\nname = "group"\npath = "../native/tests/group/main.rs"\n',
            encoding="utf-8",
        )
        self.assertTrue(any("source route mismatch" in error for error in self.run_check()))

        self.overlay.write_text(
            self.manifest("0.20.1")
            + '\n[[test]]\nname = "smoke"\npath = "../native/tests/smoke.rs"\n'
            + '\n[[test]]\nname = "wrapper_only"\npath = "../native/tests/smoke.rs"\n',
            encoding="utf-8",
        )
        self.assertTrue(any("extra test target 'wrapper_only'" in error for error in self.run_check()))

    def test_autotests_false_disables_discovery_but_keeps_explicit_targets(self):
        self.native.write_text(self.manifest("0.20.1") + '\n[package]\nautotests = false\n', encoding="utf-8")
        self.overlay.write_text(self.manifest("0.20.1"), encoding="utf-8")
        self.assertEqual([], self.run_check())

        self.native.write_text(
            self.manifest("0.20.1")
            + '\n[package]\nautotests = false\n'
            + '\n[[test]]\nname = "smoke"\npath = "tests/smoke.rs"\n',
            encoding="utf-8",
        )
        self.overlay.write_text(
            self.manifest("0.20.1")
            + '\n[[test]]\nname = "smoke"\npath = "../native/tests/smoke.rs"\n',
            encoding="utf-8",
        )
        self.assertEqual([], self.run_check())

    def test_explicit_targets_without_paths_use_cargo_inferred_test_paths(self):
        (self.overlay.parent / "tests").mkdir()
        (self.overlay.parent / "tests" / "smoke.rs").symlink_to(self.native.parent / "tests" / "smoke.rs")
        self.native.write_text(
            self.manifest("0.20.1")
            + '\n[package]\nautotests = false\n'
            + '\n[[test]]\nname = "smoke"\n',
            encoding="utf-8",
        )
        self.overlay.write_text(
            self.manifest("0.20.1")
            + '\n[package]\nautotests = false\n'
            + '\n[[test]]\nname = "smoke"\n',
            encoding="utf-8",
        )

        self.assertEqual([], self.run_check())

        (self.overlay.parent / "tests" / "smoke.rs").unlink()
        errors = self.run_check()
        self.assertTrue(any("wrapper test target 'smoke' source does not exist" in error for error in errors))

    def test_mismatched_event_bus_version_reports_both_values(self):
        self.native.write_text(self.manifest("0.20.0"), encoding="utf-8")

        errors = self.run_check()

        self.assertTrue(any("qubit-event-bus" in error for error in errors))
        self.assertTrue(any("0.20.1" in error and "0.20.0" in error for error in errors))

    def test_equivalent_version_requirements_pass_through_run_check(self):
        equivalent_pairs = (
            ("0.21", "^0.21"),
            ("0.4.0", "^0.4"),
            ("1", "^1.0.0"),
            ("0.0", "^0.0"),
        )
        for overlay_version, native_version in equivalent_pairs:
            with self.subTest(overlay=overlay_version, native=native_version):
                self.set_event_bus_versions(overlay_version, native_version)
                self.assertFalse(
                    any("qubit-event-bus version mismatch" in error for error in self.run_check())
                )

    def test_different_version_ranges_fail_through_run_check(self):
        different_pairs = (
            ("0", "0.0"),
            ("0.0", "0.0.0"),
            ("0.0.3", "0.0.4"),
            ("0.4", "^0.4.1"),
            ("0.20", "0.21"),
        )
        for overlay_version, native_version in different_pairs:
            with self.subTest(overlay=overlay_version, native=native_version):
                self.set_event_bus_versions(overlay_version, native_version)
                errors = self.run_check()
                self.assertTrue(any("qubit-event-bus version mismatch" in error for error in errors))

    def test_complex_requirement_must_match_exactly(self):
        requirement = ">=0.4, <0.5"
        self.set_event_bus_versions(requirement, requirement)
        self.assertFalse(any("qubit-event-bus version mismatch" in error for error in self.run_check()))
        self.set_event_bus_versions(requirement, ">=0.4.0, <0.5")
        self.assertTrue(any("qubit-event-bus version mismatch" in error for error in self.run_check()))

    def test_unsupported_requirement_forms_are_only_equal_when_identical(self):
        rejected_pairs = (
            ("~0.4", "^0.4"),
            ("0.4.0-alpha", "0.4.0"),
            ("0.4.*", "0.4"),
            ("0.4", "0.4.0+build"),
            ("01.2", "1.2"),
            ("0.4 trailing", "0.4"),
        )
        for overlay_version, native_version in rejected_pairs:
            with self.subTest(overlay=overlay_version, native=native_version):
                self.set_event_bus_versions(overlay_version, native_version)
                errors = self.run_check()
                self.assertTrue(any("qubit-event-bus version mismatch" in error for error in errors))

        for requirement in ("~0.4", "0.4.0-alpha", "0.4.*", "0.4.0+build", "01.2"):
            with self.subTest(requirement=requirement):
                self.set_event_bus_versions(requirement, requirement)
                self.assertFalse(
                    any("qubit-event-bus version mismatch" in error for error in self.run_check())
                )

    def test_quoted_direct_package_versions_are_rejected(self):
        for index, dependency in enumerate(DEPENDENCIES):
            with self.subTest(dependency=dependency):
                if index % 3 == 0:
                    statement = f"{dependency} = \"1.2.3\""
                elif index % 3 == 1:
                    statement = f"`{dependency}` version: '1.2.3'"
                else:
                    statement = f"`{dependency}` version\n= \"1.2.3\""
                self.source.write_text(
                    f"{statement}\n[workflow](.github/workflows/downstream-contracts.yml) "
                    "[manifest](overlay/Cargo.toml)\n",
                    encoding="utf-8",
                )

                errors = self.run_check()

                self.assertTrue(any("direct version statement" in error for error in errors))

    def test_links_to_sources_without_direct_versions_pass(self):
        self.source.write_text(
            "Read the [workflow](.github/workflows/downstream-contracts.yml) and "
            "[overlay manifest](overlay/Cargo.toml) for current source selection.\n",
            encoding="utf-8",
        )

        self.assertEqual([], self.run_check())

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
