"""Relations between invariants, derived from the catalogue's cross-references.

An invariant that links to another one mentions it while explaining what it
depends on, what maintains it, or what breaks with it. The section the link
sits in sets the direction:

  "Depends on", "Maintained by", "Statement", notes  →  this invariant rests on the target
  "Depended on by"                                    →  the target rests on this invariant

Invariants that many others rest on, directly or transitively, are the
*roots*: the architectural decisions worth a human's attention.
"""
from __future__ import annotations

import re
from collections import defaultdict

from . import catalog as C


def edges(cat: C.Catalogue):
    """[(dependent, premise, section)]"""
    ids = cat.by_id()
    anchor_to_id = {(i.md, i.anchor): i.id for i in cat.invariants}
    out = set()
    for inv in cat.invariants:
        for x in inv.xrefs:
            md = x.md_target or inv.md
            target = anchor_to_id.get((md, x.target))
            if not target or target == inv.id:
                continue
            sec = _section_of(inv, x)
            if sec == "depended":
                out.add((target, inv.id, sec))
            else:
                out.add((inv.id, target, sec))
    return sorted(out)


def _section_of(inv: C.Invariant, x: C.XRef) -> str:
    # sections are recorded in order; find the one whose text contains the link
    needle = f"[{x.text}]({x.href})" if x.href else f"[{x.text}]("
    for name, body in inv.sections.items():
        if needle in body:
            return name
    return "statement"


def roots(cat: C.Catalogue, es=None):
    es = es if es is not None else edges(cat)
    rests_on = defaultdict(set)  # premise -> dependents
    for a, b, _ in es:
        rests_on[b].add(a)

    def reach(n, seen=None):
        seen = set() if seen is None else seen
        for m in rests_on.get(n, ()):
            if m not in seen:
                seen.add(m)
                reach(m, seen)
        return seen

    def dependents(n):
        return reach(n) - {n}  # in a cycle, n reaches itself

    rows = []
    for inv in cat.invariants:
        direct = rests_on.get(inv.id, set())
        rows.append({"id": inv.id, "title": inv.title, "status": inv.status_class,
                     "direct": sorted(direct), "transitive": sorted(dependents(inv.id))})
    rows.sort(key=lambda r: (-len(r["transitive"]), -len(r["direct"]), r["id"]))
    return rows


def mermaid(cat: C.Catalogue, es=None, only=None) -> str:
    es = es if es is not None else edges(cat)
    ids = cat.by_id()
    lines = ["graph LR"]
    used = set()
    for a, b in pairs(es):
        if only and a not in only and b not in only:
            continue
        lines.append(f"  {b} --> {a}")
        used |= {a, b}
    for i in sorted(used, key=lambda s: (s[0], int(re.sub(r"\D", "", s) or 0))):
        t = ids[i].title.replace('"', "'")
        lines.append(f'  {i}["{i} {t[:40]}"]')
    broken = [i for i in used if ids[i].status_class in ("violated", "inherited")]
    if broken:
        lines.append("  classDef broken fill:#fdd,stroke:#c00;")
        lines.append(f"  class {','.join(sorted(broken))} broken;")
    return "\n".join(lines)


def pairs(es) -> list[tuple[str, str]]:
    """Distinct (dependent, premise) pairs: a pair linked from several sections counts once."""
    return sorted({(a, b) for a, b, _ in es})


def readme_roots(cat: C.Catalogue) -> list[str]:
    """Ids the README's diagram places in its `Roots` subgraph, if it has one."""
    m = re.search(r"subgraph\s+Roots(.*?)\bend\b", cat.readme, re.S)
    if not m:
        return []
    return sorted(set(re.findall(r"\b([A-Z]\d+)\b", m.group(1))))
