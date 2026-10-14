"""Verify an invariant catalogue against the code it describes."""
from __future__ import annotations

import os
import re
import subprocess
from dataclasses import dataclass, field

from . import catalog as C
from . import rustsrc

SLACK = 3  # lines around a citation searched for the names it mentions

# Words that appear in backticks but are not identifiers worth anchoring on.
STOP = {
    "Some", "None", "Ok", "Err", "self", "Self", "true", "false", "mut", "pub", "fn", "let",
    "if", "else", "for", "in", "as", "u8", "u32", "u64", "f32", "f64", "usize", "i64",
    "Vec", "Option", "Arc", "Mutex", "String", "the", "and", "or", "of", "to",
}


@dataclass
class Issue:
    level: str  # error | warn | info
    kind: str
    where: str
    message: str


@dataclass
class Report:
    issues: list[Issue] = field(default_factory=list)
    stats: dict = field(default_factory=dict)
    anchoring: list[dict] = field(default_factory=list)

    def add(self, level, kind, where, message):
        self.issues.append(Issue(level, kind, where, message))

    def count(self, level):
        return sum(1 for i in self.issues if i.level == level)


class Code:
    """Lazy cache of parsed source files."""

    def __init__(self, root):
        self.root = root
        self._src: dict[str, rustsrc.Source | None] = {}
        self._text: dict[str, list[str] | None] = {}

    def lines(self, rel):
        if rel not in self._text:
            p = os.path.join(self.root, rel)
            try:
                with open(p, encoding="utf-8") as f:
                    self._text[rel] = f.read().split("\n")
            except (OSError, UnicodeDecodeError):
                self._text[rel] = None
        return self._text[rel]

    def src(self, rel):
        if rel not in self._src:
            self._src[rel] = rustsrc.load(self.root, rel) if rel.endswith(".rs") and self.lines(rel) is not None else None
        return self._src[rel]


def anchor_names(names: list[str]) -> list[str]:
    """The identifier fragments worth looking for in code."""
    out = []
    for n in names:
        for part in n.split("::"):
            if part in STOP or len(part) < 3 or part.isdigit():
                continue
            if part not in out:
                out.append(part)
    return out


WORD = re.compile(r"[A-Za-z][A-Za-z]{3,}|\d{2,}")
PROSE_STOP = set("""
that this with from when what which then than they them there their these those into onto only also
does done each every have has been being were will would should could must here same both other
because while where after before about above below under over just like more most less such some
whole part parts first last next once twice ever never always case cases thing things uses used using
""".split())


def token(w: str) -> str:
    """Words are compared by their first five letters, which is a crude but
    consistent stem: rounds/round, snapshots/snapshot, trusted/trust."""
    return w.lower()[:5]


class Vocabulary:
    """Document frequency of tokens over the source files. Tokens present in
    most files ("node", "image") are weak evidence; rare ones are strong."""

    def __init__(self, code: "Code", files: list[str]):
        df: dict[str, int] = {}
        name_df: dict[str, int] = {}
        for f in files:
            text = "\n".join(code.lines(f) or [])
            for t in {token(x) for x in WORD.findall(text)}:
                df[t] = df.get(t, 0) + 1
            for t in {x.lower() for x in re.findall(r"[A-Za-z_]\w*", text)}:
                name_df[t] = name_df.get(t, 0) + 1
        self.n = max(1, len(files))
        self.df = df
        self.name_df = name_df

    def weight(self, t: str) -> float:
        if t.isdigit():
            return 1.5  # a specific constant (60 snapshots, 512 px) is strong evidence
        r = self.df.get(t, 0) / self.n
        return 1.0 if r <= 0.15 else 0.5 if r <= 0.4 else 0.0

    def name_weight(self, name: str) -> float:
        """Identifiers: rare in the code base → strong."""
        r = self.name_df.get(name.lower(), 0) / self.n
        return 1.0 if r <= 0.2 else 0.5

    def words(self, text: str) -> list[str]:
        text = CODE_OR_LINK.sub(" ", text)
        out = []
        for w in WORD.findall(text):
            t = token(w)
            if w.lower() in PROSE_STOP or t in out or self.weight(t) == 0:
                continue
            out.append(t)
        return out


CODE_OR_LINK = re.compile(r"\[[^\[\]]*\]\([^)]*\)|`[^`]*`")


def anchored(code: Code, cit: C.Citation, vocab: Vocabulary | None = None, slack=SLACK):
    """(state, evidence). state is one of:
    name        an identifier mentioned next to the link appears at the cited lines
    words       no identifier, but distinctive words from the clause do
    unanchored  neither
    unchecked   not a line citation into a file, or nothing to look for
    """
    if cit.start is None:
        return "unchecked", []
    lines = code.lines(cit.file)
    if lines is None:
        return "unchecked", []
    lo = max(1, cit.start - slack)
    hi = min(len(lines), (cit.end or cit.start) + slack)
    window = "\n".join(lines[lo - 1 : hi])
    low = window.lower()
    enclosing = set()
    src = code.src(cit.file)
    if src:
        for ln in range(cit.start, (cit.end or cit.start) + 1):
            for it in src.items:
                if it.line <= ln <= it.end:
                    enclosing |= {p.lower() for p in it.qual.split("::")}
                    if it.trait:
                        enclosing.add(it.trait.lower())
    names = anchor_names(cit.names)
    hit = [n for n in names if n.lower() in enclosing or re.search(rf"(?<![\w]){re.escape(n.lower())}(?![\w])", low)]
    # an identifier that is everywhere (`inputs`, `node`) is weak evidence on its own
    strong = [n for n in hit if n.lower() in enclosing or vocab is None or vocab.name_weight(n) >= 1]
    if strong or len(hit) >= 2:
        return "name", hit
    if vocab is not None:
        words = vocab.words(cit.local or cit.context)
        context = low
        got = [w for w in words if (w in context if not w.isdigit() else re.search(rf"(?<!\d){w}(?!\d)", low))]
        score = sum(vocab.weight(w) for w in got)
        # the clause names the function or type being cited ("undo restores …")
        own = {token(p) for e in enclosing for p in e.split("_") if len(p) >= 4}
        for w in WORD.findall(CODE_OR_LINK.sub(" ", cit.local or "")):
            t = token(w)
            if t in own and t not in got and w.lower() not in PROSE_STOP:
                got.append(t)
                score += 1.5
        if score >= 2:
            return "words", got
    if not names and not (vocab and vocab.words(cit.local or cit.context)):
        return "unchecked", []
    return "unanchored", []


def _git(root, *args):
    try:
        return subprocess.run(["git", "-C", root, *args], capture_output=True, text=True, check=True).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return None


def _expand_ids(text: str) -> list[str]:
    """`J1–J5` → J1..J5, plus plain ids."""
    out = []
    for m in re.finditer(r"\b([A-Z])(\d+)\s*[–-]\s*([A-Z])?(\d+)\b", text):
        p, x, q, y = m.group(1), int(m.group(2)), m.group(3) or m.group(1), int(m.group(4))
        if p == q and x < y:
            out += [f"{p}{k}" for k in range(x, y + 1)]
    out += re.findall(r"\b([A-Z]\d+)\b", text)
    return list(dict.fromkeys(out))


def run(cat: C.Catalogue, strict_sections=False) -> Report:
    rep = Report()
    code = Code(cat.root)
    invs = cat.invariants
    ids = {}

    # --- structure
    for inv in invs:
        if inv.id in ids:
            rep.add("error", "duplicate-id", inv.md, f"{inv.id} defined twice")
        ids[inv.id] = inv
        if inv.anchor != inv.id.lower():
            rep.add("warn", "anchor", f"{inv.md}:{inv.line}", f"{inv.id} has anchor #{inv.anchor}")
        for s in C.REQUIRED:
            if s not in inv.sections:
                rep.add("warn", "missing-section", f"{inv.md}:{inv.line}", f"{inv.id} has no {s} section")
        missing = [s for s in C.EXPECTED if s not in inv.sections]
        if missing:
            rep.add("warn" if strict_sections else "info", "sparse", f"{inv.md}:{inv.line}",
                    f"{inv.id} lacks: {', '.join(missing)}")
        if not inv.citations:
            rep.add("warn", "uncited", f"{inv.md}:{inv.line}", f"{inv.id} cites no code")

    # --- citations
    all_cits = [(inv.id, c) for inv in invs for c in inv.citations] + [("", c) for c in cat.other_citations]
    states = {"name": 0, "words": 0, "unanchored": 0, "unchecked": 0}
    vocab = Vocabulary(code, rustsrc.rust_files(cat.root))
    for owner, c in all_cits:
        where = f"{c.md}:{c.md_line}"
        lines = code.lines(c.file)
        if lines is None:
            hint = " (outside --root: is the root right?)" if c.file.startswith("..") else ""
            rep.add("error", "missing-file", where, f"{c.text} → {c.file} does not exist{hint}")
            continue
        if c.start is not None:
            n = len(lines) - (1 if lines and lines[-1] == "" else 0)
            if not (1 <= c.start <= (c.end or c.start) <= n):
                rep.add("error", "bad-line", where, f"{c.text}: {c.file} has {n} lines")
                continue
            m = re.search(r"#L(\d+)", c.href)
            if m and int(m.group(1)) != c.start:
                rep.add("error", "href-mismatch", where, f"text says {c.text} but link goes to #L{m.group(1)}")
        tm = re.match(r"([\w./-]+\.\w+)", c.text)
        if tm and not c.file.endswith(tm.group(1)):
            rep.add("error", "href-mismatch", where, f"text names {tm.group(1)} but link goes to {c.file}")
        state, hit = anchored(code, c, vocab)
        states[state] += 1
        rec = {"invariant": owner, "where": where, "cite": c.text, "file": c.file, "start": c.start,
               "end": c.end, "state": state, "names": anchor_names(c.names), "matched": hit}
        rep.anchoring.append(rec)
        if state == "unanchored":
            rep.add("info", "unanchored", where,
                    f"{owner or '-'} {c.text}: nothing from \"{c.local[:70]}\" within ±{SLACK} lines")
    rep.stats["citations"] = states

    # --- tests
    test_index: dict[str, set[str]] = {}
    for inv in invs:
        for t in inv.tests:
            where = f"{t.md}:{t.md_line}"
            if code.lines(t.file) is None:
                rep.add("error", "missing-test-file", where, f"{inv.id}: {t.file} does not exist")
                continue
            if t.name is None:
                continue
            if t.file not in test_index:
                src = code.src(t.file)
                test_index[t.file] = {x.name for x in rustsrc.tests(src)} if src else set()
            if t.name not in test_index[t.file]:
                rep.add("error", "missing-test", where, f"{inv.id}: no #[test] fn {t.name} in {t.file}")
    rep.stats["test_refs"] = sum(len(i.tests) for i in invs)

    # --- cross references
    for x in [x for inv in invs for x in inv.xrefs] + cat.other_xrefs:
        md = x.md_target or x.md
        where = f"{x.md}:{x.md_line}"
        if md not in cat.anchors:
            rep.add("error", "bad-xref", where, f"[{x.text}] → {md} is not in the catalogue")
        elif x.target and x.target not in cat.anchors[md]:
            rep.add("error", "bad-xref", where, f"[{x.text}] → #{x.target} is not defined in {md}")
        elif re.fullmatch(r"[A-Z]+\d+", x.text) and x.text in ids:
            inv = ids[x.text]
            if inv.md != md or inv.anchor != x.target:
                rep.add("error", "bad-xref", where, f"[{x.text}] points at {md}#{x.target}, but {x.text} is {inv.md}#{inv.anchor}")

    # --- README claims
    readme = cat.readme
    areas = len({i.area for i in invs})
    m = re.search(r"(\d+)\s+invariants\s+across\s+(\w+)\s+areas", readme)
    words = {"one": 1, "two": 2, "three": 3, "four": 4, "five": 5, "six": 6, "seven": 7, "eight": 8, "nine": 9, "ten": 10}
    if m:
        n, a = int(m.group(1)), words.get(m.group(2).lower(), m.group(2))
        if n != len(invs):
            rep.add("error", "count", "README.md", f"says {n} invariants, catalogue has {len(invs)}")
        if str(a) != str(areas):
            rep.add("error", "count", "README.md", f"says {m.group(2)} areas, catalogue has {areas}")
    else:
        rep.add("info", "count", "README.md", "no 'N invariants across M areas' claim found")
    violated = [i.id for i in invs if i.status_class == "violated"]
    m = re.search(r"(\w+) invariants turned out to be \*\*violated\*\*", readme)
    k = m and (int(m.group(1)) if m.group(1).isdigit() else words.get(m.group(1).lower()))
    if m and k != len(violated):
        rep.add("error", "count", "README.md", f"says {m.group(1)} violated, catalogue has {len(violated)}: {violated}")
    for line in readme.split("\n"):
        if line.startswith("|") and not set(line) <= set("|- "):
            for i in _expand_ids(line):
                if i not in ids and re.fullmatch(r"[A-Z]\d+", i):
                    rep.add("error", "unknown-id", "README.md", f"table mentions {i}, which is not defined")
    for i in set(re.findall(r'\b([A-Z]\d+)\b(?=[^"\n]*")', readme)) - set(ids):
        rep.add("warn", "unknown-id", "README.md", f"diagram mentions {i}, which is not defined")

    # --- violations, findings and probes
    finding_invs = {i for f in cat.findings for i in f.invariants}
    for v in violated:
        if v not in finding_invs:
            rep.add("error", "unreported", ids[v].md, f"{v} is VIOLATED but no finding names it")
    probe_names = {n: p for p, names in cat.probes.items() for n in names}
    for f in cat.findings:
        for i in f.invariants:
            if i not in ids:
                rep.add("error", "unknown-id", "findings.md", f"finding {f.number} names unknown {i}")
            elif ids[i].status_class != "violated":
                rep.add("warn", "status", "findings.md", f"finding {f.number} names {i}, whose status is {ids[i].status_class}")
        if not f.probes:
            rep.add("warn", "unconfirmed", "findings.md", f"finding {f.number} names no probe")
        for p in f.probes:
            if p not in probe_names:
                rep.add("error", "missing-probe", "findings.md", f"finding {f.number}: probe {p} is not in probes/")
    referenced = {p for f in cat.findings for p in f.probes}
    for n, p in probe_names.items():
        if n not in referenced:
            rep.add("warn", "orphan-probe", p, f"probe {n} is not named by any finding")
    for row in cat.fragile:
        for i in row["invariants"]:
            if re.fullmatch(r"[A-Z]+\d+", i) and i not in ids:
                rep.add("error", "unknown-id", "findings.md", f"fragile row {row['n']} names unknown {i}")
            elif i in ids and ids[i].status_class == "holds":
                rep.add("info", "status", "findings.md",
                        f"fragile row {row['n']} lists {i}, whose status reads as an unqualified 'Holds'")

    # --- drift since the analysed commit
    m = re.search(r"commit `([0-9a-f]{7,40})`", readme)
    if m:
        commit = m.group(1)
        rep.stats["commit"] = commit
        head = _git(cat.root, "rev-parse", "--short", "HEAD")
        changed = _git(cat.root, "diff", "--name-only", commit, "--", "src", "tests")
        if changed is None:
            rep.add("info", "drift", "README.md", f"could not compare with commit {commit}")
        else:
            files = [f for f in changed.split("\n") if f]
            cited = {c.file for _, c in all_cits}
            stale = sorted(set(files) & cited)
            rep.stats["drift"] = {"head": head, "changed": files, "cited_changed": stale}
            if stale:
                rep.add("warn", "drift", "README.md",
                        f"{len(stale)} cited file(s) changed since {commit}: {', '.join(stale)}; re-check their citations")
    rep.stats["invariants"] = len(invs)
    rep.stats["areas"] = areas
    rep.stats["status"] = C._count(i.status_class for i in invs)
    return rep


def format_text(rep: Report, verbose=False) -> str:
    out = []
    s = rep.stats
    out.append(f"{s.get('invariants')} invariants in {s.get('areas')} areas; status {s.get('status')}")
    c = s.get("citations", {})
    total = sum(c.values()) or 1
    ok = c.get("name", 0) + c.get("words", 0)
    out.append(f"citations: {ok} anchored ({c.get('name', 0)} by identifier, {c.get('words', 0)} by wording), "
               f"{c.get('unanchored', 0)} unanchored, {c.get('unchecked', 0)} unchecked "
               f"({100 * ok / max(1, ok + c.get('unanchored', 0)):.0f}% of checkable)")
    out.append(f"test references: {s.get('test_refs', 0)}")
    if "drift" in s:
        d = s["drift"]
        out.append(f"since {s['commit']}: {len(d['changed'])} changed file(s) in src/ and tests/, "
                   f"{len(d['cited_changed'])} of them cited")
    out.append(f"{rep.count('error')} error(s), {rep.count('warn')} warning(s), {rep.count('info')} note(s)")
    for lvl in ("error", "warn", "info"):
        if lvl == "info" and not verbose:
            continue
        for i in rep.issues:
            if i.level == lvl:
                out.append(f"  {lvl:5} {i.kind:16} {i.where}: {i.message}")
    return "\n".join(out)
