"""Spec ancestry: which code descends from a spec clause, and which does not.

Seeds are the exact lines that *clause* entries cite under **Implemented by**
(findings, unspecified-behaviour notes and clause Notes are context, not spec). From the
seeds, references are followed through a lexical reference graph:

- an item is **specified** when clause citations cover at least
  `SPECIFIED` of its lines; its whole body is a seed;
- an item is **partly specified** when citations cover less; only its
  cited lines are seeds, the rest of its lines are *uncited lines*;
- an item reached from a seed is **support**: plumbing a clause relies on.
  Its whole body is followed further, unless some clause cites part of it,
  in which case only its cited lines are followed;
- everything else is **unspecified**: no clause leads to it.

Three reports come out of this:

1. *Unspecified state* (`state`): struct fields written somewhere but
   never named on a line a clause cites, with the clauses whose paths
   run through their writers.
2. *Uncited branches of cited functions* (`spans`): runs of uncited
   code lines inside specified or partly specified items, the behaviour
   a clause skips over.
3. *Unspecified code* (`clusters`): unspecified items grouped by the
   references between them, each with its *triggers*: the uncited lines
   of partly specified items that lead into it (how a user reaches it).

The reference graph is lexical: a name refers to every item of that name,
narrowed by a `Type::` or `module::` qualifier and by `.method(` syntax.
Very common names (`new`, `get`, `set`, …) are ignored. It over-connects
more often than it misses, so *unspecified* is the conservative verdict.
"""
from __future__ import annotations

import os
import re
from collections import defaultdict

from . import rust
from .matrix import Matrix

SPECIFIED = 0.3
SPAN_MIN = 3   # code lines for an uncited span to be reported
SEED_SECTIONS = ("implemented by",)
STOP = set("""new get set len push insert remove clone map iter from into default take is_empty contains as_ref unwrap
expect ok err some none fmt eq hash next read write peek min max abs round floor ceil sqrt join split find filter
any all first last copied cloned collect extend sort retain drain clear pop to_string as_str parse lock send recv
spawn open load save size name id kind title file path apply after inverse new_in with is_some is_none
unwrap_or unwrap_or_default and_then or_else map_err context data bytes rect pixels finish drop""".split())

MUTATION = re.compile(
    r"\.write\(\)|\.set\(|\.(?:push|insert|remove|clear|pop|retain|truncate|drain|swap|store|fetch_add|take)\(|"
    r"\bstd::fs::(?:write|rename|remove_file|remove_dir_all|create_dir_all|copy)\b|"
    r"\bself(?:\.\w+)+\s*[-+*/]?=(?![=>])"
)
IDENT_AT = re.compile(r"(?<![A-Za-z0-9_])(?:([A-Za-z_][A-Za-z0-9_]*)::)?([A-Za-z_][A-Za-z0-9_]*)(?![A-Za-z0-9_])")


class Index:
    def __init__(self, tree: rust.Tree, dirs=("src",)):
        self.tree = tree
        self.items: list[rust.Item] = []
        for f in tree.files():
            if not f.startswith(tuple(d + "/" for d in dirs)):
                continue
            for it in tree.get(f).items:
                if it.is_test or it.kind not in ("fn", "const", "static"):
                    continue
                self.items.append(it)
        self.by_short: dict[str, list[rust.Item]] = defaultdict(list)
        for it in self.items:
            self.by_short[it.short].append(it)
        self._refs: dict[tuple[str, int], list[rust.Item]] = {}

    @staticmethod
    def owner(it: rust.Item) -> str | None:
        return it.name.split("::")[0] if "::" in it.name else None

    @staticmethod
    def module(it: rust.Item) -> str:
        stem = os.path.splitext(os.path.basename(it.file))[0]
        return os.path.basename(os.path.dirname(it.file)) if stem == "mod" else stem

    def items_in(self, path: str) -> list[rust.Item]:
        return [it for it in self.items if it.file == path]

    def innermost(self, path: str, line: int) -> rust.Item | None:
        best = None
        for it in self.items_in(path):
            if it.start <= line <= it.end and (best is None or it.end - it.start < best.end - best.start):
                best = it
        return best

    def refs_on_line(self, path: str, line: int) -> list[rust.Item]:
        """Items a source line refers to (excluding the item being defined)."""
        key = (path, line)
        if key in self._refs:
            return self._refs[key]
        src = self.tree.get(path)
        text = src.masked[line - 1] if src and line <= len(src.masked) else ""
        here = self.innermost(path, line)
        self_owner = self.owner(here) if here else None
        out: list[rust.Item] = []
        for m in IDENT_AT.finditer(text):
            qual, name = m.group(1), m.group(2)
            before = text[:m.start()].rstrip()
            dot = before.endswith(".") and not before.endswith("..")
            rest = text[m.end():].lstrip()
            # `name(…)`, or a Dioxus component used as `Name { … }` in rsx!
            call = rest.startswith("(") or (name[0].isupper() and rest.startswith("{") and not qual)
            if dot and not call:
                continue  # a field access, not a method
            if name not in self.by_short or (name in STOP and not qual):
                continue
            cands = self.by_short[name]
            if qual:
                q = self_owner if qual == "Self" else qual
                # `Type::name` or `module::name`; an unknown qualifier (std::, a
                # nested module) resolves to nothing rather than to every `name`
                cands = [c for c in cands if self.owner(c) == q or self.module(c) == q]
            elif dot:
                cands = [c for c in cands if self.owner(c)] or cands
            elif call:
                free = [c for c in cands if not self.owner(c)]
                same = [c for c in cands if self_owner and self.owner(c) == self_owner]
                cands = free or same or cands
            else:
                # a bare name that is not called: a constant, or a field or
                # local that happens to share a function's name
                cands = [c for c in cands if c.kind in ("const", "static")]
            for c in cands:
                if c.start == line and c.file == path:
                    continue  # its own signature
                if c not in out:
                    out.append(c)
        self._refs[key] = out
        return out

    def refs_in(self, it: rust.Item, lines=None) -> list[rust.Item]:
        out: list[rust.Item] = []
        for ln in (lines if lines is not None else range(it.start, it.end + 1)):
            for r in self.refs_on_line(it.file, ln):
                if r is not it and r not in out:
                    out.append(r)
        return out


def _key(it: rust.Item) -> str:
    return f"{it.file}::{it.name}@{it.start}"


def analyse(m: Matrix, tree: rust.Tree) -> dict:
    idx = Index(tree)
    cited_lines: dict[str, dict[int, set[str]]] = defaultdict(lambda: defaultdict(set))
    note_lines: dict[str, dict[int, set[str]]] = defaultdict(lambda: defaultdict(set))
    # Only what a clause says implements it is spec: citations in its Notes
    # (edge cases, defects) and in note entries are context, not ancestry.
    for e in m.entries:
        for c in e.citations:
            seed = e.kind == "clause" and c.section.lower().startswith(SEED_SECTIONS)
            target = cited_lines if seed else note_lines
            for ln in range(c.start, c.end + 1):
                target[c.path][ln].add(e.id)

    cls: dict[str, str] = {}
    share: dict[str, float] = {}
    for it in idx.items:
        lines = range(it.start, it.end + 1)
        n = sum(1 for ln in lines if cited_lines[it.file].get(ln))
        share[_key(it)] = n / max(1, len(lines))
        if n and share[_key(it)] >= SPECIFIED:
            cls[_key(it)] = "specified"
        elif n:
            cls[_key(it)] = "partly specified"
        else:
            cls[_key(it)] = "unspecified"

    # Reachability, per clause, from the cited lines.
    reach: dict[str, set[str]] = defaultdict(set)   # item key -> clauses whose paths reach it
    by_key = {_key(it): it for it in idx.items}
    for e in m.clauses():
        frontier: list[rust.Item] = []
        seen: set[str] = set()
        for c in (c for c in e.citations if c.section.lower().startswith(SEED_SECTIONS)):
            if c.path not in tree.files() and not c.path.endswith(".rs"):
                continue
            for ln in range(c.start, c.end + 1):
                for r in idx.refs_on_line(c.path, ln):
                    if _key(r) not in seen:
                        seen.add(_key(r))
                        frontier.append(r)
        while frontier:
            it = frontier.pop()
            reach[_key(it)].add(e.id)
            # A function some clause cites is specified only where cited:
            # follow its cited lines, not the rest of its body.
            own = [ln for ln in range(it.start, it.end + 1) if cited_lines[it.file].get(ln)]
            for r in idx.refs_in(it, own or None):
                if _key(r) not in seen:
                    seen.add(_key(r))
                    frontier.append(r)
    for k, c in cls.items():
        if c == "unspecified" and reach.get(k):  # reached from a cited line
            cls[k] = "support"

    # 1. Unspecified clusters.
    unspec = [it for it in idx.items if cls[_key(it)] == "unspecified"]
    uset = {_key(it) for it in unspec}
    adj: dict[str, set[str]] = defaultdict(set)
    for it in unspec:
        for r in idx.refs_in(it):
            if _key(r) in uset:
                adj[_key(it)].add(_key(r))
                adj[_key(r)].add(_key(it))
    triggers: dict[str, list[dict]] = defaultdict(list)
    for it in idx.items:
        if cls[_key(it)] not in ("specified", "partly specified"):
            continue
        for ln in range(it.start, it.end + 1):
            if cited_lines[it.file].get(ln):
                continue
            for r in idx.refs_on_line(it.file, ln):
                if _key(r) in uset:
                    triggers[_key(r)].append({"from": it.name, "file": it.file, "line": ln})
    groups = []
    done: set[str] = set()
    for it in unspec:
        k = _key(it)
        if k in done:
            continue
        comp, stack = [], [k]
        done.add(k)
        while stack:
            x = stack.pop()
            comp.append(x)
            for y in adj[x]:
                if y not in done:
                    done.add(y)
                    stack.append(y)
        members = sorted((by_key[x] for x in comp), key=lambda i: (i.file, i.start))
        trig = [t for x in comp for t in triggers.get(x, [])]
        notes = sorted({e for x in members for ln in range(x.start, x.end + 1) for e in note_lines[x.file].get(ln, ())})
        groups.append({
            "lines": sum(i.end - i.start + 1 for i in members),
            "files": sorted({i.file for i in members}),
            "members": [f"{i.name} ({i.file}:{i.start}-{i.end})" for i in members],
            "mutates": any(MUTATION.search("\n".join(tree.get(i.file).masked[i.start:i.end])) for i in members),
            "triggers": trig,
            "noted_in": notes,
        })
    groups.sort(key=lambda g: (-bool(g["triggers"]), -g["lines"]))

    # 2. Unspecified state: fields that code writes but no cited line uses.
    # Naming a field where it is declared does not specify its behaviour,
    # so the declaration line does not count as a mention.
    cited_by_file: dict[str, str] = {}
    for path, lines in cited_lines.items():
        src = tree.get(path)
        if src:
            # masked: a comment mentioning a field is not a use of it
            cited_by_file[path] = {ln: src.masked[ln - 1] for ln in lines if ln <= len(src.masked)}
    state = []
    for f in tree.files():
        if not f.startswith("src/"):
            continue
        src = tree.get(f)
        for it in src.items:
            if it.kind != "struct" or it.is_test:
                continue
            for ln in range(it.start + 1, it.end):
                fm = re.match(r"^\s*(?:pub(?:\([^)]*\))?\s+)?([a-z_][a-z0-9_]*)\s*:\s*\S", src.masked[ln - 1])
                if not fm:
                    continue
                field = fm.group(1)
                word = re.compile(rf"\b{field}\b")
                mentions = [(p2, l2) for p2, lines in cited_by_file.items() for l2, text in lines.items()
                            if word.search(text) and not (p2 == f and l2 == ln)]
                if mentions:
                    continue
                writers = _field_writers(idx, field)
                if not writers:
                    continue
                through = sorted({c for w in writers for c in reach.get(_key(w), ())})
                notes = sorted({e for w in writers for l2 in range(w.start, w.end + 1) for e in note_lines[w.file].get(l2, ())}
                               | {e for e in note_lines[f].get(ln, ())})
                state.append({"field": f"{it.name}.{field}", "file": f, "line": ln,
                              "declared_on_cited_line": bool(cited_lines[f].get(ln)),
                              "writers": [{"item": w.name, "file": w.file, "start": w.start, "class": cls[_key(w)]} for w in writers],
                              "clauses_through_writers": through, "noted_in": notes})
    state.sort(key=lambda s: (-len(s["clauses_through_writers"]), s["field"]))

    # 3. Uncited spans inside cited functions: the branches a clause skips.
    spans = []
    trivial = re.compile(r"^[\s{}()\[\];,]*$|^\s*(?:\}\s*else\s*\{|_ => \{\}|\},?|\)\s*;?)\s*$")
    for it in idx.items:
        if cls[_key(it)] not in ("specified", "partly specified"):
            continue
        src = tree.get(it.file)
        run: list[int] = []

        def flush():
            code = [ln for ln in run if not trivial.match(src.masked[ln - 1])]
            if len(code) >= SPAN_MIN:
                body = [src.masked[ln - 1] for ln in code]
                calls = sorted({r.name for ln in code for r in idx.refs_on_line(it.file, ln)
                                if cls[_key(r)] in ("unspecified", "support")})
                writes = [ln for ln in code if MUTATION.search(src.masked[ln - 1])]
                notes = sorted({e for ln in run for e in note_lines[it.file].get(ln, ())})
                first = next((src.lines[ln - 1].strip() for ln in code), "")
                spans.append({"item": it.name, "file": it.file, "start": run[0], "end": run[-1], "code_lines": len(code),
                              "first": first[:100], "calls": calls[:8], "writes": len(writes), "noted_in": notes})

        for ln in range(it.start + 1, it.end + 1):
            if cited_lines[it.file].get(ln):
                flush()
                run = []
            else:
                run.append(ln)
        flush()
    spans.sort(key=lambda x: (-(x["writes"] > 0 or bool(x["calls"])), -x["code_lines"]))

    counts = defaultdict(lambda: [0, 0])
    for it in idx.items:
        c = counts[cls[_key(it)]]
        c[0] += 1
        c[1] += it.end - it.start + 1
    return {
        "counts": {k: {"items": v[0], "lines": v[1]} for k, v in counts.items()},
        "items": {_key(it): {"name": it.name, "file": it.file, "start": it.start, "end": it.end, "class": cls[_key(it)],
                             "cited_share": round(share[_key(it)], 2), "clauses": sorted(reach.get(_key(it), ()))} for it in idx.items},
        "clusters": groups, "state": state, "spans": spans,
    }


def _field_writers(idx: Index, field: str) -> list[rust.Item]:
    pat = re.compile(rf"\.{field}\s*(?:[-+*/]?=(?![=>])|\.(?:set|write|push|insert|remove|clear|pop|take|store)\b)|\b{field}\s*\.\s*(?:set|write)\(")
    out = []
    for it in idx.items:
        src = idx.tree.get(it.file)
        if any(pat.search(l) for l in src.masked[it.start - 1:it.end]):
            out.append(it)
    return out


def report_md(a: dict, top: int = 25, min_lines: int = 10) -> str:
    out = ["# Spec ancestry", "",
           "Seeds are the lines clause entries cite. *Support* is code reached from them; *unspecified* is code no clause leads to.", "",
           "| Class | Items | Lines |", "|---|---|---|"]
    for k in ("specified", "partly specified", "support", "unspecified"):
        c = a["counts"].get(k, {"items": 0, "lines": 0})
        out.append(f"| {k} | {c['items']} | {c['lines']} |")
    out += ["", "## Unspecified state", "",
            "Fields that code writes but that no clause-cited line uses (the field's own declaration does not count). "
            "*Paths through writers*: how many clauses reach a function that writes the field. A high number means hidden state "
            "woven into specified behaviour.", "",
            "| Field | Paths through writers | Writers | Noted in |", "|---|---|---|---|"]
    for st in a["state"]:
        w = ", ".join(f"`{x['item']}` ({x['class']})" for x in st["writers"][:5]) + (" …" if len(st["writers"]) > 5 else "")
        decl = " (declared on a cited line)" if st["declared_on_cited_line"] else ""
        out.append(f"| `{st['field']}` `{st['file']}:{st['line']}`{decl} | {len(st['clauses_through_writers'])} | {w} | {', '.join(st['noted_in'])} |")
    out += ["", "## Uncited branches of cited functions", "",
            f"Runs of {SPAN_MIN}+ code lines that no clause cites, inside functions that clauses do cite: the behaviour a clause "
            "skips over. Spans that change state or call uncited code come first.", "",
            "| Where | Lines | Starts with | Calls | Noted in |", "|---|---|---|---|---|"]
    for sp in a["spans"][:top]:
        first = sp["first"].replace("|", "\\|")
        calls = ", ".join(f"`{c}`" for c in sp["calls"][:4])
        mark = " ✎" if sp["writes"] else ""
        out.append(f"| `{sp['item']}` `{sp['file']}:{sp['start']}-{sp['end']}` | {sp['code_lines']}{mark} | `{first}` | {calls} | {', '.join(sp['noted_in'])} |")
    if len(a["spans"]) > top:
        out.append(f"\n{len(a['spans']) - top} more spans (`--json` for all). ✎: the span changes state.")
    else:
        out.append("\n✎: the span changes state.")
    shown = [g for g in a["clusters"] if g["triggers"] or g["lines"] >= min_lines]
    out += ["", "## Unspecified code", "",
            f"Groups of functions no clause leads to (shown: those reached from uncited lines of cited code, or of {min_lines}+ lines). "
            "*Triggered from* names the uncited lines that call into the group: how a user reaches it.", ""]
    for i, g in enumerate(shown[:top], 1):
        head = ", ".join(x.split(" (")[0] for x in g["members"][:5]) + (" …" if len(g["members"]) > 5 else "")
        noted = f"noted in {', '.join(g['noted_in'])}" if g["noted_in"] else "**not noted anywhere**"
        out.append(f"### {i}. {head}")
        out.append("")
        out.append(f"{g['lines']} lines in {', '.join(f'`{f}`' for f in g['files'])}{' · changes state' if g['mutates'] else ''} · {noted}")
        out.append("")
        seen = set()
        for t in g["triggers"]:
            k = (t["file"], t["line"])
            if k not in seen:
                seen.add(k)
                out.append(f"- triggered from `{t['from']}` at `{t['file']}:{t['line']}`")
        if len(g["members"]) > 5:
            out.append(f"- members: {', '.join(f'`{x}`' for x in g['members'])}")
        out.append("")
    hidden = len(a["clusters"]) - min(len(shown), top)
    if hidden > 0:
        out.append(f"{hidden} more groups, untriggered and under {min_lines} lines or beyond the top {top} (`--json` for all).")
    return "\n".join(out) + "\n"


def _flagged_lines(a: dict) -> dict[str, set[int]]:
    f: dict[str, set[int]] = defaultdict(set)
    for v in a["items"].values():
        if v["class"] == "unspecified":
            f[v["file"]].update(range(v["start"], v["end"] + 1))
    for sp in a["spans"]:
        f[sp["file"]].update(range(sp["start"], sp["end"] + 1))
    return f


def _code_lines(tree: rust.Tree, path: str) -> set[int]:
    src = tree.get(path)
    return {i for i, l in enumerate(src.masked, 1) if l.strip() and l.strip() not in "{}();,"} if src else set()


def recall(a: dict, m: Matrix, tree: rust.Tree, prefix: str = "U") -> dict:
    """Scores the analysis against hand-written note entries (ids starting
    with `prefix`). Two measures:

    - *overlap*: entries whose citations touch any flagged code. Lax: on
      Delos, citations moved to random places in the same files score
      higher than the real ones, so do not read it alone.
    - *enrichment*: the flagged share of the entries' cited code lines,
      divided by the flagged share of all code lines in the same files.
      1.0 is chance."""
    writers = {(w["file"], w["start"]) for st in a["state"] for w in st["writers"]}
    flagged = _flagged_lines(a)
    rows, hit, tot, files = [], 0, 0, set()
    for e in m.entries:
        if not e.id.startswith(prefix) or e.kind != "note":
            continue
        how, h, t = set(), 0, 0
        for c in e.citations:
            files.add(c.path)
            code = _code_lines(tree, c.path)
            for ln in range(c.start, c.end + 1):
                if ln in code:
                    t += 1
                    h += ln in flagged[c.path]
            for v in a["items"].values():
                if v["file"] == c.path and v["start"] <= c.end and c.start <= v["end"]:
                    if v["class"] == "unspecified":
                        how.add("code")
                    if (v["file"], v["start"]) in writers:
                        how.add("state")
            for sp in a["spans"]:
                if sp["file"] == c.path and sp["start"] <= c.end and c.start <= sp["end"]:
                    how.add("span")
        rows.append({"id": e.id, "title": e.title, "found_by": sorted(how), "flagged_lines": h, "code_lines": t})
        hit, tot = hit + h, tot + t
    base_h = sum(len(_code_lines(tree, p) & flagged[p]) for p in files)
    base_t = sum(len(_code_lines(tree, p)) for p in files)
    share, base = hit / max(1, tot), base_h / max(1, base_t)
    return {"found": sum(1 for r in rows if r["found_by"]), "total": len(rows), "rows": rows,
            "flagged_share": round(share, 3), "chance_share": round(base, 3), "enrichment": round(share / base, 2) if base else None}


def metrics(a: dict) -> dict:
    return {
        "classes": a["counts"],
        "state_fields": len(a["state"]),
        "spans": len(a["spans"]), "span_lines": sum(s["code_lines"] for s in a["spans"]),
        "groups": len(a["clusters"]), "triggered_groups": sum(1 for g in a["clusters"] if g["triggers"]),
    }
