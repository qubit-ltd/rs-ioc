import importlib.util
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "check-manual-feature-isolation.py"
SPEC = importlib.util.spec_from_file_location("check_manual_feature_isolation", SCRIPT_PATH)
CHECKER = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(CHECKER)


BASE_TREE = """\
qubit-ioc-fixture-manual v0.0.0 (/fixture)|
qubit-ioc v0.3.0 (/repo)|
thiserror v2.0.21|default,std
"""


class ParseTreeTests(unittest.TestCase):
    def test_accepts_empty_qubit_ioc_features(self):
        self.assertEqual(CHECKER.parse_tree(BASE_TREE), "")

    def test_rejects_named_qubit_ioc_features(self):
        for features in ("default,macros,config", "future_feature"):
            with self.subTest(features=features):
                tree = BASE_TREE.replace("qubit-ioc v0.3.0 (/repo)|", f"qubit-ioc v0.3.0 (/repo)|{features}")
                with self.assertRaisesRegex(ValueError, "observed row"):
                    CHECKER.parse_tree(tree)

    def test_rejects_missing_qubit_ioc_package(self):
        tree = BASE_TREE.replace("qubit-ioc v0.3.0 (/repo)|\n", "")
        with self.assertRaisesRegex(ValueError, "found 0"):
            CHECKER.parse_tree(tree)

    def test_rejects_duplicate_qubit_ioc_packages(self):
        tree = BASE_TREE + "qubit-ioc v0.3.0 (/duplicate)|\n"
        with self.assertRaisesRegex(ValueError, "found 2"):
            CHECKER.parse_tree(tree)


if __name__ == "__main__":
    unittest.main()
