#!/usr/bin/env python3
"""Verenu's unified regression runner.

The runner intentionally uses only Python's standard library. It keeps the
existing single-script entry point while providing profiles, stable test IDs,
timeouts, parallel browser execution, and human/JSON/JUnit reports.

Examples:
    python tests/OnePyFone.py
    python tests/OnePyFone.py --suite accessibility,performance
    python tests/OnePyFone.py --test ui.settings-focus
    python tests/OnePyFone.py --profile live --verbose
    python tests/OnePyFone.py --list
"""

from __future__ import annotations

import argparse
import atexit
import concurrent.futures
import json
import os
import re
import signal
import shutil
import socket
import subprocess
import sys
import threading
import time
import tempfile
import urllib.request
from dataclasses import asdict, dataclass, field, replace
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, Iterable, List, Optional, Sequence
from xml.sax.saxutils import escape as xml_escape


if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding="utf-8")
        sys.stderr.reconfigure(encoding="utf-8")
    except Exception:
        pass


ROOT = Path(__file__).parent.parent.resolve()
TESTS_DIR = Path(__file__).parent.resolve()
SMOKE_DIR = TESTS_DIR / "smoke"
INTEGRATION_DIR = TESTS_DIR / "integration"
AUDIO_WAV = SMOKE_DIR / "smoke_test.wav"
CARGO_TOML = ROOT / "src-tauri" / "Cargo.toml"
DEFAULT_JSON_REPORT = ROOT / "test-results" / "onepyfone.json"
NPM = "npm.cmd" if sys.platform == "win32" else "npm"
PORT = 1420
RESULT_PREFIX = "VERENU_TEST_RESULT="
_OUTPUT_LOCK = threading.Lock()

SUITE_ORDER = [
    "preflight",
    "unit",
    "frontend",
    "rust",
    "contract",
    "ui",
    "accessibility",
    "state",
    "performance",
    "animation",
    "pipeline",
    "native",
]

PROFILE_SUITES = {
    "fast": [
        "preflight",
        "unit",
        "frontend",
        "rust",
        "contract",
        "ui",
        "accessibility",
        "state",
        "performance",
    ],
    "live": ["preflight", "pipeline"],
    "native": ["preflight", "native"],
    "full": SUITE_ORDER,
}

# These tests create isolated browser contexts and do not mutate shared files.
PARALLEL_SAFE_SUITES = {"ui", "accessibility", "state", "animation"}


@dataclass
class TestResult:
    status: str
    output: str = ""
    duration_s: float = 0.0
    attempts: int = 1
    expected: str = ""
    observed: str = ""
    measurements: Dict[str, Any] = field(default_factory=dict)
    baseline: Dict[str, Any] = field(default_factory=dict)
    regression_area: str = ""
    failure_kind: Optional[str] = None
    regression_status: str = "unknown"
    skip_reason: Optional[str] = None

    @property
    def passed(self) -> bool:
        return self.status == "passed"

    @property
    def skipped(self) -> bool:
        return self.status == "skipped"


@dataclass
class TestEntry:
    id: str
    suite: str
    name: str
    category: str
    expected: str
    regression_area: str
    script: Optional[str] = None
    command: Optional[List[str]] = None
    python_test: Optional["PythonTest"] = None
    timeout_s: int = 60
    retries: int = 0
    needs_server: bool = False
    required: bool = True
    failure_kind: str = "product"


def entry(
    test_id: str,
    suite: str,
    name: str,
    *,
    script: Optional[str] = None,
    command: Optional[List[str]] = None,
    timeout_s: int = 60,
    retries: int = 0,
    needs_server: bool = False,
    required: bool = True,
    category: Optional[str] = None,
    expected: str = "Test contract holds",
    regression_area: Optional[str] = None,
    failure_kind: str = "product",
) -> TestEntry:
    return TestEntry(
        id=test_id,
        suite=suite,
        name=name,
        category=category or suite,
        expected=expected,
        regression_area=regression_area or suite,
        script=script,
        command=command,
        timeout_s=timeout_s,
        retries=retries,
        needs_server=needs_server,
        required=required,
        failure_kind=failure_kind,
    )


JS_TESTS: List[TestEntry] = [
    entry("ui.app-mount", "ui", "App mount and DOM structure", script="smoke/test.cjs", timeout_s=45, retries=1, needs_server=True, expected="The app mounts with visible primary navigation", regression_area="application startup"),
    entry("ui.onboarding-layout", "ui", "Onboarding layout contracts", script="integration/playwright-test-onboarding-layout-dev.cjs", timeout_s=120, needs_server=True, expected="Onboarding remains usable at supported window sizes", regression_area="onboarding layout"),
    entry("ui.navigation", "ui", "Navigation and interaction", script="smoke/playwright-test-ui.cjs", timeout_s=30, needs_server=True, expected="Primary navigation and settings interactions work", regression_area="navigation and settings"),
    entry("ui.element-contracts", "ui", "Element contract assertions", script="smoke/playwright-test-fixes.cjs", timeout_s=15, needs_server=True, expected="Stable UI selectors and labels remain present", regression_area="shared UI contracts"),
    entry("ui.app-mappings", "ui", "App mappings flow", script="smoke/test-app.cjs", timeout_s=90, retries=1, needs_server=True, expected="A mapping can be created through the UI", regression_area="app mappings"),
    entry("ui.dictionary", "ui", "Dictionary interaction", script="smoke/test-dictionary-ui.cjs", timeout_s=90, retries=1, needs_server=True, expected="Dictionary entries can be created and inspected", regression_area="dictionary"),
    entry("ui.onboarding-persistence", "ui", "Onboarding flow persistence", script="integration/playwright-test-onboarding-dev.cjs", timeout_s=120, needs_server=True, expected="Onboarding choices survive navigation and reload", regression_area="onboarding state"),
    entry("ui.content-surfaces", "ui", "Snippets, dictionary, and style surfaces", script="integration/playwright-test-surface-dev.cjs", timeout_s=120, needs_server=True, expected="Core content surfaces render and accept input", regression_area="cross-feature content"),
    entry("ui.offline-state", "ui", "Offline failure state", script="integration/playwright-test-offline-dev.cjs", timeout_s=60, needs_server=True, expected="Offline status produces a visible, useful error", regression_area="connectivity error handling"),
    entry("ui.fullscreen-settings", "ui", "Full-screen settings behavior", script="integration/playwright-test-fullscreen-settings-dev.cjs", timeout_s=120, retries=1, needs_server=True, expected="Settings rail, transitions, and footer contracts hold", regression_area="settings shell"),
    entry("ui.history-filter", "ui", "History search and app filter", script="integration/playwright-test-history-filter-dev.cjs", timeout_s=90, needs_server=True, expected="History filters combine without losing row metadata", regression_area="history"),
    entry("ui.local-models", "ui", "Local cleanup and transcription model states", script="integration/playwright-test-local-cleanup-models-dev.cjs", timeout_s=120, needs_server=True, expected="Downloaded local models appear in the correct task and picker states", regression_area="local model configuration"),
    entry("ui.style-merge", "ui", "Style and Legacy layout interaction", script="integration/playwright-test-style-merge-dev.cjs", timeout_s=120, needs_server=True, expected="Style controls merge by default and retain the Legacy tabbed behavior", regression_area="style and legacy cross-feature behavior"),
    entry("ui.resilience", "ui", "Malformed state and recovery flows", script="integration/playwright-test-resilience-dev.cjs", timeout_s=90, needs_server=True, expected="Malformed optional state and failed commands do not crash the app", regression_area="error recovery and unusual state"),
    entry("ui.macos-permissions", "ui", "macOS permissions gates and repair states", script="integration/playwright-test-macos-permissions-dev.cjs", timeout_s=45, needs_server=True, expected="macOS onboarding and Settings enforce and explain required permission states", regression_area="macOS permissions UX"),
    entry("accessibility.semantic-audit", "accessibility", "Semantic accessibility audit", script="integration/playwright-test-accessibility-dev.cjs", timeout_s=120, needs_server=True, expected="Visible controls have names, valid semantics, and keyboard access", regression_area="accessibility semantics"),
    entry("accessibility.settings-focus", "accessibility", "Settings and modal focus flow", script="integration/playwright-test-focus-dev.cjs", timeout_s=120, needs_server=True, expected="Keyboard focus enters, stays within, and returns from dialogs", regression_area="keyboard and focus management"),
    entry("state.settings", "state", "Settings persistence", script="smoke/playwright-test-state.cjs", timeout_s=90, retries=1, needs_server=True, expected="Model and advanced settings survive close and reopen", regression_area="settings persistence"),
    entry("state.appearance", "state", "Appearance persistence", script="smoke/playwright-test-appearance.cjs", timeout_s=90, retries=1, needs_server=True, expected="Appearance selection persists", regression_area="appearance state"),
    entry("state.developer-mode", "state", "Developer mode unlock", script="smoke/playwright-test-devmode.cjs", timeout_s=60, needs_server=True, expected="Developer mode unlock remains deliberate and persistent", regression_area="developer settings"),
    entry("state.cross-feature", "state", "Legacy and Contexts cross-feature state", script="integration/playwright-test-cross-feature-state-dev.cjs", timeout_s=120, needs_server=True, expected="Legacy navigation and Contexts never conflict after persistence changes", regression_area="cross-feature configuration"),
    entry("performance.browser-startup", "performance", "Browser startup and interaction budgets", script="integration/playwright-test-performance-dev.cjs", timeout_s=120, needs_server=True, expected="Warm startup and common interactions stay within checked-in budgets", regression_area="startup and UI performance"),
    entry("animation.dropdowns", "animation", "Dropdown width animations", script="smoke/test-all-dropdown-animations.cjs", timeout_s=45, needs_server=True, expected="Dropdown widths animate without collapse or overflow", regression_area="control animation"),
    entry("animation.mic-full", "animation", "Microphone dropdown animation", script="smoke/test-animation-full.cjs", timeout_s=45, needs_server=True, expected="Microphone dropdown animation remains smooth and bounded", regression_area="microphone control animation"),
    entry("animation.mic-smoke", "animation", "Microphone dropdown smoke", script="smoke/test-mic-dropdown-animation.cjs", timeout_s=45, needs_server=True, expected="Microphone dropdown opens and settles correctly", regression_area="microphone control animation"),
    entry("pipeline.audio-fixture", "pipeline", "Audio fixture integrity", expected="Optional live audio fixture is a valid non-trivial WAV", regression_area="live test infrastructure", required=False, failure_kind="infrastructure"),
    entry("pipeline.provider-smoke", "pipeline", "Configured provider transcription", command=["cargo", "test", "--manifest-path", str(CARGO_TOML), "live_transcription_regression", "--", "--ignored", "--nocapture", "--test-threads=1"], timeout_s=240, required=False, expected="The configured transcription provider transcribes the optional known WAV fixture", regression_area="provider transcription pipeline"),
]


class PythonTest:
    def run(self) -> TestResult:
        raise NotImplementedError


class EnvironmentCheck(PythonTest):
    def run(self) -> TestResult:
        started = time.monotonic()
        issues: List[str] = []
        versions: Dict[str, str] = {}
        for executable, args in (("node", ["--version"]), ("cargo", ["--version"])):
            try:
                proc = subprocess.run([executable, *args], capture_output=True, text=True, timeout=15)
                if proc.returncode:
                    issues.append(f"{executable} returned exit code {proc.returncode}")
                else:
                    versions[executable] = proc.stdout.strip()
            except (FileNotFoundError, subprocess.TimeoutExpired):
                issues.append(f"{executable} is unavailable")
        if not (ROOT / "node_modules" / "playwright").is_dir():
            issues.append("Playwright is missing; run npm install")
        status = "failed" if issues else "passed"
        return TestResult(
            status,
            output="\n".join(issues) if issues else ", ".join(f"{k} {v}" for k, v in versions.items()),
            duration_s=time.monotonic() - started,
            expected="Node, Cargo, node_modules, and Playwright are available",
            observed="; ".join(issues) if issues else "Required tools are available",
            measurements=versions,
            failure_kind="infrastructure" if issues else None,
            regression_area="local test environment",
        )


class RegistryCheck(PythonTest):
    def run(self) -> TestResult:
        started = time.monotonic()
        ids = [test.id for test in ALL_TESTS]
        duplicates = sorted({test_id for test_id in ids if ids.count(test_id) > 1})
        missing = sorted(test.script for test in JS_TESTS if test.script and not (TESTS_DIR / test.script).is_file())
        issues = []
        if duplicates:
            issues.append("Duplicate test IDs: " + ", ".join(duplicates))
        if missing:
            issues.append("Registered test files missing: " + ", ".join(missing))
        return TestResult(
            "failed" if issues else "passed",
            output="\n".join(issues) if issues else f"{len(ids)} stable IDs and all registered files exist",
            duration_s=time.monotonic() - started,
            expected="Every registered test has a unique stable ID and an existing implementation",
            observed="; ".join(issues) if issues else "Registry is internally consistent",
            failure_kind="infrastructure" if issues else None,
            regression_area="test registry",
        )


class SettingsContractCheck(PythonTest):
    def run(self) -> TestResult:
        started = time.monotonic()
        rust_text = (ROOT / "src-tauri/src/data/store/mod.rs").read_text(encoding="utf-8")
        ts_text = (ROOT / "src/lib/settings.ts").read_text(encoding="utf-8")
        settings_text = (ROOT / "src-tauri/src/commands/settings.rs").read_text(encoding="utf-8")
        rust_keys = {
            match.group(2)
            for match in re.finditer(r'pub const ([A-Z0-9_]+): &str = "([^"]+)";', rust_text)
            if not match.group(2).startswith("api_key_")
            and match.group(2) not in {"credentials_migrated_v1", "auto_learn_event_mode"}
            and match.group(2) not in {
                "groq", "openai", "google", "assemblyai", "llama-3.1-8b-instant",
                "llama-3.3-70b-versatile", "openai/gpt-oss-20b", "qwen/qwen3.6-27b",
                "paste clipboard here",
            }
        }
        ts_match = re.search(r"type SettingsValueMap = \{(.*?)\n\};", ts_text, re.S)
        payload_match = re.search(r"pub struct AllSettings \{(.*?)\n\}", settings_text, re.S)
        if not ts_match or not payload_match:
            return TestResult("failed", "Could not parse settings contracts", time.monotonic() - started, failure_kind="infrastructure")
        ts_keys = set(re.findall(r"^\s*([a-zA-Z0-9_]+):", ts_match.group(1), re.M))
        payload_keys = set(re.findall(r"pub ([a-zA-Z0-9_]+):", payload_match.group(1)))
        missing_ts = sorted(rust_keys - ts_keys - {"hotkey", "autostart_enabled", "macos_clipboard_sniff_enabled"})
        missing_rust = sorted(ts_keys - rust_keys - {"history_retention", "update_dismissed_version"})
        missing_payload = sorted(ts_keys - payload_keys - {"setup_complete", "force_setup_on_launch", "default_tone", "cleanup_intensity", "app_mappings"})
        problems = []
        if missing_ts:
            problems.append("Missing in TypeScript: " + ", ".join(missing_ts))
        if missing_rust:
            problems.append("Missing Rust constants: " + ", ".join(missing_rust))
        if missing_payload:
            problems.append("Missing settings payload fields: " + ", ".join(missing_payload))
        return TestResult(
            "failed" if problems else "passed",
            output="\n".join(problems) if problems else f"{len(ts_keys)} TypeScript keys, {len(rust_keys)} Rust keys, {len(payload_keys)} payload fields",
            duration_s=time.monotonic() - started,
            expected="Frontend, backend, and IPC settings keys stay synchronized",
            observed="; ".join(problems) if problems else "Settings contracts match",
            failure_kind="product" if problems else None,
            regression_area="settings contract",
        )


class AudioFixtureCheck(PythonTest):
    def run(self) -> TestResult:
        started = time.monotonic()
        if not AUDIO_WAV.exists():
            return TestResult("skipped", duration_s=time.monotonic() - started, expected="A valid WAV exists for live transcription", observed="Fixture is absent", skip_reason=f"Optional fixture not found at {AUDIO_WAV}", regression_area="live test infrastructure")
        size = AUDIO_WAV.stat().st_size
        header = AUDIO_WAV.read_bytes()[:12]
        problems = []
        if size < 5_000:
            problems.append(f"fixture is only {size} bytes")
        if header[:4] != b"RIFF" or header[8:12] != b"WAVE":
            problems.append("fixture lacks a RIFF/WAVE header")
        return TestResult(
            "failed" if problems else "passed",
            output="; ".join(problems) if problems else f"{size} bytes, valid RIFF/WAVE header",
            duration_s=time.monotonic() - started,
            expected="A valid non-trivial WAV exists for live transcription",
            observed="; ".join(problems) if problems else "Fixture is valid",
            measurements={"bytes": size},
            failure_kind="infrastructure" if problems else None,
            regression_area="live test infrastructure",
        )


class NativeCapabilityCheck(PythonTest):
    def run(self) -> TestResult:
        if sys.platform != "win32":
            return TestResult("skipped", expected="Windows native harness prerequisites exist", observed=f"Platform is {sys.platform}", skip_reason="Native profile currently targets Windows hotkey and injection paths", regression_area="native platform coverage")
        exists = (TESTS_DIR / "manual/hotkey.cjs").is_file()
        return TestResult("passed" if exists else "failed", expected="The manual Windows hotkey harness exists", observed="Harness exists" if exists else "Harness is missing", failure_kind="infrastructure" if not exists else None, regression_area="native platform coverage")


PYTHON_ENTRIES = [
    entry("preflight.environment", "preflight", "Environment", expected="Required local test tools are installed", regression_area="local test environment", failure_kind="infrastructure"),
    entry("preflight.registry", "preflight", "Test registry integrity", expected="The test registry contains unique IDs and existing files", regression_area="test registry", failure_kind="infrastructure"),
    entry("contract.settings", "contract", "Settings contract sync", expected="Frontend, backend, and IPC settings contracts match", regression_area="settings contract"),
    entry("native.prerequisites", "native", "Native workflow prerequisites", expected="Platform-specific manual harnesses exist", regression_area="native platform coverage", required=False, failure_kind="infrastructure"),
]

PYTHON_ENTRIES[0].python_test = EnvironmentCheck()
PYTHON_ENTRIES[1].python_test = RegistryCheck()
PYTHON_ENTRIES[2].python_test = SettingsContractCheck()
PYTHON_ENTRIES[3].python_test = NativeCapabilityCheck()

COMMAND_TESTS = [
    entry("unit.frontend", "unit", "Frontend unit tests", command=[NPM, "run", "test:unit"], timeout_s=240, expected="All deterministic TypeScript unit tests pass", regression_area="frontend logic"),
    entry("unit.runner", "unit", "Runner self-tests", command=[sys.executable, str(TESTS_DIR / "test_onepyfone.py")], timeout_s=90, expected="Filtering, protocol parsing, and report serialization stay correct", regression_area="test infrastructure", failure_kind="infrastructure"),
    entry("frontend.typecheck", "frontend", "Frontend typecheck", command=[NPM, "run", "check"], timeout_s=300, expected="Svelte and TypeScript compile without diagnostics", regression_area="frontend compile contract"),
    entry("frontend.build", "frontend", "Frontend production build", command=[NPM, "run", "build"], timeout_s=300, expected="The production frontend bundle builds", regression_area="frontend build"),
    entry("rust.unit", "rust", "Rust unit and integration tests", command=["cargo", "test", "--manifest-path", str(CARGO_TOML)], timeout_s=900, expected="All deterministic backend tests pass", regression_area="backend behavior"),
    entry("prompt.contracts", "contract", "Deterministic prompt regression fixtures", command=["cargo", "test", "--manifest-path", str(CARGO_TOML), "prompt_regression_fixtures_hold", "--", "--nocapture"], timeout_s=180, expected="Prompt assembly preserves all data-driven semantic and safety contracts", regression_area="prompt assembly"),
    entry("pipeline.prompt-live", "pipeline", "Configured model prompt regressions", command=["cargo", "test", "--manifest-path", str(CARGO_TOML), "live_prompt_regression", "--", "--ignored", "--nocapture", "--test-threads=1"], timeout_s=360, required=False, expected="Configured cleanup model obeys semantic prompt invariants", regression_area="prompt and model behavior"),
]

# Bind the audio implementation to its declarative registry entry.
next(test for test in JS_TESTS if test.id == "pipeline.audio-fixture").python_test = AudioFixtureCheck()
ALL_TESTS: List[TestEntry] = PYTHON_ENTRIES + COMMAND_TESTS + JS_TESTS


def _terminate_process_tree(proc: subprocess.Popen[str]) -> None:
    if proc.poll() is not None:
        return
    try:
        if sys.platform == "win32":
            subprocess.run(["taskkill", "/F", "/T", "/PID", str(proc.pid)], capture_output=True, timeout=20)
        else:
            os.killpg(proc.pid, signal.SIGTERM)
        proc.wait(timeout=10)
    except Exception:
        try:
            if sys.platform != "win32":
                os.killpg(proc.pid, signal.SIGKILL)
            else:
                proc.kill()
        except Exception:
            pass
    try:
        proc.wait(timeout=10)
    except Exception:
        pass


def run_process(command: Sequence[str], timeout_s: int, env: Optional[Dict[str, str]] = None) -> tuple[int, str, float, bool]:
    started = time.monotonic()
    kwargs: Dict[str, Any] = {
        "cwd": ROOT,
        "stdout": subprocess.PIPE,
        "stderr": subprocess.STDOUT,
        "text": True,
        "encoding": "utf-8",
        "errors": "replace",
        "env": env,
    }
    if sys.platform == "win32":
        kwargs["creationflags"] = subprocess.CREATE_NEW_PROCESS_GROUP
    else:
        kwargs["start_new_session"] = True
    try:
        proc = subprocess.Popen(list(command), **kwargs)
    except FileNotFoundError:
        return 127, f"Executable not found: {command[0]}", time.monotonic() - started, False
    try:
        output, _ = proc.communicate(timeout=timeout_s)
        return proc.returncode or 0, output.strip(), time.monotonic() - started, False
    except subprocess.TimeoutExpired:
        _terminate_process_tree(proc)
        output = ""
        try:
            remaining, _ = proc.communicate(timeout=2)
            output = remaining.strip()
        except Exception:
            pass
        message = f"Timed out after {timeout_s}s"
        if output:
            message += "\n" + output[-4000:]
        return 124, message, time.monotonic() - started, True


def _parse_protocol(output: str) -> tuple[str, Optional[Dict[str, Any]]]:
    payload: Optional[Dict[str, Any]] = None
    kept: List[str] = []
    for line in output.splitlines():
        if RESULT_PREFIX in line:
            prefix_index = line.index(RESULT_PREFIX)
            try:
                candidate = json.loads(line[prefix_index + len(RESULT_PREFIX):])
                if isinstance(candidate, dict):
                    payload = candidate
                    human_prefix = line[:prefix_index].rstrip()
                    if human_prefix:
                        kept.append(human_prefix)
                    continue
            except json.JSONDecodeError:
                pass
        kept.append(line)
    return "\n".join(kept).strip(), payload


def _classify_failure(entry_: TestEntry, output: str, timed_out: bool) -> str:
    lower = output.lower()
    infrastructure_markers = [
        "executable not found", "cannot find module", "failed to launch", "browser executable",
        "eaddrinuse", "connection refused", "could not start", "registered test files missing",
    ]
    if any(marker in lower for marker in infrastructure_markers):
        return "infrastructure"
    if timed_out and entry_.failure_kind == "infrastructure":
        return "infrastructure"
    return entry_.failure_kind


def execute(entry_: TestEntry, test_url: str) -> TestResult:
    if entry_.python_test is not None:
        try:
            result = entry_.python_test.run()
        except Exception as exc:
            result = TestResult("failed", output=f"Unhandled runner exception: {exc}", observed=repr(exc), failure_kind="infrastructure")
    else:
        if entry_.id == "unit.frontend":
            package_json = ROOT / "package.json"
            try:
                package_data = json.loads(package_json.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError) as exc:
                return TestResult(
                    "skipped",
                    expected=entry_.expected,
                    observed=f"Could not inspect package.json: {exc}",
                    skip_reason="Optional frontend unit suite is unavailable because package.json could not be read",
                    regression_area=entry_.regression_area,
                )
            if "test:unit" not in (package_data.get("scripts") or {}):
                return TestResult(
                    "skipped",
                    expected=entry_.expected,
                    observed="package.json does not define test:unit",
                    skip_reason="Optional frontend unit suite is not configured in this checkout",
                    regression_area=entry_.regression_area,
                )
        command = entry_.command or ["node", str(TESTS_DIR / str(entry_.script))]
        env = dict(os.environ)
        env["TEST_URL"] = test_url
        code, raw_output, duration, timed_out = run_process(command, entry_.timeout_s, env)
        output, protocol = _parse_protocol(raw_output)
        if protocol:
            status = str(protocol.get("status", "passed" if code == 0 else "failed"))
            if status not in {"passed", "failed", "skipped"}:
                status = "failed"
            result = TestResult(
                status=status,
                output=output,
                duration_s=duration,
                expected=str(protocol.get("expected", "")),
                observed=str(protocol.get("observed", "")),
                measurements=dict(protocol.get("measurements") or {}),
                baseline=dict(protocol.get("baseline") or {}),
                regression_area=str(protocol.get("regression_area", "")),
                failure_kind=protocol.get("failure_kind"),
                regression_status=str(protocol.get("regression_status", "unknown")),
                skip_reason=protocol.get("skip_reason"),
            )
            if code and result.status == "passed":
                result.status = "failed"
        else:
            skipped = code == 0 and bool(re.search(r"\bSKIP(?:PED)?\b", output, re.I))
            status = "skipped" if skipped else "passed" if code == 0 else "failed"
            result = TestResult(status, output=output, duration_s=duration)
        if result.status == "failed" and not result.failure_kind:
            result.failure_kind = _classify_failure(entry_, output, timed_out)

    result.expected = result.expected or entry_.expected
    if not result.observed:
        if result.passed:
            result.observed = "Contract held"
        elif result.skipped:
            result.observed = result.skip_reason or result.output or "Optional prerequisite unavailable"
        else:
            lines = [line.strip() for line in result.output.splitlines() if line.strip()]
            observed = lines[-1] if lines else "Test failed without diagnostic output"
            result.observed = re.sub(r"\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])", "", observed)[:1000]
    result.regression_area = result.regression_area or entry_.regression_area
    if result.status == "failed" and not result.failure_kind:
        result.failure_kind = entry_.failure_kind
    if result.skipped and not result.skip_reason:
        result.skip_reason = result.observed
    return result


def run_with_retries(entry_: TestEntry, test_url: str, verbose: bool) -> TestResult:
    result = TestResult("failed")
    total_started = time.monotonic()
    for attempt in range(1, entry_.retries + 2):
        with _OUTPUT_LOCK:
            print(f"  RUN   {entry_.id} ({attempt}/{entry_.retries + 1})", flush=True)
        result = execute(entry_, test_url)
        result.attempts = attempt
        if result.passed or result.skipped:
            break
        if attempt <= entry_.retries:
            with _OUTPUT_LOCK:
                print(f"  RETRY {entry_.id}: {result.observed}", flush=True)
    result.duration_s = time.monotonic() - total_started
    status = "PASS" if result.passed else "SKIP" if result.skipped else "FAIL"
    with _OUTPUT_LOCK:
        print(f"  {status:<5} {entry_.id} [{result.duration_s:.2f}s]")
        if verbose or result.status != "passed":
            detail = result.skip_reason if result.skipped else result.output
            if detail:
                for line in detail.splitlines()[-40:]:
                    print(f"        {line}")
    return result


class ServerManager:
    def __init__(self) -> None:
        self.proc: Optional[subprocess.Popen[Any]] = None
        self.port = PORT
        atexit.register(self.stop)

    @staticmethod
    def is_ready(port: int = PORT, timeout_s: float = 1.0) -> bool:
        for host in ("127.0.0.1", "localhost"):
            try:
                with urllib.request.urlopen(f"http://{host}:{port}", timeout=timeout_s):
                    return True
            except Exception:
                continue
        return False

    def start(self, tauri: bool = False) -> bool:
        if self.is_ready(self.port):
            print(f"  Server: reusing http://localhost:{self.port}")
            self.warm_up()
            return True
        command = [NPM, "run", "tauri", "dev"] if tauri else [NPM, "run", "dev"]
        kwargs: Dict[str, Any] = {"cwd": ROOT, "stdout": subprocess.DEVNULL, "stderr": subprocess.DEVNULL}
        if sys.platform == "win32":
            kwargs["creationflags"] = subprocess.CREATE_NEW_PROCESS_GROUP
        else:
            kwargs["start_new_session"] = True
        try:
            self.proc = subprocess.Popen(command, **kwargs)
        except FileNotFoundError:
            return False
        deadline = time.monotonic() + 180
        while time.monotonic() < deadline:
            if self.proc.poll() is not None:
                return False
            if self.is_ready(self.port):
                print(f"  Server: ready at http://localhost:{self.port}")
                self.warm_up()
                return True
            time.sleep(0.5)
        self.stop()
        return False

    def warm_up(self) -> None:
        script = TESTS_DIR / "_warmup.cjs"
        if not script.exists():
            return
        run_process(["node", str(script), f"http://localhost:{self.port}"], 120, dict(os.environ))

    def stop(self) -> None:
        if self.proc is not None:
            _terminate_process_tree(self.proc)
            self.proc = None


def kill_port_owner(port: int) -> bool:
    if sys.platform != "win32":
        try:
<<<<<<< New base: Consolidate outstanding workspace work for dev
            probe = subprocess.run(
                ["lsof", "-ti", f"tcp:{port}", "-sTCP:LISTEN"],
                capture_output=True,
                text=True,
                timeout=10,
            )
||||||| Common ancestor
            probe = subprocess.run(["lsof", "-ti", f"tcp:{port}", "-sTCP:LISTEN"], capture_output=True, text=True, timeout=10)
=======
            probe = subprocess.run(["lsof", "-ti", f":{port}"], capture_output=True, text=True, timeout=10)
>>>>>>> Current commit: Address AI review findings
        except (FileNotFoundError, subprocess.TimeoutExpired):
            return False
        pids = [pid.strip() for pid in probe.stdout.splitlines() if pid.strip().isdigit()]
        killed = False
        for pid in pids:
            try:
                os.kill(int(pid), 15)
                killed = True
            except (ProcessLookupError, PermissionError):
<<<<<<< New base: Consolidate outstanding workspace work for dev
                continue
        if killed and not wait_for_port_closed(port):
            for pid in pids:
                try:
                    os.kill(int(pid), signal.SIGKILL)
                except (ProcessLookupError, PermissionError):
                    continue
            wait_for_port_closed(port)
||||||| Common ancestor
                pass
        if killed and not wait_for_port_closed(port, timeout_s=3.0):
            for pid in probe.stdout.splitlines():
                if pid.strip().isdigit():
                    try:
                        os.kill(int(pid), signal.SIGKILL)
                    except (ProcessLookupError, PermissionError):
                        pass
=======
                continue
>>>>>>> Current commit: Address AI review findings
        return killed
    command = (
        "try { $id = Get-NetTCPConnection -LocalPort " + str(port) +
        " -State Listen | Select-Object -First 1 -ExpandProperty OwningProcess; "
        "if ($id) { Stop-Process -Id $id -Force; 'killed' } } catch {}"
    )
<<<<<<< New base: Consolidate outstanding workspace work for dev
    try:
        proc = subprocess.run(
            ["powershell", "-NoProfile", "-Command", command],
            capture_output=True,
            text=True,
            timeout=10,
        )
    except (FileNotFoundError, subprocess.TimeoutExpired):
        return False
||||||| Common ancestor
    try:
        proc = subprocess.run(["powershell", "-NoProfile", "-Command", command], capture_output=True, text=True, timeout=10)
    except (FileNotFoundError, subprocess.TimeoutExpired):
        return False
=======
    proc = subprocess.run(["powershell", "-NoProfile", "-Command", command], capture_output=True, text=True)
>>>>>>> Current commit: Address AI review findings
    return "killed" in proc.stdout


<<<<<<< New base: Consolidate outstanding workspace work for dev
def wait_for_port_closed(port: int, timeout_s: float = 10.0) -> bool:
    deadline = time.monotonic() + timeout_s
    while time.monotonic() < deadline:
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=0.2):
                pass
        except OSError:
            return True
        time.sleep(0.1)
    return False


||||||| Common ancestor
def wait_for_port_closed(port: int, timeout_s: float = 15.0) -> bool:
    """Wait until no process is accepting TCP connections on ``port``."""
    deadline = time.monotonic() + timeout_s
    while time.monotonic() < deadline:
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=0.25):
                time.sleep(0.25)
                continue
        except OSError:
            return True
    return False


=======
>>>>>>> Current commit: Address AI review findings
def select_tests(suites: Sequence[str], pattern: str = "") -> List[TestEntry]:
    suite_set = set(suites)
    selected = [test for test in ALL_TESTS if test.suite in suite_set]
    if pattern:
        needle = pattern.lower()
        selected = [
            test for test in selected
            if needle in test.id.lower() or needle in test.name.lower() or needle in test.category.lower()
        ]
    order = {suite: index for index, suite in enumerate(SUITE_ORDER)}
    return sorted(selected, key=lambda test: (order.get(test.suite, 999), test.id))


def run_group(entries: Sequence[TestEntry], url: str, verbose: bool, parallel: bool, workers: int) -> Dict[str, TestResult]:
    if not entries:
        return {}
    can_parallelize = parallel and len(entries) > 1 and all(test.suite in PARALLEL_SAFE_SUITES for test in entries)
    if not can_parallelize:
        return {test.id: run_with_retries(test, url, verbose) for test in entries}
    results: Dict[str, TestResult] = {}
    with concurrent.futures.ThreadPoolExecutor(max_workers=min(workers, len(entries))) as pool:
        future_map = {pool.submit(run_with_retries, test, url, verbose): test for test in entries}
        for future in concurrent.futures.as_completed(future_map):
            test = future_map[future]
            try:
                results[test.id] = future.result()
            except Exception as exc:
                results[test.id] = TestResult("failed", output=f"Runner worker crashed: {exc}", expected=test.expected, observed=repr(exc), regression_area=test.regression_area, failure_kind="infrastructure")
    return results


def execute_plan(entries: Sequence[TestEntry], args: argparse.Namespace) -> Dict[str, TestResult]:
    results: Dict[str, TestResult] = {}
    url = f"http://localhost:{PORT}"
    preflight = [test for test in entries if test.suite == "preflight"]
    no_server = [test for test in entries if test.suite != "preflight" and not test.needs_server]
    server_tests = [test for test in entries if test.needs_server]
    results.update(run_group(preflight, url, args.verbose, False, args.workers))
<<<<<<< New base: Consolidate outstanding workspace work for dev
    if any(
        test.required and results.get(test.id) is not None and results[test.id].status == "failed"
        for test in preflight
    ):
        reason = "Skipped because a required preflight check failed"
        for test in entries:
            if test.suite != "preflight":
                results[test.id] = TestResult(
                    "skipped",
                    output=reason,
                    expected=test.expected,
                    observed=reason,
                    regression_area=test.regression_area,
                    failure_kind="infrastructure",
                    skip_reason=reason,
                )
        return results
||||||| Common ancestor
    failed_preflight = [test for test in preflight if results.get(test.id) and results[test.id].status == "failed" and test.required]
    if failed_preflight:
        reason = "Required preflight check failed; downstream suites were not started"
        for test in entries:
            if test.id not in results:
                results[test.id] = TestResult(
                    "skipped",
                    expected=test.expected,
                    observed=reason,
                    skip_reason=reason,
                    regression_area=test.regression_area,
                    failure_kind="infrastructure",
                )
        return results
=======
>>>>>>> Current commit: Address AI review findings
    results.update(run_group(no_server, url, args.verbose, False, args.workers))

    server = ServerManager()
    try:
        if server_tests and not args.no_server:
            if args.fresh_server:
<<<<<<< New base: Consolidate outstanding workspace work for dev
                if kill_port_owner(PORT):
                    wait_for_port_closed(PORT)
||||||| Common ancestor
                kill_port_owner(PORT)
                wait_for_port_closed(PORT)
=======
                kill_port_owner(PORT)
>>>>>>> Current commit: Address AI review findings
            if not server.start(args.tauri):
                for test in server_tests:
                    results[test.id] = TestResult(
                        "failed",
                        output=f"Could not start the {'Tauri' if args.tauri else 'Vite'} server on port {PORT}",
                        expected=test.expected,
                        observed="The test server did not become ready within 180 seconds",
                        regression_area="test server lifecycle",
                        failure_kind="infrastructure",
                    )
                return results
        by_suite: Dict[str, List[TestEntry]] = {}
        for test in server_tests:
            by_suite.setdefault(test.suite, []).append(test)
        for suite in SUITE_ORDER:
            group = by_suite.get(suite, [])
            if group:
                print(f"\n[{suite}]")
                results.update(run_group(group, url, args.verbose, not args.sequential, args.workers))
    finally:
        if not args.no_server:
            server.stop()
    return results


def merge_loop_results(accumulated: Dict[str, TestResult], current: Dict[str, TestResult]) -> Dict[str, TestResult]:
    merged = dict(accumulated)
    rank = {"passed": 0, "skipped": 1, "failed": 2}
    for test_id, result in current.items():
        previous = merged.get(test_id)
        if previous is None:
            merged[test_id] = result
            continue
        statuses_differ = previous.status != result.status
<<<<<<< New base: Consolidate outstanding workspace work for dev
        chosen = result if rank[result.status] >= rank[previous.status] else previous
        chosen = replace(
            chosen,
            duration_s=previous.duration_s + result.duration_s,
            attempts=previous.attempts + result.attempts,
        )
||||||| Common ancestor
        chosen = replace(result if rank[result.status] >= rank[previous.status] else previous)
        chosen.duration_s = previous.duration_s + result.duration_s
        chosen.attempts = previous.attempts + result.attempts
=======
        chosen = result if rank[result.status] >= rank[previous.status] else previous
        chosen.duration_s = previous.duration_s + result.duration_s
        chosen.attempts = previous.attempts + result.attempts
>>>>>>> Current commit: Address AI review findings
        if statuses_differ:
            chosen = replace(
                chosen,
                regression_status="flaky",
                observed=f"Status changed across loops: {previous.status} -> {result.status}. {chosen.observed}",
            )
        merged[test_id] = chosen
    return merged


def cleanup_artifacts() -> int:
    files = [
        SMOKE_DIR / "screenshot.png",
        SMOKE_DIR / "screenshot-general.png",
        SMOKE_DIR / "screenshot-language-anim.png",
        SMOKE_DIR / "screenshot-privacy.png",
        SMOKE_DIR / "screenshot-apps.png",
        Path(tempfile.gettempdir()) / "verenu-history-filter-fail.png",
    ]
    directories = [ROOT / "tmp-screenshots", SMOKE_DIR / "tmp-screenshots", INTEGRATION_DIR / "tmp-screenshots"]
    removed = 0
    for path in files:
        if not path.is_file():
            continue
        try:
            path.unlink()
            removed += 1
        except OSError:
            pass
    for path in directories:
        if path.is_dir():
            try:
                shutil.rmtree(path)
                removed += 1
            except OSError:
                pass
    return removed


def summary(results: Dict[str, TestResult], entries: Sequence[TestEntry], elapsed_s: float) -> int:
    lookup = {test.id: test for test in entries}
    passed = sum(result.passed for result in results.values())
    skipped = sum(result.skipped for result in results.values())
    failures = [(test_id, result) for test_id, result in results.items() if result.status == "failed"]
    required_failures = [(test_id, result) for test_id, result in failures if lookup[test_id].required]
    print("\nSummary")
    print(f"  {passed} passed, {len(failures)} failed, {skipped} skipped in {elapsed_s:.2f}s")
    if failures:
        print("\nFailures")
        for test_id, result in failures:
            required = "required" if lookup[test_id].required else "optional"
            print(f"  - {test_id} [{result.failure_kind or 'product'}, {required}]")
            print(f"    Expected: {result.expected}")
            print(f"    Observed: {result.observed}")
            print(f"    Area: {result.regression_area}")
    slowest = sorted(results.items(), key=lambda item: item[1].duration_s, reverse=True)[:5]
    if slowest:
        print("\nSlowest")
        for test_id, result in slowest:
            print(f"  - {test_id}: {result.duration_s:.2f}s")
    return 1 if required_failures else 0


def _git_metadata() -> Dict[str, Any]:
    def command(*args: str) -> str:
        try:
            proc = subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True, timeout=10)
            return proc.stdout.strip() if proc.returncode == 0 else ""
        except Exception:
            return ""
    return {
        "commit": command("rev-parse", "HEAD") or None,
        "branch": command("branch", "--show-current") or None,
        "dirty": bool(command("status", "--porcelain")),
    }


def write_json_report(path: Path, profile: str, suites: Sequence[str], entries: Sequence[TestEntry], results: Dict[str, TestResult], elapsed_s: float) -> None:
    lookup = {test.id: test for test in entries}
    tests = []
    for test_id, result in results.items():
        item = asdict(result)
        item.update({
            "id": test_id,
            "name": lookup[test_id].name,
            "suite": lookup[test_id].suite,
            "category": lookup[test_id].category,
            "required": lookup[test_id].required,
            "timeout_s": lookup[test_id].timeout_s,
            "duration_ms": round(result.duration_s * 1000, 2),
        })
        item.pop("duration_s", None)
        tests.append(item)
    counts = {status: sum(result.status == status for result in results.values()) for status in ("passed", "failed", "skipped")}
    payload = {
        "schema_version": 2,
        "runner": "OnePyFone",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "profile": profile,
        "suites": list(suites),
        "duration_ms": round(elapsed_s * 1000, 2),
        "summary": counts,
        "required_failures": [test_id for test_id, result in results.items() if result.status == "failed" and lookup[test_id].required],
        "git": _git_metadata(),
        "tests": tests,
    }
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def _xml_clean(text: str) -> str:
    ansi = re.compile(r"\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])")
    return "".join(ch for ch in ansi.sub("", text) if ch in "\t\n\r" or ord(ch) >= 32)


def _xml_attr(text: str) -> str:
    return xml_escape(_xml_clean(text), {'"': '&quot;', "'": '&apos;'})


def write_junit_report(path: Path, entries: Sequence[TestEntry], results: Dict[str, TestResult]) -> None:
    lookup = {test.id: test for test in entries}
    failures = sum(result.status == "failed" for result in results.values())
    skipped = sum(result.status == "skipped" for result in results.values())
    duration = sum(result.duration_s for result in results.values())
    lines = [
        '<?xml version="1.0" encoding="UTF-8"?>',
        f'<testsuite name="OnePyFone" tests="{len(results)}" failures="{failures}" skipped="{skipped}" time="{duration:.3f}">',
    ]
    for test_id, result in results.items():
        name = _xml_attr(test_id)
        suite = _xml_attr(lookup[test_id].suite)
        lines.append(f'  <testcase classname="{suite}" name="{name}" time="{result.duration_s:.3f}">')
        if result.skipped:
            reason = _xml_attr(result.skip_reason or result.observed)
            lines.append(f'    <skipped message="{reason}"/>')
        elif result.status == "failed":
            detail = xml_escape(_xml_clean(result.output or result.observed))
            lines.append(f'    <failure message="{_xml_attr(result.observed)}">{detail}</failure>')
        lines.append("  </testcase>")
    lines.append("</testsuite>")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def list_tests() -> None:
    for suite in SUITE_ORDER:
        tests = sorted((test for test in ALL_TESTS if test.suite == suite), key=lambda test: test.id)
        if tests:
            print(f"[{suite}]")
            for test in tests:
                optional = " (optional)" if not test.required else ""
                print(f"  {test.id:<34} {test.name}{optional}")


def parse_suites(args: argparse.Namespace) -> List[str]:
    if args.suite == "all":
        return list(SUITE_ORDER)
    suites = [value.strip() for value in args.suite.split(",") if value.strip()] if args.suite else list(PROFILE_SUITES[args.profile])
    unknown = sorted(set(suites) - set(SUITE_ORDER))
    if unknown:
        raise ValueError("Unknown suite(s): " + ", ".join(unknown))
    return suites


def main(argv: Optional[Sequence[str]] = None) -> int:
    parser = argparse.ArgumentParser(description="Verenu unified regression runner (stdlib only)")
    parser.add_argument("--profile", choices=sorted(PROFILE_SUITES), default="fast")
    parser.add_argument("--suite", default="", help="Comma-separated suites or 'all'; overrides profile")
    parser.add_argument("--test", default="", help="Run tests whose ID, name, or category contains this value")
    parser.add_argument("--list", action="store_true", help="List stable test IDs and exit")
    parser.add_argument("--loops", type=int, default=1, help="Repeat the selected tests to expose flakes")
    parser.add_argument("--until-pass", action="store_true", help="Stop repeated runs after the first clean run")
    parser.add_argument("--workers", type=int, default=min(4, os.cpu_count() or 2))
    parser.add_argument("--sequential", action="store_true", help="Disable parallel execution for isolated browser tests")
    parser.add_argument("--parallel", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--tauri", action="store_true", help="Use the full Tauri dev host instead of Vite")
    parser.add_argument("--vite", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--no-server", action="store_true", help="Use an already-running server")
    parser.add_argument("--fresh-server", action="store_true", help="Stop the current port 1420 listener before starting")
    parser.add_argument("--verbose", "-v", action="store_true")
    parser.add_argument("--json-report", default=str(DEFAULT_JSON_REPORT), help="Structured report path (default: test-results/onepyfone.json)")
    parser.add_argument("--no-json-report", action="store_true", help="Do not write the default structured report")
    parser.add_argument("--junit-report", default="", help="Optional JUnit XML report path")
    parser.add_argument("--keep-artifacts", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args(argv)
    args.workers = max(1, args.workers)

    if args.list:
        list_tests()
        return 0
    try:
        suites = parse_suites(args)
    except ValueError as exc:
        parser.error(str(exc))
<<<<<<< New base: Consolidate outstanding workspace work for dev
    selection_suites = list(SUITE_ORDER) if args.test and not args.suite else suites
    entries = select_tests(selection_suites, args.test)
||||||| Common ancestor
    selection_suites = list(SUITE_ORDER) if args.test and not args.suite else suites
    args.workers = max(1, args.workers)
    entries = select_tests(selection_suites, args.test)
=======
    entries = select_tests(suites, args.test)
>>>>>>> Current commit: Address AI review findings
    if not entries:
        print("No tests matched the requested profile, suites, and filter.", file=sys.stderr)
        return 2

    print("Verenu regression suite")
    print(f"Profile: {args.profile} | Suites: {', '.join(suites)} | Tests: {len(entries)} | Workers: {1 if args.sequential else args.workers}")
    overall_exit = 0
    accumulated_results: Dict[str, TestResult] = {}
    total_started = time.monotonic()
    for loop in range(1, max(1, args.loops) + 1):
        if args.loops > 1:
            print(f"\nLoop {loop}/{args.loops}")
        started = time.monotonic()
        results = execute_plan(entries, args)
        exit_code = summary(results, entries, time.monotonic() - started)
<<<<<<< New base: Consolidate outstanding workspace work for dev
        accumulated_results = merge_loop_results(accumulated_results, results)
||||||| Common ancestor
        accumulated_results = results if args.until_pass and exit_code == 0 else merge_loop_results(accumulated_results, results)
        overall_exit = max(overall_exit, exit_code)
=======
        accumulated_results = merge_loop_results(accumulated_results, results)
        overall_exit = max(overall_exit, exit_code)
>>>>>>> Current commit: Address AI review findings
        if args.until_pass and exit_code == 0:
            overall_exit = 0
            break
        overall_exit = max(overall_exit, exit_code)

    elapsed = time.monotonic() - total_started
    if not args.keep_artifacts:
        removed = cleanup_artifacts()
        if removed:
            print(f"\nCleaned {removed} generated browser artifact(s)")
    if not args.no_json_report:
        report_path = Path(args.json_report).resolve()
        write_json_report(report_path, args.profile, suites, entries, accumulated_results, elapsed)
        print(f"\nStructured report: {report_path}")
    if args.junit_report:
        junit_path = Path(args.junit_report).resolve()
        write_junit_report(junit_path, entries, accumulated_results)
        print(f"JUnit report: {junit_path}")
    return overall_exit


if __name__ == "__main__":
    raise SystemExit(main())

