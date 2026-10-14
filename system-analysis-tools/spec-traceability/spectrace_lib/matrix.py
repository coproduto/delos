"""The matrix side: parse a directory of Markdown files into entries.

An entry starts with `## ID · Title {#anchor}`. A *clause* entry quotes
the spec in `>` lines (one quote per line, verbatim; `…` elides) and has a
`**Status.**`. Other entries (findings, unspecified behaviour) have no
quotes. Sections start with a bold label such as `**Implemented by.**`.

Code is cited as `[graph.rs:243-264](../../src/graph.rs#L243)`: the text
names the range and the link points at its first line. Tests are cited as
`` `tests/engine.rs::cycles_are_rejected` ``.
"""
from __future__ import annotations

import os
import re
from dataclasses import dataclass, field

HEADING = re.compile(r"^##\s+([A-Z][A-Za-z]*(?:\d+(?:\.\d+)*[a-z]?|-[A-Z0-9][A-Z0-9-]*))\s+·\s+(.+?)\s*\{#([a-z0-9-]+)\}\s*$")
LABEL = re.compile(r"^\*\*([^*]+?)\.?\*\*\.?\s*(.*)$")
CITE = re.compile(r"\[([^\]\[]+?):(\d+)(?:-(\d+))?\]\(([^)#\s]+)#L(\d+)\)")
TESTREF = re.compile(r"`((?:tests|src)/[^`:\s]+\.rs)::([A-Za-z_][A-Za-z0-9_]*)`")
# Links within the matrix directory only (`../other-model/x.md#a` is not checked).
XLINK = re.compile(r"\[([^\]]+)\]\(([^)#\s/]*\.md)?#([a-z0-9-]+)\)")
TICKS = re.compile(r"`([^`]+)`")

STATUSES = ("Implemented", "Partial", "Diverges", "Missing", "Deferred", "Process")


@dataclass
class Citation:
    entry: str
    section: str
    md_file: str
    md_line: int
    path: str           # relative to the project root
    start: int
    end: int
    href_line: int
    segment: str        # the clause around the link
    idents: list[str]   # backticked identifiers in the segment
    line_idents: list[str] = field(default_factory=list)  # ... in the whole line (fallback)

    def key(self) -> str:
        return f"{self.path}:{self.start}-{self.end}"

    def to_json(self) -> dict:
        return {"entry": self.entry, "section": self.section, "path": self.path, "start": self.start, "end": self.end,
                "md": f"{os.path.basename(self.md_file)}:{self.md_line}"}


@dataclass
class Entry:
    id: str
    title: str
    anchor: str
    md_file: str
    line: int
    area: str = ""
    quotes: list[tuple[str, int]] = field(default_factory=list)   # (text, md line)
    sections: dict[str, str] = field(default_factory=dict)
    citations: list[Citation] = field(default_factory=list)
    tests: list[tuple[str, str, int]] = field(default_factory=list)  # (file, fn, md line)
    links: list[tuple[str, str, int]] = field(default_factory=list)  # (md file, anchor, md line)

    @property
    def kind(self) -> str:
        return "clause" if self.quotes else "note"

    @property
    def status(self) -> str:
        s = self.sections.get("Status", "").strip()
        for st in STATUSES:
            if s.lower().startswith(st.lower()):
                return st
        return ""

    def to_json(self) -> dict:
        return {
            "id": self.id, "title": self.title, "anchor": self.anchor, "file": os.path.basename(self.md_file), "line": self.line,
            "area": self.area, "kind": self.kind, "status": self.status, "quotes": [q for q, _ in self.quotes],
            "sections": self.sections, "citations": [c.to_json() for c in self.citations],
            "tests": [f"{f}::{n}" for f, n, _ in self.tests], "links": [f"{os.path.basename(m) if m else ''}#{a}" for m, a, _ in self.links],
        }


@dataclass
class Matrix:
    dir: str
    root: str
    entries: list[Entry] = field(default_factory=list)
    files: list[str] = field(default_factory=list)
    anchors: dict[str, set[str]] = field(default_factory=dict)   # md basename -> anchors
    loose_citations: list[Citation] = field(default_factory=list)  # outside any entry

    def clauses(self) -> list[Entry]:
        return [e for e in self.entries if e.kind == "clause"]

    def by_id(self, id_: str) -> Entry | None:
        for e in self.entries:
            if e.id == id_ or e.anchor == id_.lower():
                return e
        return None

    def all_citations(self) -> list[Citation]:
        return [c for e in self.entries for c in e.citations] + self.loose_citations


def _segments(line: str) -> list[tuple[re.Match, str]]:
    """Each citation in `line` with the text between its neighbours."""
    ms = list(CITE.finditer(line))
    out = []
    for i, m in enumerate(ms):
        a = ms[i - 1].end() if i else 0
        b = ms[i + 1].start() if i + 1 < len(ms) else len(line)
        out.append((m, line[a:b]))
    return out


def parse_file(path: str, root: str, m: Matrix):
    md_dir = os.path.dirname(os.path.abspath(path))
    with open(path, encoding="utf-8") as f:
        lines = f.read().splitlines()
    area = ""
    entry: Entry | None = None
    section = ""
    anchors = m.anchors.setdefault(os.path.basename(path), set())
    in_code = False
    for n, line in enumerate(lines, 1):
        if line.strip().startswith("```"):
            in_code = not in_code
            continue
        if in_code:
            continue
        for a in re.findall(r"\{#([a-z0-9-]+)\}", line):
            anchors.add(a)
        if line.startswith("# "):
            area = line[2:].strip()
            continue
        h = HEADING.match(line)
        if h:
            entry = Entry(h.group(1), h.group(2), h.group(3), path, n, area)
            m.entries.append(entry)
            section = ""
            continue
        if line.startswith("## ") or line.startswith("### "):
            # A heading that is not an entry ends the current entry.
            if line.startswith("## "):
                entry = None
            section = ""
            continue
        lab = LABEL.match(line)
        if lab and entry is not None:
            section = lab.group(1).strip()
            entry.sections[section] = lab.group(2).strip()
        elif entry is not None and section and line.strip():
            entry.sections[section] = (entry.sections[section] + "\n" + line).strip()
        if entry is not None and line.startswith(">") and not section:
            q = line[1:].strip()
            if q:
                entry.quotes.append((q, n))
            continue
        line_idents = [t for t in TICKS.findall(CITE.sub("", line)) if not t.startswith(("tests/", "src/"))]
        for cm, seg in _segments(line):
            href = cm.group(4)
            target = os.path.normpath(os.path.relpath(os.path.join(md_dir, href), root))
            start = int(cm.group(2))
            end = int(cm.group(3) or start)
            idents = [t for t in TICKS.findall(seg) if not t.startswith(("tests/", "src/"))]
            c = Citation(entry.id if entry else "", section, path, n, target, start, end, int(cm.group(5)), seg, idents, line_idents)
            (entry.citations if entry else m.loose_citations).append(c)
        if entry is not None:
            for tm in TESTREF.finditer(line):
                entry.tests.append((tm.group(1), tm.group(2), n))
            for xm in XLINK.finditer(line):
                entry.links.append((os.path.join(md_dir, xm.group(2)) if xm.group(2) else path, xm.group(3), n))


def load(dir_: str, root: str) -> Matrix:
    m = Matrix(dir_, root)
    for f in sorted(os.listdir(dir_)):
        if f.endswith(".md"):
            p = os.path.join(dir_, f)
            m.files.append(p)
            parse_file(p, root, m)
    return m
