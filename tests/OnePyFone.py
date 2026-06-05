#!/usr/bin/env python3
"""
OnePyFone — Open Flow Unified Test Runner
==========================================

Runs every test suite in the project with one command. Stdlib only — no pip.

QUICKSTART
    python tests/OnePyFone.py                    # fast profile (default)
    python tests/OnePyFone.py --profile full    # everything, including opt-in suites
    python tests/OnePyFone.py --suite rust      # Rust unit tests only (no server)
    python tests/OnePyFone.py --loops 3         # run 3x for flakiness detection

OPTIONS
    --profile NAME     Profile to run (default: fast)
                       Available: fast | live | native | full
    --suite SUITE      Suites to run, comma-separated or "all" (overrides profile)
                       Available: preflight | frontend | rust | contract | ui | state | animation | pipeline | native
    --loops N          Run the entire suite N times (default: 1)
    --until-pass       Stop looping early the moment all tests pass (use with --loops)
    --vite             Compatibility alias for the default Vite-backed browser flow
    --no-server        Skip server lifecycle — assume it is already running
    --verbose, -v      Print full output even for passing tests

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
HOW TO ADD MORE TESTS
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

1. ADD A NEW .cjs BROWSER TEST
   Drop your .cjs file in tests/smoke/ or tests/integration/, then register it in JS_TESTS below:

       ("integration/my-test.cjs", "ui", "Human-readable description", {
           "retry":        2,     # re-run up to N times on failure (0 = no retry)
           "timeout_s":   60,     # kill the process if it exceeds this many seconds
           "needs_server": True,  # False for tests that call external APIs directly
       }),

   Suite names: preflight | frontend | rust | contract | ui | state | animation | pipeline | native
   To create a new group, add its name to SUITE_ORDER then tag tests with it.

2. ADD A PYTHON-NATIVE TEST
   Subclass PythonTest, implement run(), and append to PYTHON_TESTS:

       class MyCheck(PythonTest):
           name         = "My custom check"
           suite        = "ui"       # group this appears in
           retry        = 1
           timeout_s    = 30
           needs_server = True       # set False if the check needs no dev server

           def run(self) -> "TestResult":
               ok  = some_condition()
               msg = "all good" if ok else "expected X, got Y"
               return TestResult(passed=ok, output=msg)

       PYTHON_TESTS.append(MyCheck())

   Python tests in a suite always run before Node.js tests in that suite,
   so they act as fast pre-checks (e.g. AudioFixtureCheck runs before the
   Node pipeline test to surface a missing WAV file immediately).

3. ADD A NEW SUITE
   Insert the new name into SUITE_ORDER at the position you want it to run:

       SUITE_ORDER = ["preflight", "frontend", "rust", "contract", "ui", "state", "animation", "pipeline", "native", "mygroup"]

   Tag any test with suite="mygroup" and it will appear grouped under [mygroup].
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
"""

import argparse
import atexit
import concurrent.futures
import json
import os
import re
import shutil
import socket
import subprocess
import sys
import time
import threading
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Optional
from xml.sax.saxutils import escape as xml_escape

_global_fails = 0

# Force UTF-8 output on Windows so box-drawing and tick characters render correctly.
if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding="utf-8")
        sys.stderr.reconfigure(encoding="utf-8")
    except Exception:
        pass

# ═══ PATHS ════════════════════════════════════════════════════════════════════

ROOT       = Path(__file__).parent.parent.resolve()
TESTS_DIR  = Path(__file__).parent
SMOKE_DIR  = Path(__file__).parent / "smoke"
INTEGRATION_DIR = Path(__file__).parent / "integration"
AUDIO_WAV  = SMOKE_DIR / "smoke_test.wav"
CARGO_TOML = ROOT / "src-tauri" / "Cargo.toml"

# ═══ PORTS ════════════════════════════════════════════════════════════════════

PORT_TAURI = 1420
PORT_VITE  = 1420

# Set in main() so _exec_node can read it without passing port through every call.
_active_test_url: str = f"http://localhost:{PORT_TAURI}"
PARALLEL_SAFE_SUITES = {"ui", "animation"}
_PRINT_LOCK = threading.Lock()

# ═══ SUITE ORDER ══════════════════════════════════════════════════════════════
# Controls the display order. Tests in unlisted suites appear at the end.

SUITE_ORDER = ["preflight", "frontend", "rust", "contract", "ui", "state", "animation", "pipeline", "native"]
PROFILE_SUITES = {
    "fast": ["preflight", "frontend", "rust", "contract", "ui", "state"],
    "live": ["preflight", "pipeline"],
    "native": ["preflight", "native"],
    "full": SUITE_ORDER,
}

# ═══ SMOKE TEST REGISTRY ══════════════════════════════════════════════════════
# Format: (filename, suite, description, options)
# Filenames are relative to tests/smoke/. Missing files are silently skipped.

JS_TESTS = [
    # ── SUITE: ui ─────────────────────────────────────────────────────────────
    ("smoke/test.cjs",                               "ui",        "App mount & DOM structure",            {"retry": 1, "timeout_s": 45}),
    ("smoke/playwright-test-ui.cjs",                 "ui",        "Navigation & interaction",             {"retry": 2, "timeout_s": 90}),
    ("smoke/playwright-test-fixes.cjs",              "ui",        "Element contract assertions",          {"retry": 1, "timeout_s": 45}),
    ("smoke/test-app.cjs",                           "ui",        "App mappings flow",                    {"retry": 2, "timeout_s": 90}),
    ("smoke/test-dictionary-ui.cjs",                 "ui",        "Dictionary UI interaction",            {"retry": 2, "timeout_s": 90}),
    ("integration/playwright-test-onboarding-dev.cjs", "ui",      "Onboarding flow persistence",          {"retry": 1, "timeout_s": 120}),
    ("integration/playwright-test-surface-dev.cjs",    "ui",      "Snippets, dictionary, style surfaces", {"retry": 1, "timeout_s": 120}),
    ("integration/playwright-test-offline-dev.cjs",    "ui",      "Offline toast in browser dev mode",    {"retry": 1, "timeout_s": 60}),

    # ── SUITE: state ──────────────────────────────────────────────────────────
    ("smoke/playwright-test-state.cjs",                "state",     "Settings state persistence",       {"retry": 2, "timeout_s": 90}),
    ("smoke/playwright-test-appearance.cjs",           "state",     "Appearance mode persistence",      {"retry": 2, "timeout_s": 90}),
    ("smoke/playwright-test-devmode.cjs",              "state",     "Developer mode unlock",            {"retry": 1, "timeout_s": 45}),

    # ── SUITE: pipeline (no browser required) ─────────────────────────────────
    ("smoke/playwright-test-pipeline.cjs",            "pipeline",  "API pipeline (smoke_test.wav)",    {"retry": 1, "timeout_s": 120, "needs_server": False}),

    # ── SUITE: animation ──────────────────────────────────────────────────────
    ("smoke/test-all-dropdown-animations.cjs",        "animation", "All dropdown width animations",    {"retry": 3, "timeout_s": 90}),
    ("smoke/test-animation-full.cjs",                 "animation", "Mic dropdown animation (full)",    {"retry": 2, "timeout_s": 45}),
    ("smoke/test-mic-dropdown-animation.cjs",         "animation", "Mic dropdown animation (smoke)",   {"retry": 2, "timeout_s": 45}),
]

# ═══ DATA CLASSES ═════════════════════════════════════════════════════════════

@dataclass
class TestResult:
    passed:   bool
    output:   str   = ""
    duration: float = 0.0
    attempts: int   = 1
    skipped:  bool  = False


@dataclass
class TestEntry:
    script:       Optional[str]    # .cjs filename, or None for Python tests
    suite:        str
    desc:         str
    retry:        int  = 1
    timeout_s:    int  = 60
    needs_server: bool = True
    py_test:      object = None    # PythonTest instance

    @property
    def report_name(self) -> str:
        return f"{self.suite}: {self.desc}"

# ═══ PYTHON TEST BASE ═════════════════════════════════════════════════════════


class PythonTest:
    """Subclass this to add a native Python test. See module docstring."""
    name:         str  = "Unnamed"
    suite:        str  = "preflight"
    retry:        int  = 1
    timeout_s:    int  = 30
    needs_server: bool = False

    def run(self) -> TestResult:
        raise NotImplementedError

# ═══ PYTHON TEST IMPLEMENTATIONS ══════════════════════════════════════════════


class EnvCheck(PythonTest):
    """Verifies node, cargo, node_modules, and Playwright prerequisites are present."""
    name  = "Environment check"
    suite = "preflight"

    def run(self) -> TestResult:
        t0     = time.monotonic()
        issues = []

        try:
            r_node = subprocess.run(["node", "--version"], capture_output=True, text=True)
            if r_node.returncode != 0:
                issues.append("node not found — install Node.js 18+")
        except FileNotFoundError:
            issues.append("node executable not found in PATH")

        if not (ROOT / "node_modules").is_dir():
            issues.append("node_modules missing — run: npm install")

        if not (ROOT / "node_modules" / "playwright").is_dir():
            issues.append("playwright package missing — run: npm install")

        try:
            r_cargo = subprocess.run(["cargo", "--version"], capture_output=True, text=True)
            if r_cargo.returncode != 0:
                issues.append("cargo not found — install Rust toolchain from rustup.rs")
        except FileNotFoundError:
            issues.append("cargo executable not found in PATH")

        if issues:
            return TestResult(False, "\n".join(f"  {i}" for i in issues), time.monotonic() - t0)

        node_ver  = r_node.stdout.strip()
        cargo_ver = r_cargo.stdout.strip().split()[1] if r_cargo.stdout else "?"
        out = f"node {node_ver} · cargo {cargo_ver} · node_modules present"
        return TestResult(True, out, time.monotonic() - t0)


class AudioFixtureCheck(PythonTest):
    """Verifies smoke_test.wav exists, is large enough, and has a valid RIFF/WAVE header."""
    name  = "Audio fixture (smoke_test.wav)"
    suite = "pipeline"

    def run(self) -> TestResult:
        t0 = time.monotonic()
        if not AUDIO_WAV.exists():
            return TestResult(True, f"Skipped — smoke_test.wav not found at {AUDIO_WAV}", time.monotonic() - t0, skipped=True)

        size = AUDIO_WAV.stat().st_size
        if size < 5_000:
            return TestResult(False, f"Only {size} bytes — file is likely corrupt", time.monotonic() - t0)

        with open(AUDIO_WAV, "rb") as f:
            header = f.read(12)
        if header[:4] != b"RIFF" or header[8:12] != b"WAVE":
            return TestResult(False, "Invalid RIFF/WAVE header", time.monotonic() - t0)

        return TestResult(True, f"{size / 1_048_576:.2f} MB · RIFF/WAVE header OK", time.monotonic() - t0)


class ShellCommandTest(PythonTest):
    command: List[str] = []
    cwd: Path = ROOT

    def run(self) -> TestResult:
        t0 = time.monotonic()
        try:
            r = subprocess.run(
                self.command,
                capture_output=True,
                text=True,
                cwd=self.cwd,
                timeout=self.timeout_s,
            )
        except subprocess.TimeoutExpired:
            return TestResult(False, f"Timed out after {self.timeout_s}s", time.monotonic() - t0)
        except FileNotFoundError:
            return TestResult(False, f"Executable not found: {self.command[0]}", time.monotonic() - t0)

        output = (r.stdout + r.stderr).strip()
        if r.returncode == 0:
            summary = output.splitlines()[-1] if output else "Command succeeded"
            return TestResult(True, summary, time.monotonic() - t0)
        tail = "\n".join(output.splitlines()[-30:]) if output else "Command failed"
        return TestResult(False, tail, time.monotonic() - t0)


class FrontendTypecheck(ShellCommandTest):
    name = "Frontend typecheck"
    suite = "frontend"
    timeout_s = 240
    command = ["npm", "run", "check"]


class FrontendBuild(ShellCommandTest):
    name = "Frontend build"
    suite = "frontend"
    timeout_s = 240
    command = ["npm", "run", "build"]


class SettingsContractSyncCheck(PythonTest):
    name = "Settings contract sync"
    suite = "contract"

    def run(self) -> TestResult:
        t0 = time.monotonic()
        rust_path = ROOT / "src-tauri" / "src" / "data" / "store.rs"
        ts_path = ROOT / "src" / "lib" / "settings.ts"
        all_settings_path = ROOT / "src-tauri" / "src" / "commands" / "mod.rs"

        rust_text = rust_path.read_text(encoding="utf-8")
        ts_text = ts_path.read_text(encoding="utf-8")
        all_settings_text = all_settings_path.read_text(encoding="utf-8")

        rust_keys = {
            match.group(2)
            for match in re.finditer(r'pub const ([A-Z0-9_]+): &str = "([^"]+)";', rust_text)
            if not match.group(2).startswith("api_key_")
            and match.group(2) not in {"credentials_migrated_v1", "auto_learn_event_mode"}
            and match.group(2) not in {"groq", "openai", "google"}
        }

        ts_match = re.search(r"type SettingsValueMap = \{(.*?)\n\};", ts_text, re.S)
        if not ts_match:
            return TestResult(False, "Could not parse SettingsValueMap in src/lib/settings.ts", time.monotonic() - t0)
        ts_keys = {
            match.group(1)
            for match in re.finditer(r"^\s*([a-zA-Z0-9_]+):", ts_match.group(1), re.M)
        }

        payload_match = re.search(r"pub struct AllSettings \{(.*?)\n\}", all_settings_text, re.S)
        if not payload_match:
            return TestResult(False, "Could not parse AllSettings in commands/mod.rs", time.monotonic() - t0)
        payload_keys = {
            match.group(1)
            for match in re.finditer(r"pub ([a-zA-Z0-9_]+):", payload_match.group(1))
        }

        missing_in_ts = sorted(k for k in rust_keys if k not in ts_keys and k not in {"hotkey", "autostart_enabled", "macos_clipboard_sniff_enabled"})
        missing_in_rust = sorted(k for k in ts_keys if k not in rust_keys and k not in {"history_retention", "update_dismissed_version"})
        missing_in_payload = sorted(k for k in ts_keys if k not in payload_keys and k not in {"setup_complete", "force_setup_on_launch", "default_tone", "cleanup_intensity", "app_mappings"})

        problems = []
        if missing_in_ts:
            problems.append("Missing in SettingsValueMap: " + ", ".join(missing_in_ts))
        if missing_in_rust:
            problems.append("Missing in store.rs constants: " + ", ".join(missing_in_rust))
        if missing_in_payload:
            problems.append("Missing in AllSettings payload: " + ", ".join(missing_in_payload))

        if problems:
            return TestResult(False, "\n".join(problems), time.monotonic() - t0)

        summary = f"{len(ts_keys)} TS keys · {len(rust_keys)} Rust keys · {len(payload_keys)} AllSettings fields"
        return TestResult(True, summary, time.monotonic() - t0)


class NativeCapabilityCheck(PythonTest):
    name = "Native workflow prerequisites"
    suite = "native"

    def run(self) -> TestResult:
        t0 = time.monotonic()
        if sys.platform != "win32":
            return TestResult(True, "Skipped - native profile targets Windows hotkey/injection paths", time.monotonic() - t0, skipped=True)
        manual_hotkey = ROOT / "tests" / "manual" / "hotkey.cjs"
        if not manual_hotkey.exists():
            return TestResult(False, "tests/manual/hotkey.cjs is missing", time.monotonic() - t0)
        return TestResult(True, "Windows platform detected · manual native harnesses present", time.monotonic() - t0)


class RustTestSuite(PythonTest):
    """Runs the full Rust test suite via cargo and reports per-test failures by name."""
    name      = "Rust unit tests"
    suite     = "rust"
    retry     = 1
    timeout_s = 300

    def run(self) -> TestResult:
        t0 = time.monotonic()
        try:
            r = subprocess.run(
                ["cargo", "test", "--manifest-path", str(CARGO_TOML)],
                capture_output=True, text=True, cwd=ROOT, timeout=self.timeout_s,
            )
        except subprocess.TimeoutExpired:
            return TestResult(False, f"Timed out after {self.timeout_s}s", time.monotonic() - t0)
        except FileNotFoundError:
            return TestResult(False, "cargo executable not found in PATH", time.monotonic() - t0)

        output = r.stdout + r.stderr

        m = re.search(r"test result: (\w+)\. (\d+) passed; (\d+) failed", output)
        if m:
            ok     = m.group(1) == "ok" and int(m.group(3)) == 0
            n_pass = int(m.group(2))
            n_fail = int(m.group(3))
            summary = f"{n_pass} passed, {n_fail} failed"
            if not ok:
                fails = re.findall(r"FAILED\s+(\S+)", output)
                if fails:
                    summary += "\n  Failed: " + ", ".join(fails[:15])
                summary += "\n\n" + output[-2000:]
        else:
            ok      = r.returncode == 0
            summary = "Tests completed" if ok else output[-1500:]

        return TestResult(ok, summary, time.monotonic() - t0)


# ═══ PYTHON TEST LIST ═════════════════════════════════════════════════════════
# Python tests run before Node.js tests in the same suite (stable sort).
# Append new PythonTest instances here.

PYTHON_TESTS: List[PythonTest] = [
    EnvCheck(),
    FrontendTypecheck(),
    FrontendBuild(),
    AudioFixtureCheck(),
    RustTestSuite(),
    SettingsContractSyncCheck(),
    NativeCapabilityCheck(),
]

# ═══ COLORS ═══════════════════════════════════════════════════════════════════

_USE_COLOR: bool = sys.stdout.isatty()

if _USE_COLOR and sys.platform == "win32":
    # Enable VT100 processing in Windows Console Host / PowerShell
    try:
        import ctypes
        ctypes.windll.kernel32.SetConsoleMode(ctypes.windll.kernel32.GetStdHandle(-11), 7)
    except Exception:
        _USE_COLOR = False


def _c(code: str, t: str) -> str:
    return f"\033[{code}m{t}\033[0m" if _USE_COLOR else t


def green(t: str)  -> str: return _c("32", t)
def red(t: str)    -> str: return _c("31", t)
def yellow(t: str) -> str: return _c("33", t)
def bold(t: str)   -> str: return _c("1",  t)
def dim(t: str)    -> str: return _c("2",  t)
def cyan(t: str)   -> str: return _c("36", t)


PASS_  = green("✓")  if _USE_COLOR else "PASS"
FAIL_  = red("✗")    if _USE_COLOR else "FAIL"
SKIP_  = yellow("↷") if _USE_COLOR else "SKIP"
RETRY_ = yellow("↻") if _USE_COLOR else ".."
BAR    = "━" * 62

# ═══ SERVER MANAGER ═══════════════════════════════════════════════════════════


class ServerManager:
    """Manages the lifecycle of the Tauri or Vite dev server."""

    def __init__(self) -> None:
        self._proc: Optional[subprocess.Popen] = None
        self._port: Optional[int] = None
        atexit.register(self.stop)

    def is_port_open(self, port: int) -> bool:
        for family, host in ((socket.AF_INET, "127.0.0.1"), (socket.AF_INET6, "::1")):
            try:
                with socket.socket(family, socket.SOCK_STREAM) as s:
                    s.settimeout(0.5)
                    s.connect((host, port))
                    return True
            except (socket.timeout, ConnectionRefusedError, OSError):
                continue
        return False

    def is_http_ready(self, port: int, timeout_s: float = 1.5) -> bool:
        for host in ("127.0.0.1", "localhost", "[::1]"):
            try:
                with urllib.request.urlopen(f"http://{host}:{port}", timeout=timeout_s):
                    return True
            except Exception:
                continue
        return False

    def start(self, mode: str) -> bool:
        """Start the dev server if not already running. Returns True on success."""
        port = PORT_TAURI if mode == "tauri" else PORT_VITE

        if self.is_port_open(port) and self.is_http_ready(port):
            self._port = port
            return True  # already running — nothing to start

        if mode == "tauri":
            print(f"    {dim('note: first Tauri run compiles Rust — this can take several minutes')}")
        print(f"    starting {mode} dev server on :{port} ...", end=" ", flush=True)

        # On Windows use shell=True so "npm run tauri dev" works as documented.
        # CREATE_NEW_PROCESS_GROUP lets taskkill /T kill the entire tree later.
        try:
            if sys.platform == "win32":
                cmd_str = "npm run tauri dev" if mode == "tauri" else "npm run dev"
                self._proc = subprocess.Popen(
                    cmd_str, shell=True, cwd=ROOT,
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                    creationflags=subprocess.CREATE_NEW_PROCESS_GROUP,
                )
            else:
                import os as _os
                cmd_list = ["npm", "run", "tauri", "dev"] if mode == "tauri" else ["npm", "run", "dev"]
                self._proc = subprocess.Popen(
                    cmd_list, cwd=ROOT,
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                    preexec_fn=_os.setsid,
                )
        except FileNotFoundError:
            return False

        self._port = port
        if self._wait_for_http(port, timeout=180):
            print(green("ready"))
            return True

        print(red("timed out"))
        self.stop()
        return False

    def stop(self) -> None:
        """Kill the dev server process tree."""
        if self._proc is None:
            return
        try:
            if sys.platform == "win32":
                subprocess.run(
                    ["taskkill", "/F", "/T", "/PID", str(self._proc.pid)],
                    capture_output=True,
                )
            else:
                import os as _os, signal as _sig
                _os.killpg(_os.getpgid(self._proc.pid), _sig.SIGTERM)
            self._proc.wait(timeout=10)
        except Exception:
            try:
                self._proc.kill()
            except Exception:
                pass
        self._proc = None

    def _wait_for_port(self, port: int, timeout: int) -> bool:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self.is_port_open(port):
                return True
            time.sleep(0.5)
        return False

    def _wait_for_http(self, port: int, timeout: int) -> bool:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self.is_http_ready(port):
                return True
            time.sleep(0.5)
        return False


def _kill_port_owner(port: int) -> bool:
    """Best-effort kill of the listener process on a port (Windows only)."""
    if sys.platform != "win32":
        return False
    cmd = (
        "try { "
        f"$p = Get-NetTCPConnection -LocalPort {port} -State Listen | Select-Object -First 1 -ExpandProperty OwningProcess; "
        "if ($p) { Stop-Process -Id $p -Force; Write-Output 'killed'; } "
        "} catch {}"
    )
    try:
        r = subprocess.run(["powershell", "-NoProfile", "-Command", cmd], capture_output=True, text=True)
    except FileNotFoundError:
        return False
    return "killed" in (r.stdout or "")


def _cleanup_test_artifacts() -> int:
    """Delete screenshot artifacts produced by smoke tests. Returns removed count."""
    removed = 0
    roots = [ROOT, SMOKE_DIR, INTEGRATION_DIR]
    file_patterns = ["screenshot*.png", "tmp-*.png"]
    dir_names = {"tmp-screenshots"}

    for root in roots:
        for pattern in file_patterns:
            for p in root.glob(pattern):
                if p.is_file():
                    try:
                        p.unlink()
                        removed += 1
                    except OSError:
                        pass
        for name in dir_names:
            d = root / name
            if d.is_dir():
                try:
                    shutil.rmtree(d)
                    removed += 1
                except OSError:
                    pass
    return removed

# ═══ TEST EXECUTION ═══════════════════════════════════════════════════════════


def _exec_node(entry: TestEntry) -> TestResult:
    t0 = time.monotonic()
    # Pass TEST_URL so tests like test-dictionary-ui.cjs that default to :5173
    # connect to whichever server OnePyFone actually started.
    env = {**os.environ, "TEST_URL": _active_test_url}
    try:
        r = subprocess.run(
            ["node", str(TESTS_DIR / entry.script)],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            cwd=ROOT, timeout=entry.timeout_s, env=env,
        )
        return TestResult(r.returncode == 0, (r.stdout + r.stderr).strip(), time.monotonic() - t0)
    except subprocess.TimeoutExpired:
        return TestResult(False, f"Timed out after {entry.timeout_s}s", time.monotonic() - t0)
    except FileNotFoundError:
        return TestResult(False, "node executable not found in PATH", time.monotonic() - t0)


def _exec_python(entry: TestEntry) -> TestResult:
    try:
        return entry.py_test.run()
    except Exception as exc:
        return TestResult(False, f"Unhandled exception: {exc}")


def _run_one(entry: TestEntry) -> TestResult:
    return _exec_python(entry) if entry.py_test is not None else _exec_node(entry)


def run_with_retry(entry: TestEntry, idx: int, total: int, show_elapsed: bool = True) -> TestResult:
    """Run a test, retrying on failure up to entry.retry times."""
    result = TestResult(False)
    for attempt in range(1, entry.retry + 2):
        stop_heartbeat = threading.Event()
        start_t = time.monotonic()

        def _heartbeat() -> None:
            while not stop_heartbeat.wait(1.0):
                elapsed_s = int(time.monotonic() - start_t)
                with _PRINT_LOCK:
                    _show_running_elapsed(entry.desc, idx, total, elapsed_s)

        with _PRINT_LOCK:
            _show_running(entry.desc, idx, total)
        heartbeat_t = None
        if show_elapsed:
            heartbeat_t = threading.Thread(target=_heartbeat, daemon=True)
            heartbeat_t.start()
        try:
            result = _run_one(entry)
        finally:
            stop_heartbeat.set()
            if heartbeat_t is not None:
                heartbeat_t.join(timeout=0.2)
        result.attempts = attempt
        if result.passed or attempt > entry.retry:
            return result
        with _PRINT_LOCK:
            _print_retry_line(entry.desc, attempt, entry.retry, idx, total)
        time.sleep(2)
    return result  # unreachable; satisfies type checker

# ═══ ENTRY BUILDER ════════════════════════════════════════════════════════════


def _build_entries(suites: List[str]) -> List[TestEntry]:
    """Build an ordered list of TestEntry objects for the requested suites."""
    suite_set = set(suites)
    order     = {s: i for i, s in enumerate(SUITE_ORDER)}
    entries: List[TestEntry] = []

    # Python tests first so they precede Node tests within the same suite.
    for pt in PYTHON_TESTS:
        if pt.suite in suite_set:
            entries.append(TestEntry(
                script=None, suite=pt.suite, desc=pt.name,
                retry=pt.retry, timeout_s=pt.timeout_s,
                needs_server=pt.needs_server, py_test=pt,
            ))

    # Node.js browser and smoke tests.
    for (script, suite, desc, opts) in JS_TESTS:
        if suite not in suite_set:
            continue
        path = TESTS_DIR / script
        if not path.exists():
            continue
        entries.append(TestEntry(
            script=script, suite=suite, desc=desc,
            retry=opts.get("retry", 1),
            timeout_s=opts.get("timeout_s", 60),
            needs_server=opts.get("needs_server", True),
        ))

    # Stable sort preserves Python-before-Node ordering within each suite.
    entries.sort(key=lambda e: order.get(e.suite, 99))
    return entries

# ═══ REPORTER ═════════════════════════════════════════════════════════════════


def _print_banner(loop: int, loops: int, suites: List[str], server_label: str) -> None:
    print()
    print(bold(BAR))
    print(f"  {bold('Open Flow')} {dim('·')} {cyan('OnePyFone')} {dim('unified test runner')}")
    loop_label = f"loop {loop}/{loops}" if loops > 1 else "single run"
    print(f"  {dim(loop_label)}  ·  suites: {', '.join(suites)}  ·  {dim(server_label)}")
    print(bold(BAR))


def _prog(idx: int, total: int) -> str:
    """Dimmed [N/T] progress tag."""
    return dim(f"[{idx:2}/{total}]")


def _show_running(desc: str, idx: int, total: int) -> None:
    """Print a 'running' placeholder that the result line will overwrite."""
    if not _USE_COLOR:
        return
    arrow = dim("▶")
    print(f"\033[2K\r    {_prog(idx, total)}  {arrow}  {desc}", end="", flush=True)


def _show_running_elapsed(desc: str, idx: int, total: int, elapsed_s: int) -> None:
    """Refresh the running placeholder with elapsed seconds."""
    if not _USE_COLOR:
        return
    arrow = dim("▶")
    elapsed = dim(f"{elapsed_s}s")
    print(f"\033[2K\r    {_prog(idx, total)}  {arrow}  {desc}  {elapsed}", end="", flush=True)


def _clear_running() -> None:
    """Erase the running placeholder before printing a permanent line."""
    if _USE_COLOR:
        print("\033[2K\r", end="", flush=True)


def _print_suite_header(suite: str) -> None:
    print(f"\n  {dim('[' + suite + ']')}")


def _draw_bottom_bar(done: int, total: int, n_fail: int) -> None:
    """Render the persistent bottom progress bar (no trailing newline so it's overwriteable)."""
    if not _USE_COLOR:
        return
    pct    = int(done / total * 100) if total else 0
    filled = int(done / total * 26) if total else 0
    bar    = "█" * filled + "░" * (26 - filled)
    counts = f"{done}/{total}"
    if n_fail:
        badge = red(f"  {n_fail} failed")
    elif done > 0:
        badge = green("  passing")
    else:
        badge = ""
    print(f"\n  {dim(bar)}  {dim(counts)}  {dim(str(pct) + '%')}{badge}", end="", flush=True)


def _print_result_line(desc: str, result: TestResult, verbose: bool, idx: int, total: int) -> None:
    _clear_running()
    mark = SKIP_ if result.skipped else PASS_ if result.passed else FAIL_
    dur  = dim(f"{result.duration:.1f}s")
    print(f"    {_prog(idx, total)}  {mark}  {desc:<45} {dur}")
    if verbose or not result.passed or result.skipped:
        for line in result.output.splitlines():
            print(f"               {dim(line)}")


def _print_retry_line(desc: str, attempt: int, max_retries: int, idx: int, total: int) -> None:
    _clear_running()
    print(f"    {_prog(idx, total)}  {FAIL_}  {desc:<45} {yellow(f'(retry {attempt}/{max_retries})')}")


def _print_summary(results: Dict[str, TestResult], elapsed: float) -> int:
    n_skip  = sum(1 for r in results.values() if r.skipped)
    n_pass  = sum(1 for r in results.values() if r.passed and not r.skipped)
    n_total = len(results)
    n_fail  = n_total - n_pass - n_skip
    print()
    print(bold(BAR))
    if n_fail == 0:
        mark   = green("✓") if _USE_COLOR else "PASS"
        status = bold(green("ALL TESTS PASSED")) if _USE_COLOR else "ALL TESTS PASSED"
        detail = f"{n_pass} passed"
        if n_skip:
            detail += f", {n_skip} skipped"
        print(f"  {mark}  {status}  ·  {detail}  ·  {_fmt_time(elapsed)}")
    else:
        mark   = red("✗") if _USE_COLOR else "FAIL"
        status = bold(red(f"{n_fail} TEST{'S' if n_fail > 1 else ''} FAILED")) if _USE_COLOR else f"{n_fail} FAILED"
        detail = f"{n_pass} passed"
        if n_skip:
            detail += f", {n_skip} skipped"
        print(f"  {mark}  {status}  ·  {detail}  ·  {_fmt_time(elapsed)}")
    print(bold(BAR))
    print()
    return 0 if n_fail == 0 else 1


def _print_slowest(results: Dict[str, TestResult], limit: int = 5) -> None:
    rows = sorted(results.items(), key=lambda item: item[1].duration, reverse=True)[:limit]
    if not rows:
        return
    print(f"  {bold('Slowest tests:')}")
    for name, result in rows:
        status = "skipped" if result.skipped else "failed" if not result.passed else "passed"
        print(f"  {name:<49} {result.duration:>5.1f}s  {dim(status)}")
    print()


def _print_loop_table(summaries: List[Dict[str, TestResult]]) -> None:
    n     = len(summaries)
    names = list(summaries[0].keys())
    print(f"\n  {bold(f'Loop results ({n} runs):')}")
    print(f"  {'Test':<49} {'Pass':>5}  Flaky")
    print(f"  {'─' * 62}")
    any_flaky = False
    for name in names:
        passes = sum(1 for s in summaries if s.get(name, TestResult(False)).passed)
        flaky  = 0 < passes < n
        any_flaky = any_flaky or flaky
        flag = yellow("Yes ←") if flaky else dim("No")
        print(f"  {name:<49} {passes}/{n}   {flag}")
    total      = len(names) * n
    total_pass = sum(sum(1 for r in s.values() if r.passed) for s in summaries)
    suffix     = f"  {yellow('Flaky tests detected.')}" if any_flaky else ""
    print()
    print(bold(f"  {total_pass}/{total} passed across {n} loops.{suffix}"))
    print()


def _fmt_time(secs: float) -> str:
    m = int(secs // 60)
    s = int(secs % 60)
    return f"{m}m {s:02d}s" if m else f"{s:.1f}s"


def _write_json_report(path: Path, profile: str, suites: List[str], results: Dict[str, TestResult]) -> None:
    payload = {
        "profile": profile,
        "suites": suites,
        "results": [
            {
                "name": name,
                "passed": result.passed,
                "skipped": result.skipped,
                "duration_s": round(result.duration, 3),
                "attempts": result.attempts,
                "output": result.output,
            }
            for name, result in results.items()
        ],
    }
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2), encoding="utf-8")


def _sanitize_xml(text: str) -> str:
    ansi_escape = re.compile(r'\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])')
    text = ansi_escape.sub('', text)
    return "".join(c for c in text if c in "\t\n\r" or ord(c) >= 32)


def xml_attr_escape(val: str) -> str:
    return xml_escape(_sanitize_xml(val), {'"': '&quot;', "'": '&apos;'})


def _write_junit_report(path: Path, profile: str, results: Dict[str, TestResult]) -> None:
    total = len(results)
    failures = sum(1 for result in results.values() if not result.passed and not result.skipped)
    skipped = sum(1 for result in results.values() if result.skipped)
    duration = sum(result.duration for result in results.values())
    lines = [
        '<?xml version="1.0" encoding="UTF-8"?>',
        f'<testsuite name="OnePyFone:{xml_attr_escape(profile)}" tests="{total}" failures="{failures}" skipped="{skipped}" time="{duration:.3f}">',
    ]
    for name, result in results.items():
        lines.append(f'  <testcase classname="OnePyFone" name="{xml_attr_escape(name)}" time="{result.duration:.3f}">')
        if result.skipped:
            lines.append(f'    <skipped message="{xml_attr_escape(result.output or "Skipped")}"/>')
        elif not result.passed:
            lines.append(f'    <failure message="Test failed">{xml_escape(_sanitize_xml(result.output))}</failure>')
        lines.append("  </testcase>")
    lines.append("</testsuite>")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")

# ═══ LOOP RUNNER ══════════════════════════════════════════════════════════════


def _run_sequential(entries: List[TestEntry], verbose: bool, start_idx: int, total: int) -> Dict[str, TestResult]:
    results: Dict[str, TestResult] = {}
    n_fail = 0
    for offset, entry in enumerate(entries):
        idx = start_idx + offset
        result = run_with_retry(entry, idx, total, show_elapsed=True)
        with _PRINT_LOCK:
            _print_result_line(entry.desc, result, verbose, idx, total)
        if not result.passed:
            n_fail += 1
            global _global_fails
            _global_fails += 1
        results[entry.report_name] = result
        with _PRINT_LOCK:
            _draw_bottom_bar(idx, total, _global_fails)
    return results


def _run_parallel(entries: List[TestEntry], verbose: bool, start_idx: int, total: int, workers: int) -> Dict[str, TestResult]:
    results: Dict[str, TestResult] = {}
    n_fail = 0
    indexed = [(start_idx + i, e) for i, e in enumerate(entries)]
    with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as pool:
        future_map = {
            pool.submit(run_with_retry, entry, idx, total, False): (idx, entry.desc, entry.report_name)
            for idx, entry in indexed
        }
        for future in concurrent.futures.as_completed(future_map):
            idx, desc, report_name = future_map[future]
            result = future.result()
            with _PRINT_LOCK:
                _print_result_line(desc, result, verbose, idx, total)
            if not result.passed:
                n_fail += 1
                global _global_fails
                _global_fails += 1
            results[report_name] = result
            done = len(results)
            with _PRINT_LOCK:
                _draw_bottom_bar(start_idx + done - 1, total, _global_fails)
    return results


def _run_loop(
    entries: List[TestEntry],
    verbose: bool,
    parallel: bool,
    workers: int,
    start_idx: int,
    total: int,
) -> Dict[str, TestResult]:
    results: Dict[str, TestResult] = {}
    by_suite: Dict[str, List[TestEntry]] = {}
    for entry in entries:
        by_suite.setdefault(entry.suite, []).append(entry)

    cursor = start_idx
    for suite in [s for s in SUITE_ORDER if s in by_suite]:
        suite_entries = by_suite[suite]
        with _PRINT_LOCK:
            _print_suite_header(suite)
        run_parallel = parallel and workers > 1 and suite in PARALLEL_SAFE_SUITES and len(suite_entries) > 1
        suite_results = (
            _run_parallel(suite_entries, verbose, cursor, total, workers)
            if run_parallel else
            _run_sequential(suite_entries, verbose, cursor, total)
        )
        results.update(suite_results)
        cursor += len(suite_entries)

    if total > 0 and _USE_COLOR:
        with _PRINT_LOCK:
            print()
    return results

# ═══ MAIN ═════════════════════════════════════════════════════════════════════


def main() -> int:
    ap = argparse.ArgumentParser(
        prog="OnePyFone",
        description="Open Flow unified test runner — stdlib only, no pip",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="Profiles: fast | live | native | full · Suites: preflight | frontend | rust | contract | ui | state | animation | pipeline | native | all",
    )
    ap.add_argument("--profile",    default="fast", metavar="NAME",
                    help="Profile to run: fast | live | native | full")
    ap.add_argument("--suite",      default="", metavar="SUITE",
                    help="Suite(s) to run, comma-separated or 'all' (overrides profile)")
    ap.add_argument("--loops",      type=int, default=1, metavar="N",
                    help="Run N times (flakiness detection)")
    ap.add_argument("--until-pass", action="store_true",
                    help="Stop looping early when all tests pass (use with --loops)")
    ap.add_argument("--vite",       action="store_true",
                    help="Compatibility alias for the default Vite-backed browser flow")
    ap.add_argument("--tauri",      action="store_true",
                    help="Use full Tauri dev instead of the default Vite browser server")
    ap.add_argument("--no-server",  action="store_true",
                    help="Skip server lifecycle — assume it is already running")
    ap.add_argument("--verbose", "-v", action="store_true",
                    help="Show full output even for passing tests")
    ap.add_argument("--parallel", action="store_true",
                    help="Run parallel-safe suites (ui/animation) concurrently")
    ap.add_argument("--workers", type=int, default=4, metavar="N",
                    help="Max worker threads for --parallel (default: 4)")
    ap.add_argument("--fresh-server", action="store_true",
                    help="Kill any existing listener on the test port before starting server")
    ap.add_argument("--keep-artifacts", action="store_true",
                    help="Keep screenshots and temp screenshot folders after run")
    ap.add_argument("--json-report", metavar="PATH",
                    help="Write machine-readable JSON results")
    ap.add_argument("--junit-report", metavar="PATH",
                    help="Write JUnit XML results")
    args = ap.parse_args()

    profile = args.profile.strip().lower()
    if profile not in PROFILE_SUITES:
        print(red(f"Unknown profile: {args.profile}"))
        return 1

    suites = (
        list(SUITE_ORDER)
        if args.suite == "all"
        else [s.strip() for s in args.suite.split(",") if s.strip()]
        if args.suite
        else list(PROFILE_SUITES[profile])
    )

    entries = _build_entries(suites)
    if not entries:
        print(red("No tests match the requested suite(s)."))
        return 1

    server_mode  = "tauri" if args.tauri else "vite"
    port         = PORT_TAURI if args.tauri else PORT_VITE
    needs_server = any(e.needs_server for e in entries)

    # Tell _exec_node which URL to hand to tests that take TEST_URL (e.g. test-dictionary-ui.cjs).
    global _active_test_url
    _active_test_url = f"http://localhost:{port}"

    server = ServerManager()
    no_server_entries = [entry for entry in entries if not entry.needs_server]
    server_entries = [entry for entry in entries if entry.needs_server]

    loops      = max(1, args.loops)
    summaries: List[Dict[str, TestResult]] = []
    final_exit = 0
    last_results: Dict[str, TestResult] = {}

    try:
        for loop in range(1, loops + 1):
            server_label = f"{server_mode} :{port}" if needs_server else "no server"
            _print_banner(loop, loops, suites, server_label)
            t0      = time.monotonic()
            workers = max(1, args.workers)
            global _global_fails
            _global_fails = 0
            results: Dict[str, TestResult] = {}

            if no_server_entries:
                results.update(_run_loop(no_server_entries, args.verbose, args.parallel, workers, 1, len(entries)))

            if server_entries and not args.no_server:
                print(f"\n  {bold('Server')}")
                if args.fresh_server and _kill_port_owner(port):
                    print(f"    {dim(f'killed existing process on :{port}')}")
                    time.sleep(0.5)
                if server.is_port_open(port):
                    if server.is_http_ready(port):
                        print(f"    {PASS_}  {server_mode} already running on :{port}")
                    else:
                        print(f"    {yellow('!')}  port :{port} is open but HTTP is not ready, restarting...")
                        _kill_port_owner(port)
                        if not server.start(server_mode):
                            print(red(f"\n  Could not start {server_mode} dev server."))
                            return 1
                elif not server.start(server_mode):
                    print(red(f"\n  Could not start {server_mode} dev server."))
                    print(  f"  • Is npm installed and are node_modules present?")
                    print(  f"  • Use --no-server if the server is already running elsewhere.")
                    print(  f"  • Use --suite rust or --profile live to run without a server.")
                    return 1

            if server_entries:
                results.update(
                    _run_loop(
                        server_entries,
                        args.verbose,
                        args.parallel,
                        workers,
                        len(no_server_entries) + 1,
                        len(entries),
                    )
                )

            code    = _print_summary(results, time.monotonic() - t0)
            _print_slowest(results)
            summaries.append(results)
            last_results = results
            if code != 0:
                final_exit = 1
            if args.until_pass and code == 0:
                if loops > 1:
                    print(green("  All tests passed — stopping early."))
                break
    finally:
        if needs_server and not args.no_server:
            server.stop()
        if not args.keep_artifacts:
            removed = _cleanup_test_artifacts()
            if removed > 0:
                print(dim(f"  cleaned {removed} test artifact(s)"))

    if len(summaries) > 1:
        _print_loop_table(summaries)

    if args.json_report:
        _write_json_report(Path(args.json_report), profile, suites, last_results)
    if args.junit_report:
        _write_junit_report(Path(args.junit_report), profile, last_results)

    # Pause so the window stays open when launched by double-clicking.
    if sys.stdin.isatty():
        try:
            input(f"  {'Press Enter to close...'}\n")
        except (EOFError, KeyboardInterrupt):
            pass

    return final_exit


if __name__ == "__main__":
    sys.exit(main())
