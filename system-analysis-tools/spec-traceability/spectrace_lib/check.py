"""Consistency checks between the spec, the matrix and the code.

Errors (exit 1) mean the matrix is wrong or stale: a quote that is no
longer in the spec, a citation outside its file, a missing test, a broken
link, an invalid status, a spec sentence nobody accounts for, or a stale
generated summary. Warnings and notes are for a human to read.
"""
from __future__ import annotations

import os
import re
from dataclasses import dataclass, field

from . import report, rust
from .matrix import STATUSES, Matrix
from .spec import Spec

NEAR = 3                 # lines of slack around a cited range
COVERED = 0.5            # fraction of a unit's text that quotes must cover


@dataclass
class Result:
    errors: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)
    notes: list[str] = field(default_factory=list)
    stats: dict = field(default_factory=dict)

    def to_json(self) -> dict:
        return {"errors": self.errors, "warnings": self.warnings, "notes": self.notes, "stats": self.stats}


def coverage(spec: Spec, m: Matrix) -> tuple[dict[int, list[str]], dict[int, float], list[tuple[str, str, int]]]:
    """Unit index -> entry ids quoting it, unit index -> covered fraction,
    and quotes that could not be found."""
    by_unit: dict[int, list[str]] = {}
    covered = [bytearray(u.end - u.start) for u in spec.units]
    missing = []
    for e in m.clauses():
        for q, ln in e.quotes:
            span = spec.find(q)
            if span is None:
                missing.append((e.id, q, ln))
                continue
            for u in spec.units_in(span):
                by_unit.setdefault(u.index, [])
                if e.id not in by_unit[u.index]:
                    by_unit[u.index].append(e.id)
                a, b = max(span[0], u.start), min(span[1], u.end)
                for k in range(a - u.start, b - u.start):
                    covered[u.index][k] = 1
    frac = {u.index: (sum(covered[u.index]) / max(1, len(covered[u.index]))) for u in spec.units}
    return by_unit, frac, missing


def anchored(src: rust.Source, c, near: int = NEAR) -> bool | None:
    """Whether a backticked identifier of the citation's clause occurs near
    the cited lines (or names the enclosing item). The clause is the text
    between the neighbouring links; if that fails, the whole line. None:
    no identifier to check."""
    first = _anchored(src, c.idents, c, near)
    if first:
        return True
    second = _anchored(src, c.line_idents, c, near)
    if second is None:
        return first
    return second


def _anchored(src: rust.Source, idents: list[str], c, near: int) -> bool | None:
    tokens = []
    for ident in idents:
        tokens += [t for t in rust.identifier_tokens(ident) if len(t) > 1]
    if not tokens:
        return None
    lo, hi = max(1, c.start - near), min(len(src.lines), c.end + near)
    window = "\n".join(src.lines[lo - 1:hi])
    for t in tokens:
        if re.search(rf"(?<![A-Za-z0-9_]){re.escape(t)}(?![A-Za-z0-9_])", window):
            return True
    item = src.item_at(c.start) if c.path.endswith(".rs") else None
    if item and any(t in rust.identifier_tokens(item.name) for t in tokens):
        return True
    return False


def run(spec: Spec, m: Matrix, tree: rust.Tree, readme_summary: bool = True) -> Result:
    r = Result()
    # Entries ------------------------------------------------------------
    seen: dict[str, str] = {}
    for e in m.entries:
        where = f"{os.path.basename(e.md_file)}:{e.line}"
        if e.id in seen:
            r.errors.append(f"{where}: duplicate id {e.id} (first in {seen[e.id]})")
        seen.setdefault(e.id, where)
        expected = e.id.lower().replace(".", "-")
        if e.anchor != expected:
            r.errors.append(f"{where}: {e.id} has anchor #{e.anchor}, expected #{expected}")
        if e.kind == "clause":
            if not e.status:
                r.errors.append(f"{where}: {e.id} needs **Status.** starting with one of {', '.join(STATUSES)}")
            if e.status in ("Implemented", "Partial", "Diverges") and not e.citations:
                r.errors.append(f"{where}: {e.id} is {e.status} but cites no code")
            if e.status in ("Implemented", "Partial", "Diverges", "Process") and not e.tests and "Tests" not in e.sections:
                r.warnings.append(f"{where}: {e.id} has no **Tests.** section (write 'None.' if there are none)")
    # Spec coverage ------------------------------------------------------
    by_unit, frac, missing = coverage(spec, m)
    for id_, q, ln in missing:
        e = m.by_id(id_)
        r.errors.append(f"{os.path.basename(e.md_file)}:{ln}: {id_} quotes text that is not in the spec: “{q[:70]}”")
    uncovered = [u for u in spec.units if frac[u.index] < COVERED]
    for u in uncovered:
        msg = f"{os.path.basename(spec.path)}:{u.line}: no entry quotes “{u.text[:80]}”"
        (r.notes if u.kind == "code" else r.errors).append(msg)
    # Citations ----------------------------------------------------------
    unanchored = 0
    checked = 0
    for c in m.all_citations():
        where = f"{os.path.basename(c.md_file)}:{c.md_line}"
        src = tree.get(c.path)
        if src is None:
            r.errors.append(f"{where}: {c.entry} cites missing file {c.path}")
            continue
        if not (1 <= c.start <= c.end <= len(src.lines)):
            r.errors.append(f"{where}: {c.entry} cites {c.path}:{c.start}-{c.end}, outside the file ({len(src.lines)} lines)")
            continue
        if c.href_line != c.start:
            r.errors.append(f"{where}: link text says line {c.start} but the link points at #L{c.href_line}")
        if c.path.endswith(".rs"):
            a = anchored(src, c)
            if a is not None:
                checked += 1
            if a is False:
                unanchored += 1
                r.notes.append(f"{where}: {c.entry} {c.key()} unanchored: none of {sorted(set(c.idents + c.line_idents))} occurs near the cited lines")
            elif a is None:
                r.notes.append(f"{where}: {c.entry} {c.key()} has no backticked identifier to anchor on")
    # Tests --------------------------------------------------------------
    for e in m.entries:
        for f, name, ln in e.tests:
            src = tree.get(f)
            if src is None:
                r.errors.append(f"{os.path.basename(e.md_file)}:{ln}: {e.id} cites test file {f}, which does not exist")
            elif name not in src.fn_names():
                r.errors.append(f"{os.path.basename(e.md_file)}:{ln}: {e.id} cites {f}::{name}, which does not exist")
    # Links --------------------------------------------------------------
    for e in m.entries:
        for md, anchor, ln in e.links:
            base = os.path.basename(md)
            if base not in m.anchors:
                r.errors.append(f"{os.path.basename(e.md_file)}:{ln}: {e.id} links to {base}, which is not in the matrix")
            elif anchor not in m.anchors[base]:
                r.errors.append(f"{os.path.basename(e.md_file)}:{ln}: {e.id} links to {base}#{anchor}, which does not exist")
    # Generated summary --------------------------------------------------
    if readme_summary:
        readme = os.path.join(m.dir, "README.md")
        if os.path.exists(readme):
            with open(readme, encoding="utf-8") as f:
                text = f.read()
            cur = report.extract_block(text, "summary")
            if cur is None:
                r.warnings.append("README.md has no <!-- spectrace:summary --> block")
            elif cur.strip() != report.summary_md(spec, m).strip():
                r.errors.append("README.md: the summary block is stale (run `spectrace summary --write`)")
    r.stats = {
        "entries": len(m.entries), "clauses": len(m.clauses()), "spec_units": len(spec.units),
        "uncovered_units": len(uncovered), "citations": len(m.all_citations()), "anchor_checked": checked,
        "unanchored": unanchored, "tests": sum(len(e.tests) for e in m.entries),
    }
    return r
