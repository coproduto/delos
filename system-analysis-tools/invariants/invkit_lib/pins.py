"""Pin the code a catalogue cites, and detect when it moves or changes.

`pin` records the exact text of every cited line range. `verify` compares the
pins with the current code:

  same     the cited lines are unchanged
  moved    the same text exists elsewhere in the file: the citation needs a
           new line number (suggested)
  changed  the text is gone: re-read the code and re-check the invariant
  missing  the file is gone

Unlike the heuristic anchoring in `check`, this is exact, so it is the tool
for keeping a catalogue current as the code changes.
"""
from __future__ import annotations

import hashlib
import os
import subprocess


def _norm(lines):
    return [l.strip() for l in lines]


def _read(root, rel):
    try:
        with open(os.path.join(root, rel), encoding="utf-8") as f:
            return f.read().split("\n")
    except (OSError, UnicodeDecodeError):
        return None


def pin(cat) -> dict:
    rows = []
    cits = [(i.id, c) for i in cat.invariants for c in i.citations] + [("", c) for c in cat.other_citations]
    for owner, c in cits:
        if c.start is None:
            continue
        lines = _read(cat.root, c.file)
        if lines is None:
            continue
        block = lines[c.start - 1 : (c.end or c.start)]
        rows.append({
            "invariant": owner, "md": c.md, "md_line": c.md_line, "cite": c.text, "file": c.file,
            "start": c.start, "end": c.end or c.start, "lines": block,
            "sha": hashlib.sha1("\n".join(_norm(block)).encode()).hexdigest()[:12],
        })
    try:
        commit = subprocess.run(["git", "-C", cat.root, "rev-parse", "--short", "HEAD"],
                                capture_output=True, text=True, check=True).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        commit = None
    return {"format": "invkit.pins/1", "commit": commit, "pins": rows}


def verify(root: str, pins: dict) -> list[dict]:
    out = []
    cache: dict[str, list[str] | None] = {}
    for p in pins["pins"]:
        if p["file"] not in cache:
            cache[p["file"]] = _read(root, p["file"])
        lines = cache[p["file"]]
        rec = {k: p[k] for k in ("invariant", "md", "md_line", "cite", "file", "start", "end")}
        if lines is None:
            out.append({**rec, "state": "missing"})
            continue
        want = _norm(p["lines"])
        n = len(want)
        now = _norm(lines[p["start"] - 1 : p["start"] - 1 + n])
        if now == want:
            out.append({**rec, "state": "same"})
            continue
        norm = _norm(lines)
        hits = [k + 1 for k in range(len(norm) - n + 1) if norm[k : k + n] == want]
        if hits:
            best = min(hits, key=lambda h: abs(h - p["start"]))
            new = f"{best}" if n == 1 else f"{best}-{best + n - 1}"
            out.append({**rec, "state": "moved", "now": new, "candidates": hits[:5]})
        else:
            out.append({**rec, "state": "changed"})
    return out


def format_text(rows) -> str:
    from collections import Counter

    cnt = Counter(r["state"] for r in rows)
    out = [", ".join(f"{cnt.get(k, 0)} {k}" for k in ("same", "moved", "changed", "missing"))]
    for r in rows:
        if r["state"] == "same":
            continue
        tail = f" → now at {r['now']}" if r["state"] == "moved" else ""
        out.append(f"  {r['state']:8} {r['invariant'] or '-':4} {r['md']}:{r['md_line']} {r['cite']}{tail}")
    return "\n".join(out)


def changed_ranges(root: str, rev: str, paths=("src", "tests")) -> dict[str, list[tuple[int, int]]]:
    """Old-side line ranges touched by `git diff REV` (working tree included)."""
    import re

    out = subprocess.run(["git", "-C", root, "diff", "-U0", rev, "--", *paths],
                         capture_output=True, text=True, check=True).stdout
    ranges: dict[str, list[tuple[int, int]]] = {}
    cur = None
    for line in out.split("\n"):
        if line.startswith("--- "):
            cur = line[6:] if line.startswith("--- a/") else None
        m = re.match(r"@@ -(\d+)(?:,(\d+))? \+\d+(?:,\d+)? @@", line)
        if m and cur:
            a, n = int(m.group(1)), int(m.group(2) if m.group(2) is not None else 1)
            # a pure insertion (n == 0) sits after line a
            ranges.setdefault(cur, []).append((a, a + max(n, 1) - 1) if n else (a, a + 1))
    return ranges


def impact(cat, rev: str, slack: int = 0) -> list[dict]:
    """Invariants whose cited lines (in the analysed version) a diff touches."""
    ranges = changed_ranges(cat.root, rev)
    rows = []
    for inv in cat.invariants:
        hits = []
        for c in inv.citations:
            if c.start is None:
                if c.file in ranges:
                    hits.append({"cite": c.text, "section": c.section, "whole_file": True})
                continue
            for a, b in ranges.get(c.file, []):
                if a <= (c.end or c.start) + slack and b >= c.start - slack:
                    hits.append({"cite": c.text, "section": c.section, "hunk": f"{a}-{b}"})
                    break
        if hits:
            rows.append({"id": inv.id, "title": inv.title, "status": inv.status_class, "md": inv.md, "hits": hits})
    return rows
