"""Drift detection. `pin` records the exact text behind every citation
and every quoted spec unit at the analysed commit; `verify` reports what
has moved or changed since. `impact` maps a git diff onto the matrix.
"""
from __future__ import annotations

import hashlib
import os
import re
import subprocess

from . import rust, spec as specmod
from .check import coverage
from .matrix import Matrix


def _digest(lines: list[str]) -> str:
    return hashlib.sha1("\n".join(l.rstrip() for l in lines).encode()).hexdigest()[:16]


def pin(spec: specmod.Spec, m: Matrix, tree: rust.Tree, commit: str | None) -> dict:
    cites = {}
    for c in m.all_citations():
        src = tree.get(c.path)
        if src is None or c.end > len(src.lines):
            continue
        body = src.lines[c.start - 1:c.end]
        cites[c.key()] = {"path": c.path, "start": c.start, "end": c.end, "digest": _digest(body), "text": body, "entries": sorted({x.entry for x in m.all_citations() if x.key() == c.key()})}
    by_unit, _, _ = coverage(spec, m)
    units = {str(u.index): {"line": u.line, "digest": u.digest, "text": u.text, "entries": by_unit.get(u.index, [])} for u in spec.units}
    return {"commit": commit, "spec": os.path.basename(spec.path), "citations": cites, "spec_units": units}


def verify(pins: dict, spec: specmod.Spec, tree: rust.Tree) -> dict:
    """Per citation: same / moved (with new lines) / changed / missing.
    Per pinned spec unit: same / changed / removed; plus new units."""
    out = {"citations": {}, "spec": {"changed": [], "removed": [], "added": []}}
    for key, p in pins["citations"].items():
        src = tree.get(p["path"])
        if src is None:
            out["citations"][key] = {"state": "missing", **_meta(p)}
            continue
        n = p["end"] - p["start"] + 1
        here = src.lines[p["start"] - 1:p["end"]]
        if _digest(here) == p["digest"]:
            out["citations"][key] = {"state": "same", **_meta(p)}
            continue
        found = None
        for s in range(0, max(0, len(src.lines) - n + 1)):
            if _digest(src.lines[s:s + n]) == p["digest"]:
                found = (s + 1, s + n)
                break
        if found:
            out["citations"][key] = {"state": "moved", "to": f"{found[0]}-{found[1]}", **_meta(p)}
        else:
            out["citations"][key] = {"state": "changed", **_meta(p)}
    now = {u.digest: u for u in spec.units}
    then = {p["digest"]: p for p in pins["spec_units"].values()}
    for d, p in then.items():
        if d not in now:
            # Same line still there with other text: changed; otherwise removed.
            out["spec"]["changed" if spec.unit_at_line(p["line"]) else "removed"].append({"line": p["line"], "text": p["text"], "entries": p["entries"]})
    for d, u in now.items():
        if d not in then:
            out["spec"]["added"].append({"line": u.line, "text": u.text})
    return out


def _meta(p: dict) -> dict:
    return {"entries": p["entries"], "path": p["path"], "lines": f"{p['start']}-{p['end']}"}


def commit_of(matrix_dir: str) -> str | None:
    readme = os.path.join(matrix_dir, "README.md")
    if not os.path.exists(readme):
        return None
    with open(readme, encoding="utf-8") as f:
        m = re.search(r"(?i:commit) `([0-9a-f]{6,40})`", f.read())
    return m.group(1) if m else None


def _git(root: str, *args: str) -> str:
    """stdout of `git -C root ARGS`; a failure exits with git's message."""
    p = subprocess.run(["git", "-C", root, *args], capture_output=True, text=True)
    if p.returncode != 0:
        msg = (p.stderr.strip().splitlines() or [f"exit status {p.returncode}"])[0]
        raise SystemExit(f"git {args[0]} failed: {msg}")
    return p.stdout


def _parse_diff(out: str) -> tuple[dict[str, list[tuple[int, int, int, int]]], set[str]]:
    """Hunks of a `git diff -U0` per old-side path, as (old start, old count,
    new start, new count); and the old-side paths the diff deletes."""
    hunks: dict[str, list[tuple[int, int, int, int]]] = {}
    deleted: set[str] = set()
    cur = None
    for line in out.splitlines():
        if line.startswith("--- "):
            cur = line[6:] if line.startswith("--- a/") else None
        elif line.startswith("+++ ") and cur and line[4:] == "/dev/null":
            deleted.add(cur)
        m = re.match(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@", line)
        if m and cur:
            g = [int(x) if x is not None else 1 for x in m.groups()]
            hunks.setdefault(cur, []).append((g[0], g[1], g[2], g[3]))
    return hunks, deleted


def diff_hunks(root: str, rev: str, paths: list[str]) -> dict[str, list[tuple[int, int]]]:
    """Old-side line ranges changed since `rev`, per file (working tree).
    A pure insertion maps to the line before it, or to line 1 at the top."""
    hunks, _ = _parse_diff(_git(root, "diff", "-U0", rev, "--", *paths))
    return {path: [(a, a + n - 1) if n else (max(a, 1), max(a, 1)) for a, n, _, _ in hs] for path, hs in hunks.items()}


def _map_line(hunks: list[tuple[int, int, int, int]], line: int) -> tuple[int, int]:
    """Where old `line` is on the new side of `hunks` (sorted, from one file).
    Unchanged lines shift by the hunks before them; a changed line widens to
    its hunk's new lines; a deleted line becomes the line before the gap (or
    line 1 at the top), the same convention as `diff_hunks`."""
    delta = 0
    for a, n, b, nb in hunks:
        if n == 0:
            if line <= a:  # inserted after old line a: earlier lines are untouched
                break
        else:
            if line < a:
                break
            if line <= a + n - 1:
                return (b, b + nb - 1) if nb else (max(b, 1), max(b, 1))
        delta += nb - n
    return (line + delta, line + delta)


def map_range(hunks: list[tuple[int, int, int, int]], start: int, end: int) -> tuple[int, int]:
    """Old lines `start`-`end` carried to the new side of `hunks`."""
    return _map_line(hunks, start)[0], _map_line(hunks, end)[1]


def _same_commit(root: str, a: str, b: str) -> bool:
    return _git(root, "rev-parse", "--verify", f"{a}^{{commit}}").strip() == _git(root, "rev-parse", "--verify", f"{b}^{{commit}}").strip()


def impact(root: str, rev: str, spec_path: str, m: Matrix, slack: int = 0, base: str | None = None) -> dict:
    """Entries whose cited lines, or whose quoted spec text, a diff touches.

    Citations give line numbers at `base`, the analysed commit (default:
    `rev`). When `rev` is another commit, each citation is first carried
    through the `base`→`rev` diff, so it is compared with the `rev`→working
    tree diff in `rev`'s numbering. Citations of files that `rev` no longer
    has are listed under `unmapped`."""
    spec_rel = os.path.relpath(spec_path, root)
    cited = {c.path for c in m.all_citations() if not c.path.startswith("..")}
    hunks = diff_hunks(root, rev, sorted({"src", "tests", spec_rel} | cited))
    carry = base is not None and not _same_commit(root, base, rev)
    moves: dict[str, list[tuple[int, int, int, int]]] = {}
    gone: set[str] = set()
    if carry and cited:
        moves, gone = _parse_diff(_git(root, "diff", "-U0", "--no-renames", base, rev, "--", *sorted(cited)))
    code: dict[str, list[str]] = {}
    unmapped: dict[str, list[str]] = {}
    for c in m.all_citations():
        if c.path in gone:
            unmapped.setdefault(c.entry or "(loose)", []).append(c.key())
            continue
        lo, hi = map_range(moves.get(c.path, []), c.start, c.end) if carry else (c.start, c.end)
        at = f" → {lo}-{hi}" if (lo, hi) != (c.start, c.end) else ""
        for a, b in hunks.get(c.path, []):
            if a <= hi + slack and lo - slack <= b:
                code.setdefault(c.entry or "(loose)", []).append(f"{c.key()}{at} ∩ {a}-{b}")
    spec_hits: dict[str, list[str]] = {}
    if spec_rel in hunks:
        old_text = _git(root, "show", f"{rev}:{spec_rel}")
        old = specmod.parse(old_text, spec_path)
        by_unit, _, _ = coverage(old, m)
        for a, b in hunks[spec_rel]:
            for u in old.units:
                if u.line <= b and a <= u.end_line:
                    for e in by_unit.get(u.index, ["(unclaimed)"]):
                        spec_hits.setdefault(e, []).append(f"{spec_rel}:{u.line} “{u.text[:60]}”")
    uncited = sorted(f for f in hunks if f != spec_rel and not any(c.path == f for c in m.all_citations()))
    return {"rev": rev, "base": base or rev, "carried": carry, "code": code, "spec": spec_hits, "unmapped": unmapped, "files_without_citations": uncited}
