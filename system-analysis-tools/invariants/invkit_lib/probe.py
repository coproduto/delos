"""Run a catalogue's probe tests against the code.

A probe asserts an invariant the catalogue says is violated, so the expected
outcome is FAIL: a failing probe *confirms* the violation. A passing probe
means the violation is gone (fixed, or the probe is wrong), and the
catalogue should be updated.

Cargo only builds integration tests that live in `tests/`, so each probe file
is copied to `tests/zz_invkit_<name>.rs` for the run and always removed
afterwards. Nothing else in the project is touched.
"""
from __future__ import annotations

import os
import re
import shutil
import subprocess

PREFIX = "zz_invkit_"


def parse_results(output: str) -> dict[str, str]:
    """`test name ... ok|FAILED|ignored` lines → {name: ok|FAILED|ignored}"""
    res = {}
    for m in re.finditer(r"^test\s+(\S+)\s+\.\.\.\s+(ok|FAILED|ignored)\b", output, re.M):
        res[m.group(1).split("::")[-1]] = m.group(2)
    return res


def panic_messages(output: str) -> dict[str, str]:
    msgs = {}
    for m in re.finditer(r"---- (\S+) stdout ----\n(.*?)(?=\n---- |\nfailures:|\Z)", output, re.S):
        body = m.group(2)
        pm = re.search(r"panicked at [^\n]*:\n(.*?)(?:\nnote:|\n\n|\Z)", body, re.S)
        msgs[m.group(1).split("::")[-1]] = (pm.group(1) if pm else body).strip()[:400]
    return msgs


def run(cat, cargo_args=(), timeout=1800, keep=False, log=print):
    root = cat.root
    tests_dir = os.path.join(root, "tests")
    if not os.path.isfile(os.path.join(root, "Cargo.toml")):
        raise SystemExit(f"invkit: error: {root} has no Cargo.toml: probes are Rust integration tests")
    if not cat.probes:
        return {"probes": [], "summary": "no probes"}
    expected_fail = {p for f in cat.findings for p in f.probes}
    results = []
    for rel, names in cat.probes.items():
        stem = os.path.splitext(os.path.basename(rel))[0]
        target = PREFIX + re.sub(r"\W", "_", stem)
        dest = os.path.join(tests_dir, target + ".rs")
        if os.path.exists(dest):
            raise SystemExit(f"invkit: error: {dest} already exists; remove it (left from an interrupted run?) and retry")
        shutil.copyfile(os.path.join(cat.dir, rel), dest)
        try:
            cmd = ["cargo", "test", *cargo_args, "--test", target, "--", "--test-threads=1"]
            log(f"$ {' '.join(cmd)}")
            p = subprocess.run(cmd, cwd=root, capture_output=True, text=True, timeout=timeout)
            out = p.stdout + "\n" + p.stderr
        finally:
            if not keep:
                os.remove(dest)
        got = parse_results(out)
        msgs = panic_messages(out)
        if not got:
            tail = "\n".join(out.strip().split("\n")[-25:])
            results.append({"file": rel, "name": None, "outcome": "build-failed", "verdict": "error", "detail": tail})
            continue
        for n in names:
            outcome = got.get(n, "missing")
            exp = "FAILED" if n in expected_fail else "ok"
            if outcome == "missing":
                verdict = "error"
            elif n in expected_fail:
                verdict = "confirmed" if outcome == "FAILED" else "no-longer-violated"
            else:
                verdict = "holds" if outcome == "ok" else "unexpected-failure"
            results.append({"file": rel, "name": n, "outcome": outcome, "expected": exp,
                            "verdict": verdict, "detail": msgs.get(n, "")})
    bad = [r for r in results if r["verdict"] not in ("confirmed", "holds")]
    return {"probes": results, "ok": not bad}


def format_text(rep) -> str:
    out = []
    for r in rep.get("probes", []):
        line = f"{r['verdict']:20} {r.get('name') or r['file']}  ({r['outcome']}"
        line += f", expected {r['expected']})" if "expected" in r else ")"
        out.append(line)
        if r.get("detail") and r["verdict"] != "holds":
            out += ["    " + l for l in r["detail"].split("\n")[:6]]
    if "ok" in rep:
        out.append("all probes behave as the catalogue says" if rep["ok"] else
                   "some probes disagree with the catalogue: update findings.md or the probe")
    return "\n".join(out)
