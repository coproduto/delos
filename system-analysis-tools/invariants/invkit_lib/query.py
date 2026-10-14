"""Targeted questions about the code, asked while writing an invariant.

  writes FIELD   every place that changes `.FIELD` (assignment, index
                 assignment, mutating method). "Maintained by" claims that say
                 "only X writes this" are checked with it.
  refs NAME      every place that uses NAME outside its definition, grouped by
                 enclosing item. "Depended on by" is built from it.
"""
from __future__ import annotations

import re
from collections import defaultdict

from . import rustsrc

MUTATORS = r"push|push_back|push_front|insert|remove|retain|clear|extend|truncate|drain|pop|pop_back|pop_front|swap|get_mut|entry|append|sort\w*|dedup\w*|set"


def _sources(root, dirs):
    return [rustsrc.load(root, f) for f in rustsrc.rust_files(root, dirs)]


def writes(root: str, field: str, dirs=("src",), init=False):
    f = re.escape(field)
    pat = re.compile(
        rf"\.{f}\s*(?:\[[^\]]*\]\s*)*(?:=(?!=)|\+=|-=|\*=|/=|\|=|&=)"
        rf"|\.{f}\s*(?:\[[^\]]*\]\s*)*\.(?:{MUTATORS})\s*\("
        rf"|&mut\s+[\w.]*\.{f}\b"
        rf"|\b{f}\s*:\s*[^,}}:][^,}}]*[,}}]"  # struct literal field init
    )
    out = []
    for s in _sources(root, dirs):
        for k, line in enumerate(s.masked_lines):
            m = pat.search(line)
            if not m:
                continue
            kind = "init" if re.match(rf"\b{f}\s*:", m.group(0)) else "write"
            if kind == "init" and (not init or re.search(r"\b(?:let|fn)\b", line)):
                continue
            if kind == "init" and re.search(r"\b(?:pub\s+)?" + f + r"\s*:\s*[\w&<(\[]", line) and s.enclosing(k + 1, ("struct",)):
                continue  # the field's declaration
            fn = s.enclosing(k + 1, ("fn",))
            if kind == "init" and fn and (k + 1, m.start()) < fn.body:
                continue  # a parameter in a (multi-line) signature
            out.append({"file": s.path, "line": k + 1, "item": s.where(k + 1), "kind": kind, "text": s.lines[k].strip()})
    return out


def _other_owner(s, ln: int, before: str, owner: str) -> bool:
    """True when the use whose line text precedes it is `before` certainly
    belongs to a type other than `owner`: `Other::name`, or `self.name` /
    `Self::name` inside `impl Other`. Receivers of unknown type (`x.name`)
    are not excluded."""
    m = re.search(r"\b(\w+)\s*::\s*$", before)
    if m and m.group(1) != "Self":
        return m.group(1) != owner
    if m or re.search(r"\bself\s*\.\s*$", before):
        imp = s.enclosing(ln, ("impl", "trait"))
        return bool(imp and imp.kind == "impl" and imp.name != owner)
    return False


def refs(root: str, name: str, dirs=("src",)):
    short = name.split("::")[-1]
    owner = name.split("::")[-2] if "::" in name else None
    pat = re.compile(rf"(?<![\w]){re.escape(short)}(?![\w])")
    out = []
    for s in _sources(root, dirs):
        for k, line in enumerate(s.masked_lines):
            hits = list(pat.finditer(line))
            if owner:
                hits = [h for h in hits if not _other_owner(s, k + 1, line[: h.start()], owner)]
            if not hits:
                continue
            ln = k + 1
            it = s.enclosing(ln, ("fn",))
            # skip the definition itself
            if re.search(rf"\b(?:fn|struct|enum|trait|const|static|type|mod)\s+{re.escape(short)}\b", line):
                continue
            if it and it.name == short and (owner is None or it.qual == name) and it.line == ln:
                continue
            out.append({"file": s.path, "line": ln, "item": s.where(ln), "text": s.lines[k].strip()})
    return out


def group(rows):
    by = defaultdict(list)
    for r in rows:
        by[(r["file"], r["item"])].append(r)
    return by


def format_rows(rows, title):
    out = [f"{title}: {len(rows)} site(s) in {len(group(rows))} item(s)"]
    for (f, item), rs in sorted(group(rows).items()):
        out.append(f"  {item or '(top level)'}  [{f}]")
        for r in rs:
            tag = f" ({r['kind']})" if r.get("kind") and r["kind"] != "write" else ""
            out.append(f"    {r['line']:>5}{tag}  {r['text'][:110]}")
    return "\n".join(out)
