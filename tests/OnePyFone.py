#!/usr/bin/env python3
"""
OnePyFone — Open Flow Unified Test Runner
==========================================

Runs every test suite in the project with one command. Stdlib only — no pip.

QUICKSTART
    python tests/OnePyFone.py              # full suite, auto-starts Tauri server
    python tests/OnePyFone.py --suite rust # Rust unit tests only (no server)
    python tests/OnePyFone.py --loops 3   # run 3x for flakiness detection

OPTIONS
    --suite SUITE      Suites to run, comma-separated or "all" (default: all)
                       Available: preflight | rust | pipeline | ui | state | animation
    --loops N          Run the entire suite N times (default: 1)
    --until-pass       Stop looping early the moment all tests pass (use with --loops)
    --vite             Start Vite dev server on :5173 instead of Tauri on :1420
    --no-server        Skip server lifecycle — assume it is already running
    --verbose, -v      Print full output even for passing tests

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
HOW TO ADD MORE TESTS
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

1. ADD A NEW .cjs SMOKE TEST
   Drop your .cjs file in tests/smoke/, then register it in SMOKE_TESTS below:

       ("my-test.cjs", "ui", "Human-readable description", {
           "retry":        2,     # re-run up to N times on failure (0 = no retry)
           "timeout_s":   60,     # kill the process if it exceeds this many seconds
           "needs_server": True,  # False for tests that call external APIs directly
       }),

   Suite names: preflight | rust | pipeline | ui | state | animation
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

       SUITE_ORDER = ["preflight", "rust", "pipeline", "ui", "state", "animation", "mygroup"]

   Tag any test with suite="mygroup" and it will appear grouped under [mygroup].
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
"""

import argparse
import atexit
import concurrent.futures
import os
import re
import socket
import subprocess
import sys
import time
import threading
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Optional

# Force UTF-8 output on Windows so box-drawing and tick characters render correctly.
if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding="utf-8")
        sys.stderr.reconfigure(encoding="utf-8")
    except Exception:
        pass

# ═══ PATHS ════════════════════════════════════════════════════════════════════

ROOT       = Path(__file__).parent.parent.resolve()
SMOKE_DIR  = Path(__file__).parent / "smoke"
AUDIO_WAV  = SMOKE_DIR / "smoke_test.wav"
CARGO_TOML = ROOT / "src-tauri" / "Cargo.toml"

# ═══ PORTS ════════════════════════════════════════════════════════════════════

PORT_TAURI = 1420
PORT_VITE  = 5173

# Set in main() so _exec_node can read it without passing port through every call.
_active_test_url: str = f"http://localhost:{PORT_TAURI}"
PARALLEL_SAFE_SUITES = {"ui", "animation"}
_PRINT_LOCK = threading.Lock()

# ═══ SUITE ORDER ══════════════════════════════════════════════════════════════
# Controls the display order. Tests in unlisted suites appear at the end.

SUITE_ORDER = ["preflight", "rust", "pipeline", "ui", "state", "animation"]

# ═══ SMOKE TEST REGISTRY ══════════════════════════════════════════════════════
# Format: (filename, suite, description, options)
# Filenames are relative to tests/smoke/. Missing files are silently skipped.

SMOKE_TESTS = [
    # ── SUITE: ui ─────────────────────────────────────────────────────────────
    ("test.cjs",                         "ui",        "App mount & DOM structure",        {"retry": 1, "timeout_s": 45}),
    ("playwright-test-ui.cjs",           "ui",        "Navigation & interaction",         {"retry": 2, "timeout_s": 90}),
    ("playwright-test-fixes.cjs",        "ui",        "Element contract assertions",      {"retry": 1, "timeout_s": 45}),
    ("test-app.cjs",                     "ui",        "App mappings flow",                {"retry": 2, "timeout_s": 90}),
    ("test-dictionary-ui.cjs",           "ui",        "Dictionary UI interaction",        {"retry": 2, "timeout_s": 90}),

    # ── SUITE: state ──────────────────────────────────────────────────────────
    ("playwright-test-state.cjs",        "state",     "Settings state persistence",       {"retry": 2, "timeout_s": 90}),
    ("playwright-test-appearance.cjs",   "state",     "Appearance mode persistence",      {"retry": 2, "timeout_s": 90}),
    ("playwright-test-devmode.cjs",      "state",     "Developer mode unlock",            {"retry": 1, "timeout_s": 45}),

    # ── SUITE: pipeline (no browser required) ─────────────────────────────────
    ("playwright-test-pipeline.cjs",     "pipeline",  "API pipeline (smoke_test.wav)",    {"retry": 1, "timeout_s": 120, "needs_server": False}),

    # ── SUITE: animation ──────────────────────────────────────────────────────
    ("test-all-dropdown-animations.cjs", "animation", "All dropdown width animations",    {"retry": 3, "timeout_s": 90}),
    ("test-animation-full.cjs",          "animation", "Mic dropdown animation (full)",    {"retry": 2, "timeout_s": 45}),
    ("test-mic-dropdown-animation.cjs",  "animation", "Mic dropdown animation (smoke)",   {"retry": 2, "timeout_s": 45}),
]

# ═══ DATA CLASSES ═════════════════════════════════════════════════════════════

@dataclass
class TestResult:
    passed:   bool
    output:   str   = ""
    duration: float = 0.0
    attempts: int   = 1


@dataclass
class TestEntry:
    script:       Optional[str]    # .cjs filename, or None for Python tests
    suite:        str
    desc:         str
    retry:        int  = 1
    timeout_s:    int  = 60
    needs_server: bool = True
    py_test:      object = None    # PythonTest instance

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
    """Verifies node, cargo, node_modules, and the audio fixture are all present."""
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

        if not AUDIO_WAV.exists():
            issues.append(f"smoke_test.wav missing at {AUDIO_WAV}")

        if issues:
            return TestResult(False, "\n".join(f"  {i}" for i in issues), time.monotonic() - t0)

        size_mb   = AUDIO_WAV.stat().st_size / 1_048_576
        node_ver  = r_node.stdout.strip()
        cargo_ver = r_cargo.stdout.strip().split()[1] if r_cargo.stdout else "?"
        out = f"node {node_ver} · cargo {cargo_ver} · smoke_test.wav {size_mb:.1f} MB"
        return TestResult(True, out, time.monotonic() - t0)


class AudioFixtureCheck(PythonTest):
    """Verifies smoke_test.wav exists, is large enough, and has a valid RIFF/WAVE header."""
    name  = "Audio fixture (smoke_test.wav)"
    suite = "pipeline"

    def run(self) -> TestResult:
        t0 = time.monotonic()
        if not AUDIO_WAV.exists():
            return TestResult(False, f"Not found at {AUDIO_WAV}", time.monotonic() - t0)

        size = AUDIO_WAV.stat().st_size
        if size < 5_000:
            return TestResult(False, f"Only {size} bytes — file is likely corrupt", time.monotonic() - t0)

        with open(AUDIO_WAV, "rb") as f:
            header = f.read(12)
        if header[:4] != b"RIFF" or header[8:12] != b"WAVE":
            return TestResult(False, "Invalid RIFF/WAVE header", time.monotonic() - t0)

        return TestResult(True, f"{size / 1_048_576:.2f} MB · RIFF/WAVE header OK", time.monotonic() - t0)


class RustTestSuite(PythonTest):
    """Runs the full Rust test suite via cargo and reports per-test failures by name."""
    name      = "Rust unit tests"
    suite     = "rust"
    retry     = 1
    timeout_s = 300

    def run(self) -> TestResult:
        t0 = time.monotonic()
        try:
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
    AudioFixtureCheck(),
    RustTestSuite(),
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
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            s.settimeout(0.5)
            try:
                s.connect(("127.0.0.1", port))
                return True
            except (socket.timeout, ConnectionRefusedError, OSError):
                return False

    def is_http_ready(self, port: int, timeout_s: float = 1.5) -> bool:
        try:
            with urllib.request.urlopen(f"http://127.0.0.1:{port}", timeout=timeout_s):
                return True
        except Exception:
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
        return "killed" in (r.stdout or "")
    except FileNotFoundError:
        return False

# ═══ TEST EXECUTION ═══════════════════════════════════════════════════════════


def _exec_node(entry: TestEntry) -> TestResult:
    t0 = time.monotonic()
    # Pass TEST_URL so tests like test-dictionary-ui.cjs that default to :5173
    # connect to whichever server OnePyFone actually started.
    env = {**os.environ, "TEST_URL": _active_test_url}
    try:
        r = subprocess.run(
            ["node", str(SMOKE_DIR / entry.script)],
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

    # Node.js smoke tests.
    for (script, suite, desc, opts) in SMOKE_TESTS:
        if suite not in suite_set:
            continue
        path = SMOKE_DIR / script
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
    mark = PASS_ if result.passed else FAIL_
    dur  = dim(f"{result.duration:.1f}s")
    print(f"    {_prog(idx, total)}  {mark}  {desc:<45} {dur}")
    if verbose or not result.passed:
        for line in result.output.splitlines():
            print(f"               {dim(line)}")


def _print_retry_line(desc: str, attempt: int, max_retries: int, idx: int, total: int) -> None:
    _clear_running()
    print(f"    {_prog(idx, total)}  {FAIL_}  {desc:<45} {yellow(f'(retry {attempt}/{max_retries})')}")


def _print_summary(results: Dict[str, TestResult], elapsed: float) -> int:
    n_pass  = sum(1 for r in results.values() if r.passed)
    n_total = len(results)
    n_fail  = n_total - n_pass
    print()
    print(bold(BAR))
    if n_fail == 0:
        mark   = green("✓") if _USE_COLOR else "PASS"
        status = bold(green("ALL TESTS PASSED")) if _USE_COLOR else "ALL TESTS PASSED"
        print(f"  {mark}  {status}  ·  {n_pass}/{n_total}  ·  {_fmt_time(elapsed)}")
    else:
        mark   = red("✗") if _USE_COLOR else "FAIL"
        status = bold(red(f"{n_fail} TEST{'S' if n_fail > 1 else ''} FAILED")) if _USE_COLOR else f"{n_fail} FAILED"
        print(f"  {mark}  {status}  ·  {n_pass}/{n_total} passed  ·  {_fmt_time(elapsed)}")
    print(bold(BAR))
    print()
    return 0 if n_fail == 0 else 1


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
        results[entry.desc] = result
        with _PRINT_LOCK:
            _draw_bottom_bar(idx, total, n_fail)
    return results


def _run_parallel(entries: List[TestEntry], verbose: bool, start_idx: int, total: int, workers: int) -> Dict[str, TestResult]:
    results: Dict[str, TestResult] = {}
    n_fail = 0
    indexed = [(start_idx + i, e) for i, e in enumerate(entries)]
    with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as pool:
        future_map = {
            pool.submit(run_with_retry, entry, idx, total, False): (idx, entry.desc)
            for idx, entry in indexed
        }
        for future in concurrent.futures.as_completed(future_map):
            idx, desc = future_map[future]
            result = future.result()
            with _PRINT_LOCK:
                _print_result_line(desc, result, verbose, idx, total)
            if not result.passed:
                n_fail += 1
            results[desc] = result
            done = len(results)
            with _PRINT_LOCK:
                _draw_bottom_bar(start_idx + done - 1, total, n_fail)
    return results


def _run_loop(entries: List[TestEntry], verbose: bool, parallel: bool, workers: int) -> Dict[str, TestResult]:
    results: Dict[str, TestResult] = {}
    total = len(entries)
    by_suite: Dict[str, List[TestEntry]] = {}
    for entry in entries:
        by_suite.setdefault(entry.suite, []).append(entry)

    start_idx = 1
    for suite in [s for s in SUITE_ORDER if s in by_suite]:
        suite_entries = by_suite[suite]
        with _PRINT_LOCK:
            _print_suite_header(suite)
        run_parallel = parallel and workers > 1 and suite in PARALLEL_SAFE_SUITES and len(suite_entries) > 1
        suite_results = (
            _run_parallel(suite_entries, verbose, start_idx, total, workers)
            if run_parallel else
            _run_sequential(suite_entries, verbose, start_idx, total)
        )
        results.update(suite_results)
        start_idx += len(suite_entries)

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
        epilog="Suites: preflight | rust | pipeline | ui | state | animation | all",
    )
    ap.add_argument("--suite",      default="all", metavar="SUITE",
                    help="Suite(s) to run, comma-separated or 'all'")
    ap.add_argument("--loops",      type=int, default=1, metavar="N",
                    help="Run N times (flakiness detection)")
    ap.add_argument("--until-pass", action="store_true",
                    help="Stop looping early when all tests pass (use with --loops)")
    ap.add_argument("--vite",       action="store_true",
                    help="Use Vite dev server on :5173 instead of Tauri on :1420")
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
    args = ap.parse_args()

    suites = list(SUITE_ORDER) if args.suite == "all" else [
        s.strip() for s in args.suite.split(",") if s.strip()
    ]

    entries = _build_entries(suites)
    if not entries:
        print(red("No tests match the requested suite(s)."))
        return 1

    server_mode  = "vite" if args.vite else "tauri"
    port         = PORT_VITE if args.vite else PORT_TAURI
    needs_server = any(e.needs_server for e in entries)

    # Tell _exec_node which URL to hand to tests that take TEST_URL (e.g. test-dictionary-ui.cjs).
    global _active_test_url
    _active_test_url = f"http://localhost:{port}"

    server = ServerManager()

    if needs_server and not args.no_server:
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
            print(  f"  • Use --suite rust or --suite pipeline to run without a server.")
            return 1

    loops      = max(1, args.loops)
    summaries: List[Dict[str, TestResult]] = []
    final_exit = 0

    try:
        for loop in range(1, loops + 1):
            server_label = f"{server_mode} :{port}" if needs_server else "no server"
            _print_banner(loop, loops, suites, server_label)
            t0      = time.monotonic()
            workers = max(1, args.workers)
            results = _run_loop(entries, args.verbose, args.parallel, workers)
            code    = _print_summary(results, time.monotonic() - t0)
            summaries.append(results)
            if code != 0:
                final_exit = 1
            if args.until_pass and code == 0:
                if loops > 1:
                    print(green("  All tests passed — stopping early."))
                break
    finally:
        if needs_server and not args.no_server:
            server.stop()

    if len(summaries) > 1:
        _print_loop_table(summaries)

    # Pause so the window stays open when launched by double-clicking.
    if sys.stdin.isatty():
        try:
            input(f"  {'Press Enter to close...'}\n")
        except (EOFError, KeyboardInterrupt):
            pass

    return final_exit


if __name__ == "__main__":
    sys.exit(main())
