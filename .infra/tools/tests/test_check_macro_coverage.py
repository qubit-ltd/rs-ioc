import json
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "check-macro-coverage.sh"


class MacroCoverageBoundaryTests(unittest.TestCase):
    def run_report(self, report):
        with tempfile.TemporaryDirectory() as temporary_directory:
            report_path = Path(temporary_directory) / "coverage.json"
            report_path.write_text(json.dumps(report), encoding="utf-8")
            return subprocess.run(
                ["bash", str(SCRIPT), str(report_path)],
                capture_output=True,
                text=True,
                check=False,
            )

    def test_exact_thresholds_pass(self):
        result = self.run_report(
            {
                "data": [
                    {
                        "totals": {
                            "functions": {"covered": 90, "count": 100},
                            "lines": {"covered": 85, "count": 100},
                            "regions": {"covered": 85, "count": 100},
                        }
                    }
                ]
            }
        )

        self.assertEqual(result.returncode, 0, result.stderr)

    def test_each_metric_one_point_below_threshold_fails(self):
        thresholds = {"functions": 90, "lines": 85, "regions": 85}
        for metric, threshold in thresholds.items():
            with self.subTest(metric=metric):
                totals = {
                    name: {"covered": value, "count": 100}
                    for name, value in thresholds.items()
                }
                totals[metric]["covered"] = threshold - 1
                result = self.run_report({"data": [{"totals": totals}]})

                self.assertNotEqual(result.returncode, 0, result.stdout)

    def test_empty_denominator_fails(self):
        result = self.run_report(
            {
                "data": [
                    {
                        "totals": {
                            "functions": {"covered": 0, "count": 0},
                            "lines": {"covered": 85, "count": 100},
                            "regions": {"covered": 85, "count": 100},
                        }
                    }
                ]
            }
        )

        self.assertNotEqual(result.returncode, 0)

    def test_report_without_data_fails(self):
        result = self.run_report({})

        self.assertNotEqual(result.returncode, 0)

    def test_missing_report_fails(self):
        with tempfile.TemporaryDirectory() as temporary_directory:
            missing_report = Path(temporary_directory) / "missing.json"
            result = subprocess.run(
                ["bash", str(SCRIPT), str(missing_report)],
                capture_output=True,
                text=True,
                check=False,
            )

        self.assertNotEqual(result.returncode, 0)

    def test_malformed_json_fails(self):
        with tempfile.TemporaryDirectory() as temporary_directory:
            report_path = Path(temporary_directory) / "coverage.json"
            report_path.write_text("{invalid json", encoding="utf-8")
            result = subprocess.run(
                ["bash", str(SCRIPT), str(report_path)],
                capture_output=True,
                text=True,
                check=False,
            )

        self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
