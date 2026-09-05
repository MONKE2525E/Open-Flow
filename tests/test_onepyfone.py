import importlib.util
import json
import sys
import tempfile
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path


RUNNER_PATH = Path(__file__).with_name("OnePyFone.py")
SPEC = importlib.util.spec_from_file_location("onepyfone", RUNNER_PATH)
assert SPEC and SPEC.loader
runner = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = runner
SPEC.loader.exec_module(runner)


class RunnerTests(unittest.TestCase):
    def test_protocol_payload_is_removed_from_human_output(self):
        raw = 'before\ntest name ... VERENU_TEST_RESULT={"status":"failed","observed":"broken"}\nafter'
        output, payload = runner._parse_protocol(raw)
        self.assertEqual(output, "before\ntest name ...\nafter")
        self.assertEqual(payload["status"], "failed")

    def test_filter_matches_stable_id(self):
        selected = runner.select_tests(["accessibility"], "settings-focus")
        self.assertEqual([test.id for test in selected], ["accessibility.settings-focus"])

    def test_unknown_suite_is_rejected(self):
        args = type("Args", (), {"suite": "made-up", "profile": "fast"})()
        with self.assertRaisesRegex(ValueError, "Unknown suite"):
            runner.parse_suites(args)

    def test_json_report_contains_agent_fields(self):
        selected = runner.select_tests(["preflight"], "environment")
        result = runner.TestResult(
            "failed",
            expected="tools exist",
            observed="cargo missing",
            regression_area="environment",
            failure_kind="infrastructure",
            measurements={"cargo": False},
        )
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "result.json"
            runner.write_json_report(path, "fast", ["preflight"], selected, {selected[0].id: result}, 0.1)
            payload = json.loads(path.read_text(encoding="utf-8"))
        self.assertEqual(payload["schema_version"], 2)
        self.assertEqual(payload["required_failures"], ["preflight.environment"])
        test = payload["tests"][0]
        self.assertEqual(test["failure_kind"], "infrastructure")
        self.assertEqual(test["observed"], "cargo missing")
        self.assertEqual(test["measurements"], {"cargo": False})

    def test_junit_escapes_failure_attributes(self):
        selected = runner.select_tests(["preflight"], "environment")
        result = runner.TestResult("failed", observed='expected "quoted" value', output="bad <value>")
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "result.xml"
            runner.write_junit_report(path, selected, {selected[0].id: result})
            root = ET.parse(path).getroot()
        self.assertEqual(root.find("testcase/failure").attrib["message"], 'expected "quoted" value')

    def test_loop_merge_keeps_failure_and_marks_flake(self):
        merged = runner.merge_loop_results(
            {"sample": runner.TestResult("passed", duration_s=0.1)},
            {"sample": runner.TestResult("failed", observed="broke", duration_s=0.2)},
        )
        self.assertEqual(merged["sample"].status, "failed")
        self.assertEqual(merged["sample"].regression_status, "flaky")
        self.assertAlmostEqual(merged["sample"].duration_s, 0.3)


if __name__ == "__main__":
    unittest.main()

