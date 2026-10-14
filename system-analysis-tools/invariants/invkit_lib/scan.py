"""Find candidate invariant sites in Rust source.

Each detector looks for one kind of evidence that the code is assuming or
maintaining something. None of them decides what the invariant *is*; they
point a reader (human or agent) at the places worth reading, grouped so that
the reading can be organised by mechanism.

Detectors (the `kind` of a candidate):

  claim        comments that state a rule: must, always, never, only, at most…
  panic        assumptions that crash when false: unwrap, expect, map[&k], assert
  guard        validation at a mutation point: early return of an error or no-op
  normalise    clamping and rounding that keep values in a domain
  field-write  writes to a pub field outside its type's impl (bypasses the API)
  identity     id counters, with their scope (per value vs. process)
  key          hashing and cache keys (identity consumers)
  limit        named constants: caps, budgets, maxima
  dual         code embedded in another language (a parity obligation)
  order        sequencing: sequence numbers, channels, locks, cancellation
  durable      file-system writes, renames, deletes (atomicity, cleanup)
  snapshot     clones of state handed to history or background work
  borrow       explicit drops and borrow comments (borrow-checker discipline at runtime)
  sentinel     magic values used as encodings (1_000_000 + i, mode == 3)
  contract     trait methods that implementors must honour, with impl counts
  default      conversions that fill a field with a constant (`From<Id> for Link
               { output: 0 }`): callers passing the short form get an assumption
"""
from __future__ import annotations

import re
from collections import Counter, defaultdict
from dataclasses import asdict, dataclass, field

from . import rustsrc

KINDS = ["claim", "panic", "guard", "normalise", "field-write", "identity", "key", "limit", "dual",
         "order", "durable", "snapshot", "borrow", "sentinel", "contract", "default"]


@dataclass
class Candidate:
    kind: str
    file: str
    line: int
    item: str  # enclosing Type::fn
    text: str  # the line (or comment) that triggered it
    note: str = ""
    end: int | None = None


CLAIM = re.compile(
    r"\b(must|always|never|only|invariant|guarantee[sd]?|assum(?:e|es|ed|ing)|ensures?|"
    r"at most|at least|exactly|by construction|deliberately|so that|otherwise|cannot|can't|"
    r"safe(?:ly)?|unique|same|equal|before|after|until)\b",
    re.I,
)

# Ranking weights per detector: roughly each detector's lift over chance when
# its candidates were compared with the 256 citations of the Delos catalogue
# (system-models/invariants). Coarse on purpose; see README "Calibration".
WEIGHTS = {
    "key": 3.0, "borrow": 2.5, "durable": 2.0, "snapshot": 2.0, "guard": 2.0,
    "sentinel": 1.5, "dual": 1.5, "claim": 1.5, "normalise": 1.5, "panic": 1.5,
    "order": 1.0, "field-write": 1.0, "limit": 1.0, "identity": 1.0, "contract": 0.5, "default": 1.0,
}
PANIC = re.compile(
    r"\.unwrap\(\)|\.expect\(|\bpanic!|\bunreachable!|\bassert(?:_eq|_ne)?!|"
    r"\w\[&[\w.]+\]|\.copy_from_slice\(|\[\s*0\s*\]\s*(?:[.;?)]|$)"
)
GUARD = re.compile(
    r"return\s+(?:Err|err|None|false|Ok\(\(\)\))\b|\bbail!|\bensure!|else\s*\{\s*(?:return|continue)\b|"
    r"\{\s*return\s*;?\s*\}|^\s*return\s*;|\.ok_or|\?\s*;\s*$"
)
NORMALISE = re.compile(r"\.(?:clamp|max|min)\(\s*-?[\d.]|\.(?:round|floor|ceil|trunc)\(\)|clamp_\w+\(|\.saturating_|\.div_ceil\(")
SEQ = re.compile(r"\b(seq|latest|generation|epoch|version|token|cancel\w*)\b")
ORDER = re.compile(
    r"\b(?:sync_)?channel\s*\(|\bMutex\b|\bRwLock\b|\bAtomic\w+\b|\bspawn\w*\s*\(|\.lock\(\)|"
    r"\bCondvar\b|\.store\(|\.load\(|\.fetch_\w+\("
)
DURABLE = re.compile(
    r"\bfs::(?:rename|remove_file|remove_dir_all|create_dir\w*|write|copy|File::create)|"
    r"\b(?:rename|remove_file|remove_dir_all|create_dir_all)\(|File::create|\.sync_all\(|\.tmp\b|\bfsync\b|temp_dir\("
)
SNAPSHOT = re.compile(r"\.clone\(\)")
SNAPSHOT_CTX = re.compile(r"\b(history|undo|redo|snapshot|job|graph|state|prev|push)\b", re.I)
BORROW = re.compile(r"\bdrop\(\w+\)|\b(borrow|AlreadyBorrowed)\b")
SENTINEL = re.compile(r"\b\d{1,3}(?:_\d{3}){2,}\b|==\s*\d+u\b|\b(?:u32|u64|usize|i64|f32|f64)::MAX\b")
LIMIT = re.compile(r"\b(?:pub\s+)?(?:const|static)\s+([A-Z][A-Z0-9_]+)\s*:\s*[^=]+=\s*(.+);")
HASH = re.compile(r"\.hash\(&mut\b|\bHasher\b|DefaultHasher|\bhash_\w+\(|\bcache\.(?:get|insert)\(|\bimpl\s+Hash\b")


def _snippet(s: str, n=140) -> str:
    s = s.strip()
    return s if len(s) <= n else s[: n - 1] + "…"


def pub_fields(srcs: list[rustsrc.Source]):
    """{field: [(owner type, file, line)]} for `pub` fields of structs."""
    out = defaultdict(list)
    for s in srcs:
        for it in s.items:
            if it.kind != "struct":
                continue
            for ln in range(it.line, it.end + 1):
                m = re.match(r"\s*pub\s+(\w+)\s*:", s.masked_lines[ln - 1])
                if m:
                    out[m.group(1)].append((it.name, s.path, ln))
    return out


def detect(srcs: list[rustsrc.Source], tests_glob=("tests/",)) -> list[Candidate]:
    cands: list[Candidate] = []
    fields = pub_fields(srcs)
    structs = {it.name: (s.path, it) for s in srcs for it in s.items if it.kind == "struct"}
    trait_impls = Counter(it.trait for s in srcs for it in s.items if it.kind == "impl" and it.trait)

    for s in srcs:
        if s.path.startswith(tests_glob):
            continue
        test_spans = [(it.line, it.end) for it in s.items if it.kind == "mod" and it.name == "tests"]

        def in_tests(ln):
            return any(a <= ln <= b for a, b in test_spans)

        def add(kind, ln, text, note="", end=None):
            if in_tests(ln):
                return
            cands.append(Candidate(kind, s.path, ln, s.where(ln), _snippet(text), note, end))

        # --- comments
        for c in s.comments:
            body = c.text.strip()
            if not body or body.startswith(("TODO", "FIXME")) and not CLAIM.search(body):
                continue
            words = CLAIM.findall(body)
            if words:
                add("claim", c.line, body, ", ".join(sorted({w.lower() for w in words})))
            if BORROW.search(body):
                add("borrow", c.line, body)

        # --- code lines
        for k, line in enumerate(s.masked_lines):
            ln = k + 1
            code = line.strip()
            if not code or code.startswith("#"):
                continue
            raw = s.lines[k]
            if PANIC.search(line):
                add("panic", ln, raw)
            if GUARD.search(line):
                add("guard", ln, raw)
            if NORMALISE.search(line):
                add("normalise", ln, raw)
            if ORDER.search(line) or (SEQ.search(line) and re.search(r"[<>!=]=|[<>]|\+=", line)):
                add("order", ln, raw)
            if DURABLE.search(raw):
                add("durable", ln, raw)
            if SNAPSHOT.search(line) and SNAPSHOT_CTX.search(line):
                add("snapshot", ln, raw)
            if re.search(r"\bdrop\(\w+\)", line):
                add("borrow", ln, raw)
            if SENTINEL.search(line):
                add("sentinel", ln, raw)
            if HASH.search(line):
                add("key", ln, raw)
            m = LIMIT.search(line)
            if m and re.search(r"\d", m.group(2)):
                add("limit", ln, raw, m.group(1))

            # writes to pub fields from outside the owning type
            for m in re.finditer(r"\.(\w+)\s*(?:\[[^\]]*\])?\s*(?:=(?!=)|\+=|-=)|\.(\w+)\.(?:push|insert|remove|retain|clear|extend|truncate|drain|pop|swap|get_mut)\b", line):
                f = m.group(1) or m.group(2)
                if f not in fields:
                    continue
                owners = {o for o, _, _ in fields[f]}
                here = s.enclosing(ln, ("impl",))
                if here and here.name in owners and not here.trait:
                    continue
                if f in ("id", "name", "x", "y", "w", "h"):  # too generic to be useful
                    continue
                add("field-write", ln, raw, f"{'/'.join(sorted(owners))}.{f}")

        # --- id counters and their scope
        for it in s.items:
            if it.kind != "struct":
                continue
            for ln in range(it.line, it.end + 1):
                m = re.match(r"\s*(?:pub(?:\([^)]*\))?\s+)?(next_\w*|\w*counter\w*|\w*_seq|seq)\s*:\s*(\w+)", s.masked_lines[ln - 1])
                if m:
                    scope = "per value: cloned with its owner, restarts when the owner is rebuilt" if "Clone" in it.derives \
                        else "per instance"
                    add("identity", ln, s.lines[ln - 1], f"{it.name}.{m.group(1)}: {scope}")
        for k, line in enumerate(s.masked_lines):
            if re.search(r"\bstatic\s+\w+\s*:\s*(?:[\w:]+::)?Atomic(?:U|I)\d+", line):
                add("identity", k + 1, s.lines[k], "static atomic: unique for the life of the process")
            elif re.search(r"\b(?:self\.)?(next_\w+|\w*counter\w*)\s*\+=\s*1\b", line):
                add("identity", k + 1, s.lines[k], "issues ids")

        # --- code in another language inside a string
        for lit in s.strings:
            if lit.end_line - lit.line >= 2 and re.search(r"\bfn\s+\w+\s*\(|\breturn\b|@group|@compute|SELECT\b", lit.text):
                add("dual", lit.line, s.lines[lit.line - 1], "embedded code: the other half of a parity pair?", lit.end_line)

        # --- conversions that fill in a constant
        for it in s.items:
            if it.kind == "impl" and it.trait in ("From", "Default") and it.end - it.line < 30:
                body = "\n".join(s.masked_lines[it.line - 1:it.end])
                lits = re.findall(r"\b(\w+)\s*:\s*(0|None|false|true|\d+(?:\.\d+)?|Default::default\(\))\s*[,}]", body)
                if lits:
                    hdr = s.lines[it.line - 1].strip()
                    add("default", it.line, hdr, "fills " + ", ".join(f"{a}: {b}" for a, b in lits), it.end)

        # --- trait contracts: declared methods of project traits
        for it in s.items:
            if it.kind != "trait":
                continue
            n = trait_impls.get(it.name, 0)
            for f in s.items:
                if f.kind == "fn" and it.line < f.line <= it.end:
                    doc = f.doc or ""
                    add("contract", f.line, s.lines[f.line - 1],
                        f"{it.name}: {n} impl(s){'; ' + doc[:100] if doc else ''}")
    return cands


def hotspots(cands: list[Candidate], top=None):
    """Rank enclosing items by how many *different* kinds of evidence they hold."""
    by = defaultdict(list)
    for c in cands:
        by[(c.file, c.item)].append(c)
    rows = []
    for (f, item), cs in by.items():
        kinds = Counter(c.kind for c in cs)
        score = sum(WEIGHTS.get(k, 1.0) * (1 + 0.25 * (min(n, 5) - 1)) for k, n in kinds.items())
        rows.append({"file": f, "item": item or "(top level)", "kinds": dict(kinds),
                     "score": round(score, 2), "lines": sorted({c.line for c in cs})})
    rows.sort(key=lambda r: (-r["score"], r["file"], r["item"]))
    return rows[:top] if top else rows


def identity_report(cands: list[Candidate], srcs: list[rustsrc.Source]):
    """Id sources and their scope, next to the places that hash or key on
    things. A per-value counter whose ids reach a long-lived key is the
    pattern behind 'equal hash ⇒ equal content' failures."""
    ids = [c for c in cands if c.kind == "identity"]
    keys = [c for c in cands if c.kind == "key"]
    issuers = {}
    for c in ids:
        if c.note == "issues ids" and c.item:
            issuers[c.item] = c
    owners = {c.note.split(".")[0] for c in ids if "per value" in c.note}
    rebuilt = defaultdict(list)
    for s in srcs:
        for k, line in enumerate(s.masked_lines):
            for o in owners:
                if re.search(rf"\b{o}::(?:new|default)\s*\(", line) and not s.where(k + 1).startswith(f"{o}::"):
                    rebuilt[o].append(f"{s.path}:{k + 1} {s.where(k + 1)}")
    callers = defaultdict(list)
    for s in srcs:
        for name in issuers:
            short = name.split("::")[-1]
            for k, line in enumerate(s.masked_lines):
                if re.search(rf"\b{re.escape(short)}\s*\(", line) and not re.search(rf"\bfn\s+{re.escape(short)}\b", line):
                    callers[name].append(f"{s.path}:{k + 1} {s.where(k + 1)}")
    return {"sources": [asdict(c) for c in ids], "issuers": {k: callers[k] for k in issuers},
            "rebuilt": dict(rebuilt), "keys": [asdict(c) for c in keys],
            "question": "Do ids from a per-value counter reach any key that outlives the value "
                        "(a cache, a hook, a file)? Cloning restores the counter and rebuilding "
                        "restarts it, so such keys can collide. If so, write a probe."}


def run(root: str, dirs=("src",), exclude=()):
    files = [f for f in rustsrc.rust_files(root, dirs) if not any(f.startswith(e) for e in exclude)]
    srcs = [rustsrc.load(root, f) for f in files]
    cands = detect(srcs)
    tests = []
    for f in rustsrc.rust_files(root, ("src", "tests")):
        s = rustsrc.load(root, f)
        tests += [{"file": f, "name": t.name, "line": t.line} for t in rustsrc.tests(s)]
    return {
        "format": "invkit.scan/1",
        "root": root,
        "files": files,
        "counts": dict(Counter(c.kind for c in cands)),
        "candidates": [asdict(c) for c in cands],
        "hotspots": hotspots(cands),
        "identity": identity_report(cands, srcs),
        "tests": tests,
    }


def recall(scan: dict, cat, slack=2, kinds=None):
    """How much of a catalogue the scan points at.

    A citation is *covered* when some candidate lies within its line range
    (± slack). An invariant is covered when any of its citations is."""
    by_file = defaultdict(list)
    for c in scan["candidates"]:
        if kinds is None or c["kind"] in kinds:
            by_file[c["file"]].append(c)
    per_inv = {}
    total = hit = 0
    missed = []
    kinds_hit = Counter()
    for inv in cat.invariants:
        cits = [c for c in inv.citations if c.start and c.file.endswith(".rs") and c.file.startswith(tuple(f"{d}/" for d in ("src",)))]
        got = 0
        for c in cits:
            total += 1
            near = [x for x in by_file.get(c.file, []) if c.start - slack <= x["line"] <= (c.end or c.start) + slack
                    or (x.get("end") and x["line"] <= c.start <= x["end"])]
            if near:
                hit += 1
                got += 1
                kinds_hit.update({x["kind"] for x in near})
            else:
                missed.append({"invariant": inv.id, "cite": c.text, "file": c.file, "start": c.start, "end": c.end,
                               "context": c.local[:120]})
        per_inv[inv.id] = (got, len(cits))
    covered = sum(1 for g, n in per_inv.values() if g > 0)
    with_cits = sum(1 for g, n in per_inv.values() if n > 0)
    return {"citations": {"covered": hit, "total": total},
            "invariants": {"covered": covered, "total": with_cits},
            "per_invariant": per_inv, "missed": missed, "kinds": dict(kinds_hit),
            "candidates": sum(len(v) for v in by_file.values())}
