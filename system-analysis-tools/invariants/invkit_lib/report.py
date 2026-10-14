"""Markdown report of a scan: the reading list for building a catalogue."""
from __future__ import annotations

from collections import Counter, defaultdict


def _loc(c):
    return f"{c['file']}:{c['line']}"


def scan_markdown(scan: dict, top=40) -> str:
    cands = scan["candidates"]
    by_kind = defaultdict(list)
    for c in cands:
        by_kind[c["kind"]].append(c)
    out = ["# Invariant scan", ""]
    out.append(f"{len(cands)} candidate sites in {len(scan['files'])} files; {len(scan['tests'])} tests.")
    out.append("")
    out.append("| kind | sites |")
    out.append("|---|---|")
    for k, n in sorted(scan["counts"].items(), key=lambda kv: -kv[1]):
        out.append(f"| {k} | {n} |")

    out += ["", f"## Hotspots (top {top})", "",
            "Items holding the most *kinds* of evidence, weighted. Read these first: each "
            "usually maintains or relies on one or more invariants.", "",
            "| # | item | file | evidence | lines |", "|---|---|---|---|---|"]
    for i, h in enumerate(scan["hotspots"][:top], 1):
        ev = ", ".join(f"{k}×{n}" if n > 1 else k for k, n in sorted(h["kinds"].items(), key=lambda kv: -kv[1]))
        lines = ", ".join(map(str, h["lines"][:8])) + ("…" if len(h["lines"]) > 8 else "")
        out.append(f"| {i} | `{h['item']}` | {h['file']} | {ev} | {lines} |")

    ident = scan["identity"]
    out += ["", "## Identity: where ids come from, and what keys on them", ""]
    for s in ident["sources"]:
        out.append(f"- `{s['item']}` {_loc(s)}: {s['note']}  \n  `{s['text']}`")
    if ident["issuers"]:
        out += ["", "Issuers and their callers:"]
        for k, v in ident["issuers"].items():
            out.append(f"- `{k}` ← " + ", ".join(f"`{x}`" for x in v))
    if ident.get("rebuilt"):
        out += ["", "Owners of per-value counters are rebuilt (counter restarts) at:"]
        for k, v in ident["rebuilt"].items():
            out.append(f"- `{k}`: " + ", ".join(f"`{x}`" for x in v))
    out += ["", "Hashing and cache keys:"]
    for k in ident["keys"]:
        out.append(f"- {_loc(k)} `{k['item']}`: `{k['text']}`")
    out += ["", f"> **Question.** {ident['question']}"]

    fw = defaultdict(list)
    for c in by_kind["field-write"]:
        fw[c["note"]].append(c)
    out += ["", "## Public fields written outside their type", "",
            "Each is an API bypass. If the field carries a rule (acyclic edges, matching sizes), "
            "every site listed must keep it.", ""]
    for field, cs in sorted(fw.items(), key=lambda kv: -len(kv[1])):
        items = sorted({c["item"] for c in cs})
        out.append(f"- `{field}`: {len(cs)} write(s) in " + ", ".join(f"`{i}`" for i in items))

    out += ["", "## Contracts on implementors", ""]
    tr = defaultdict(list)
    for c in by_kind["contract"]:
        tr[c["note"].split(":")[0]].append(c)
    for t, cs in tr.items():
        n = cs[0]["note"].split(":")[1].split(";")[0].strip()
        out.append(f"- `{t}` ({n}): " + ", ".join(f"`{c['item'].split('::')[-1]}`" for c in cs))

    for kind, title, blurb in [
        ("default", "Conversions that fill in a constant", "Callers using the short form inherit the constant. A "
         "constant like `output: 0` is a single-output assumption."),
        ("dual", "Code embedded in another language", "Usually one half of a pair that must agree with Rust code "
         "(GPU kernels, SQL, shaders): a parity obligation."),
        ("limit", "Named limits", ""),
        ("sentinel", "Sentinel values", "Encodings hidden in numbers."),
        ("durable", "File-system effects", "Look for write-then-rename, cleanup on error and missing fsync."),
        ("snapshot", "Snapshots", "State cloned for history or background work."),
        ("borrow", "Borrow discipline", ""),
        ("claim", "Rules stated in comments", ""),
    ]:
        cs = by_kind.get(kind, [])
        if not cs:
            continue
        out += ["", f"## {title}", ""]
        if blurb:
            out += [blurb, ""]
        for c in cs:
            note = f" ({c['note']})" if c.get("note") and kind not in ("claim",) else ""
            out.append(f"- {_loc(c)} `{c['item'] or '-'}`{note}: {c['text']}")

    out += ["", "## Tests", ""]
    per = Counter(t["file"] for t in scan["tests"])
    for f, n in sorted(per.items()):
        out.append(f"- {f}: {n}")
    return "\n".join(out) + "\n"
