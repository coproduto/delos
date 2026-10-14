"""Parse an invariant catalogue written in the invkit Markdown format.

A catalogue is a directory:

    README.md            overview, counts, root graph, enforcement table
    NN-<area>.md         one file per area; one `## ID · title {#anchor}` per invariant
    findings.md          `## Finding N · title {#finding-N}` per violation, plus a fragile table
    probes/*.rs          tests asserting violated invariants (expected to FAIL)

Inside an invariant, sections start with a bold label: `**Statement.**`,
`**Maintained by**`, `**Status: VIOLATED.**` and so on. Code is cited with
links such as `[graph.rs:103-107](../../src/graph.rs#L103)`. Tests are cited in
backticks: `tests/engine.rs::cycles_are_rejected`.
"""
from __future__ import annotations

import os
import re
from dataclasses import asdict, dataclass, field

HEADING = re.compile(r"^##\s+([A-Z]+\d+)\s+·\s+(.*?)\s*\{#([\w-]+)\}\s*$")
FINDING = re.compile(r"^##\s+Finding\s+(\d+)\s+·\s+(.*?)\s*\{#([\w-]+)\}\s*$")
LABEL = re.compile(r"^(?:[-*]\s+)?\*\*([^*]+?)\*\*\s*(.*)$")
LINK = re.compile(r"\[([^\[\]]+)\]\(([^)\s]+)\)")
CODE_SPAN = re.compile(r"`([^`]+)`")
TEST_REF = re.compile(r"^((?:tests|src)/[\w/]+\.rs)(?:::(\w+))?$")

# Canonical section names. Labels are matched by prefix, case-insensitively.
SECTIONS = [
    ("statement", ("statement",)),
    ("maintained", ("maintained by", "sub-invariants maintained by")),
    ("depended", ("depended on by",)),
    ("depends", ("depends on",)),
    ("tests", ("tests",)),
    ("broken", ("if broken",)),
    ("status", ("status",)),
]
REQUIRED = ("statement", "status")
EXPECTED = ("maintained", "depended", "tests", "broken")


def canonical(label: str) -> str:
    low = label.lower().rstrip(".").strip()
    for name, prefixes in SECTIONS:
        if any(low == p or low.startswith(p + " ") or low.startswith(p + ":") or low.startswith(p + ".") for p in prefixes):
            return name
    return "note:" + re.sub(r"\W+", "-", low).strip("-")


@dataclass
class Citation:
    file: str  # repo-relative
    start: int | None  # None for a whole-file link
    end: int | None
    text: str  # link text
    href: str
    md: str  # catalogue file
    md_line: int
    section: str
    context: str  # the bullet or sentence the link sits in
    names: list[str] = field(default_factory=list)  # code identifiers in the clause around the link
    local: str = ""  # that clause: text between the previous link and the next one
    block_names: list[str] = field(default_factory=list)  # identifiers anywhere in the bullet


@dataclass
class TestRef:
    file: str
    name: str | None
    md: str
    md_line: int


@dataclass
class XRef:
    target: str  # anchor, e.g. "i1" or "finding-1"
    md_target: str  # catalogue file the anchor should be in ("" = same file)
    text: str
    md: str
    md_line: int
    href: str = ""


@dataclass
class Invariant:
    id: str
    title: str
    anchor: str
    area: str
    md: str
    line: int
    sections: dict[str, str] = field(default_factory=dict)
    status: str = ""
    status_label: str = ""
    status_class: str = ""
    citations: list[Citation] = field(default_factory=list)
    tests: list[TestRef] = field(default_factory=list)
    xrefs: list[XRef] = field(default_factory=list)


@dataclass
class Finding:
    number: int
    title: str
    anchor: str
    line: int
    invariants: list[str]
    probes: list[str]
    sections: dict[str, str] = field(default_factory=dict)


@dataclass
class Catalogue:
    dir: str
    root: str
    invariants: list[Invariant]
    findings: list[Finding]
    fragile: list[dict]
    readme: str
    anchors: dict[str, set[str]]  # md file -> anchors defined in it
    probes: dict[str, list[str]]  # probe file -> test names
    other_citations: list[Citation]  # citations outside any invariant (README, findings)
    other_xrefs: list[XRef]

    def by_id(self) -> dict[str, Invariant]:
        return {i.id: i for i in self.invariants}

    def to_json(self) -> dict:
        def inv(i: Invariant):
            d = asdict(i)
            return d

        return {
            "format": "invkit.catalogue/1",
            "dir": self.dir,
            "counts": {
                "invariants": len(self.invariants),
                "areas": len({i.area for i in self.invariants}),
                "by_status": _count(i.status_class for i in self.invariants),
                "citations": sum(len(i.citations) for i in self.invariants),
            },
            "invariants": [inv(i) for i in self.invariants],
            "findings": [asdict(f) for f in self.findings],
            "fragile": self.fragile,
            "probes": self.probes,
        }


def _count(xs):
    out: dict[str, int] = {}
    for x in xs:
        out[x] = out.get(x, 0) + 1
    return out


def classify_status(label: str, text: str) -> str:
    """holds | qualified | violated | inherited.

    `qualified` means the author wrote a condition into the status ("holds
    for PNG", "holds, but only by discipline"): the invariant is true today
    within stated limits. That is the catalogue's notion of fragile.
    """
    if re.search(r"\bVIOLATED\b", f"{label} {text}"):
        return "violated"
    if re.match(r"Inherits\b.*\bviolation", text):
        return "inherited"
    first = re.split(r"(?<=\.)\s", text.strip(), maxsplit=1)[0]
    if re.fullmatch(r"Holds\.?", first.strip()) and not re.search(
        r"\b(only|until|must|not checked|is not|violat\w*)\b", text, re.I
    ):
        return "holds"
    return "qualified"


def _resolve(md_dir: str, href: str, root: str) -> str:
    path = href.split("#", 1)[0]
    full = os.path.normpath(os.path.join(md_dir, path))
    return os.path.relpath(full, root)


def _line_range(text: str, href: str):
    m = re.search(r":(\d+)(?:-(\d+))?\s*$", text)
    if m:
        a = int(m.group(1))
        return a, int(m.group(2) or a)
    m = re.search(r"#L(\d+)(?:-L?(\d+))?$", href)
    if m:
        a = int(m.group(1))
        return a, int(m.group(2) or a)
    return None, None


def _names(context: str) -> list[str]:
    """Code identifiers in backticks: `remove_node`, `Graph::new`, `inputs[0]`."""
    out = []
    for span in CODE_SPAN.findall(context):
        for tok in re.findall(r"[A-Za-z_][\w]*(?:::[A-Za-z_]\w*)*", span):
            if len(tok) >= 2 and tok not in out:
                out.append(tok)
    return out


def _local(block: str, link: str, cursor: dict) -> str:
    """The clause a link belongs to: from the end of the previous link (or a
    sentence break) to the start of the next one."""
    at = block.find(link, cursor.get(block, 0))
    if at < 0:
        at = block.find(link)
    if at < 0:
        return block
    cursor[block] = at + len(link)
    link_end = at + len(link)
    spans = [(m.start(), m.end()) for m in LINK.finditer(block)]
    prev_end = max([e for s, e in spans if e <= at] + [0])
    next_start = min([s for s, e in spans if s >= link_end] + [len(block)])
    # adjacent links ("([a], [b])") share the clause in front of the first one
    k = len(spans) - 1
    while re.fullmatch(r"[\s,;(]*(?:and\s*)?", block[prev_end:at]) and prev_end > 0:
        earlier = [(s, e) for s, e in spans if e <= prev_end and s < prev_end]
        if not earlier:
            break
        s0 = max(s for s, e in earlier)
        at = s0
        prev_end = max([e for s, e in spans if e <= s0] + [0])
    before = block[prev_end:at]
    # do not reach back past the end of an earlier sentence
    cut = max(before.rfind(". "), before.rfind("; "), before.rfind(": "))
    if cut > 0 and len(before) - cut > 12:
        before = before[cut + 2:]
    after = block[link_end:next_start]
    cut = min([i for i in (after.find(". "), after.find("; ")) if i >= 0] + [len(after)])
    return (before + " " + after[:cut]).strip()


def _contexts(lines: list[str]):
    """Yield (line_no, context) where context is the bullet or paragraph
    containing each line (so a citation can be matched with the names
    mentioned next to it)."""
    blocks: list[tuple[int, int]] = []
    start = None
    for k, l in enumerate(lines):
        s = l.strip()
        is_break = s == "" or s.startswith("#") or s == "---" or s.startswith("|")
        is_new = bool(re.match(r"^\s*(?:[-*]|\d+\.)\s", l)) or bool(LABEL.match(s))
        if is_break:
            if start is not None:
                blocks.append((start, k))
                start = None
            if s.startswith("|"):
                blocks.append((k, k + 1))
            continue
        if is_new and start is not None:
            blocks.append((start, k))
            start = None
        if start is None:
            start = k
    if start is not None:
        blocks.append((start, len(lines)))
    ctx = [""] * len(lines)
    for a, b in blocks:
        text = " ".join(x.strip() for x in lines[a:b])
        for k in range(a, b):
            ctx[k] = text
    return ctx


def parse_file(path: str, root: str, cat_dir: str):
    md = os.path.relpath(path, cat_dir)
    md_dir = os.path.dirname(os.path.abspath(path))
    with open(path, encoding="utf-8") as f:
        lines = f.read().split("\n")
    ctx = _contexts(lines)
    cursor: dict[str, int] = {}  # block text -> search position, to find each link once
    area = ""
    invariants: list[Invariant] = []
    findings: list[Finding] = []
    anchors: set[str] = set()
    other_cites: list[Citation] = []
    other_x: list[XRef] = []
    cur: Invariant | Finding | None = None
    section = "intro"
    in_code = False
    tests_file = None
    for k, raw in enumerate(lines):
        ln = k + 1
        if raw.strip().startswith("```"):
            in_code = not in_code
            continue
        if in_code:
            continue
        if raw.startswith("# ") and not area:
            area = re.sub(r"^#\s+(\d+\s+·\s+)?", "", raw).strip()
        for a in re.findall(r"\{#([\w-]+)\}", raw):
            anchors.add(a)
        m = HEADING.match(raw)
        fm = FINDING.match(raw)
        if m:
            cur = Invariant(m.group(1), m.group(2), m.group(3), area, md, ln)
            invariants.append(cur)
            section = "title"
            continue
        if fm:
            cur = Finding(int(fm.group(1)), fm.group(2), fm.group(3), ln, [], [])
            findings.append(cur)
            section = "title"
            continue
        if raw.startswith("## ") or raw.strip() == "---":
            cur = None if raw.startswith("## ") else cur
            if raw.startswith("## "):
                section = re.sub(r"\W+", "-", raw[3:].lower()).strip("-")
            continue
        lm = LABEL.match(raw.strip())
        # a bullet that starts with bold text ("- **Export.** …") stays in its section
        is_label = bool(lm and cur is not None and not raw.startswith(" ") and not re.match(r"[-*]\s", raw))
        if is_label:
            label = lm.group(1).strip()
            section = canonical(label) if isinstance(cur, Invariant) else re.sub(r"\W+", "-", label.lower()).strip("-")
            body = lm.group(2)
            tests_file = None
            cur.sections[section] = (cur.sections.get(section, "") + " " + body).strip()
            if isinstance(cur, Invariant) and section == "status":
                cur.status_label = label.rstrip(".")
                cur.status = body.strip()
        elif cur is not None and section != "title" and raw.strip():
            cur.sections[section] = (cur.sections.get(section, "") + " " + raw.strip()).strip()
            if isinstance(cur, Invariant) and section == "status":
                cur.status = (cur.status + " " + raw.strip()).strip()

        # links
        for text, href in LINK.findall(raw):
            if href.startswith("http"):
                continue
            target = href.split("#", 1)
            if target[0].endswith(".md") and target[0] and not href.startswith("../") or (href.startswith("#")):
                x = XRef(target[1] if len(target) > 1 else "", target[0], text, md, ln, href)
                (cur.xrefs if isinstance(cur, Invariant) else other_x).append(x)
                continue
            if href.startswith("../") and os.path.splitext(target[0])[1] in (".rs", ".md", ".toml", ".wgsl"):
                file = _resolve(md_dir, href, root)
                a, b = _line_range(text, href)
                local = _local(ctx[k], f"[{text}]({href})", cursor)
                c = Citation(file, a, b, text, href, md, ln, section, ctx[k], _names(local), local, _names(ctx[k]))
                (cur.citations if isinstance(cur, Invariant) else other_cites).append(c)
            elif target[0].endswith(".rs"):  # e.g. probes/invariant_probes.rs
                pass

        # test references
        if isinstance(cur, Invariant):
            for span in CODE_SPAN.findall(raw):
                tm = TEST_REF.match(span.strip())
                if tm and (section == "tests" or tm.group(2)):
                    if tm.group(1).startswith("tests/") or tm.group(2):
                        cur.tests.append(TestRef(tm.group(1), tm.group(2), md, ln))
                        tests_file = tm.group(1)
                elif section == "tests" and tests_file and re.fullmatch(r"[a-z_][a-z0-9_]*", span) and "_" in span:
                    cur.tests.append(TestRef(tests_file, span, md, ln))
        if isinstance(cur, Finding):
            if section == "invariant":
                cur.invariants += [t for t, h in LINK.findall(raw) if re.fullmatch(r"[A-Z]+\d+", t)]
            if section == "confirmed":
                cur.probes += re.findall(r"[Pp]robes?\s+`(\w+)`", raw)
    for inv in invariants:
        inv.status_class = classify_status(inv.status_label, inv.status)
    return area, invariants, findings, anchors, other_cites, other_x, lines


def _fragile_table(lines: list[str]) -> list[dict]:
    rows = []
    for l in lines:
        m = re.match(r"^\|\s*(\d+)\s*\|(.*)\|\s*$", l)
        if not m:
            continue
        cells = [c.strip() for c in m.group(2).split("|")]
        ids = [t for t, h in LINK.findall(cells[0])] + re.findall(r"`([A-Z_]{3,})`", cells[0])
        rows.append({"n": int(m.group(1)), "invariants": ids, "cells": cells})
    return rows


def load(cat_dir: str, root: str) -> Catalogue:
    cat_dir = os.path.abspath(cat_dir)
    root = os.path.abspath(root)
    invariants: list[Invariant] = []
    findings: list[Finding] = []
    anchors: dict[str, set[str]] = {}
    other_c: list[Citation] = []
    other_x: list[XRef] = []
    fragile: list[dict] = []
    readme = ""
    names = sorted(os.listdir(cat_dir))
    for name in names:
        if not name.endswith(".md"):
            continue
        path = os.path.join(cat_dir, name)
        area, invs, fnds, anch, oc, ox, lines = parse_file(path, root, cat_dir)
        anchors[name] = anch | {_slug(l[2:].strip()) for l in lines if l.startswith("## ") or l.startswith("# ")}
        if re.match(r"\d+-", name):
            for inv in invs:
                inv.area = area
            invariants += invs
        findings += fnds
        other_c += oc
        other_x += ox
        if name == "findings.md":
            fragile = _fragile_table(lines)
        if name == "README.md":
            readme = "\n".join(lines)
    probes: dict[str, list[str]] = {}
    pdir = os.path.join(cat_dir, "probes")
    if os.path.isdir(pdir):
        from . import rustsrc

        for f in sorted(os.listdir(pdir)):
            if f.endswith(".rs"):
                src = rustsrc.load(pdir, f)
                probes[os.path.join("probes", f)] = [t.name for t in rustsrc.tests(src)]
    return Catalogue(cat_dir, root, invariants, findings, fragile, readme, anchors, probes, other_c, other_x)


def _slug(heading: str) -> str:
    """GitHub-style anchor for a heading without an explicit {#id}."""
    h = re.sub(r"\{#[\w-]+\}", "", heading).strip().lower()
    h = re.sub(r"[^\w\- ]", "", h)
    return h.replace(" ", "-")
