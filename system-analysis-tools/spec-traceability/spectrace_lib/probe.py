"""Probes: Rust integration tests kept in `<matrix>/probes/` that assert
what a spec clause says. A probe that FAILS confirms a finding.

Each probe file is copied to `tests/zz_spectrace_<stem>.rs` for the run
and deleted afterwards, so `tests/` is unchanged. Findings name their
probe with ``**Confirmed.** Probe `test_name` ``.
"""
from __future__ import annotations

import os
import re
import shutil
import subprocess

from .matrix import Matrix

RESULT = re.compile(r"^test (\S+) \.\.\. (ok|FAILED|ignored)", re.M)
PREFIX = "zz_spectrace_"


def probe_tests(path: str) -> list[str]:
    text = open(path, encoding="utf-8").read()
    return re.findall(r"#\[(?:tokio::)?test[^\]]*\]\s*(?:#\[[^\]]*\]\s*)*(?:async\s+)?fn\s+(\w+)", text)


def claimed(m: Matrix) -> dict[str, str]:
    """Probe test name -> entry id that cites it as confirmation."""
    out = {}
    for e in m.entries:
        for k, v in e.sections.items():
            if k.lower().startswith("kind") or k.lower().startswith("confirmed"):
                for name in re.findall(r"[Pp]robe `(\w+)`", v):
                    out[name] = e.id
    return out


def run(root: str, m: Matrix, cargo_args: list[str], timeout: int = 1800) -> dict:
    pdir = os.path.join(m.dir, "probes")
    files = sorted(f for f in os.listdir(pdir) if f.endswith(".rs")) if os.path.isdir(pdir) else []
    stale = [f for f in os.listdir(os.path.join(root, "tests")) if f.startswith(PREFIX)]
    if stale:
        raise SystemExit(f"tests/ already holds {stale}: a previous run was killed. Delete them and retry.")
    claims = claimed(m)
    results = {}
    for f in files:
        stem = os.path.splitext(f)[0]
        name = PREFIX + stem
        dest = os.path.join(root, "tests", name + ".rs")
        shutil.copy(os.path.join(m.dir, "probes", f), dest)
        try:
            p = subprocess.run(["cargo", "test", *cargo_args, "--test", name, "--", "--test-threads=1"],
                               cwd=root, capture_output=True, text=True, timeout=timeout)
            out = p.stdout + p.stderr
        except subprocess.TimeoutExpired:
            raise SystemExit(f"probe {f}: cargo test timed out after {timeout} s (raise --timeout)") from None
        finally:
            os.remove(dest)
        found = dict(RESULT.findall(out))
        if not found and p.returncode != 0:
            results[stem] = {"error": out[-3000:]}
            continue
        for test in probe_tests(os.path.join(m.dir, "probes", f)):
            state = found.get(test, "missing")
            entry = claims.get(test)
            verdict = {"FAILED": "confirms", "ok": "no-longer-violated", "ignored": "ignored"}.get(state, "did-not-run")
            msg = ""
            if state == "FAILED":
                mm = re.search(rf"---- {test} stdout ----\n(.*?)(?:\n\n|\Z)", out, re.S)
                msg = mm.group(1).strip()[-600:] if mm else ""
            results[test] = {"file": f, "verdict": verdict, "entry": entry, "message": msg}
    return results
