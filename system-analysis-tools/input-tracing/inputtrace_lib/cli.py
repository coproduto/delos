"""Command line for inputtrace. See README.md."""
from __future__ import annotations

import argparse
import json
import os
import re
import sys

from . import model, trace

KINDS = ("handler", "effect", "env", "fs", "dialog", "webview")


def _receiver(text: str) -> tuple:
    """`--receiver NAME=Type` → (NAME, Type); both parts are required."""
    k, eq, v = text.partition("=")
    if not (eq and k.strip() and v.strip()):
        raise argparse.ArgumentTypeError(f"expected NAME=Type, got {text!r}")
    return k.strip(), v.strip()


def _index(args) -> model.Index:
    cfg = model.Config()
    for k, v in args.receiver or []:
        cfg.receivers[k] = v
    if args.src:
        cfg.src_dirs = tuple(args.src)
    return model.build(os.path.abspath(args.root), cfg)


def _entry_json(idx: model.Index, e: model.Entry) -> dict:
    d = {
        "id": e.id, "kind": e.kind, "file": e.file, "line": e.line, "end_line": e.end_line,
        "fn": e.fn, "detail": e.detail, "describe": e.describe(),
    }
    if e.element:
        d["element"] = e.element
    if e.via:
        d["via"] = e.via
    if e.kind in ("handler", "effect"):
        ed = model.entry_edges(idx, e)
        d["calls"] = [f.qual for f, _ in ed.calls]
        d["writes"] = ed.writes
        d["sends"] = [v for v, _ in ed.sends]
        if e.kind == "effect":
            d["subscribes"] = sorted(model.effect_reads(idx, e))
    return d


def cmd_inventory(args) -> int:
    idx = _index(args)
    entries = [e for e in idx.entries if not args.kind or e.kind in args.kind]
    data = [_entry_json(idx, e) for e in entries]
    if args.json:
        with open(args.json, "w") as f:
            json.dump({"root": os.path.relpath(idx.root), "entries": data}, f, indent=1)
        print(f"wrote {len(data)} entries to {args.json}", file=sys.stderr)
    md = ["# Input inventory", "", f"{len(data)} entry points found by `inputtrace inventory`.", ""]
    counts = {k: sum(1 for d in data if d["kind"] == k) for k in KINDS}
    md.append("| Kind | Count |\n|---|---|")
    md += [f"| {k} | {n} |" for k, n in counts.items() if n]
    md.append("")
    for kind in KINDS:
        rows = [d for d in data if d["kind"] == kind]
        if not rows:
            continue
        md += [f"## {kind}", ""]
        if kind in ("handler", "effect"):
            md.append("| Where | Component / fn | Input | Calls | Writes |\n|---|---|---|---|---|")
            for d in rows:
                calls = ", ".join(f"`{c}`" for c in d.get("calls", [])) or "—"
                writes = ", ".join(f"`{w}`" for w in d.get("writes", [])) or "—"
                desc = d["describe"].replace("|", "\\|")
                md.append(f"| `{d['file']}:{d['line']}` | `{d['fn']}` | {desc} | {calls} | {writes} |")
        else:
            md.append("| Where | fn | Input |\n|---|---|---|")
            for d in rows:
                md.append(f"| `{d['file']}:{d['line']}` | `{d['fn']}` | {d['describe']} |")
        md.append("")
    text = "\n".join(md)
    if args.md:
        with open(args.md, "w") as f:
            f.write(text)
        print(f"wrote {args.md}", file=sys.stderr)
    if not args.json and not args.md:
        print(text)
    return 0


def cmd_trace(args) -> int:
    idx = _index(args)
    hits = idx.lookup(args.entry)
    if not hits:
        print(f"no entry or fn matches {args.entry!r}; try `inventory`", file=sys.stderr)
        return 1
    if len(hits) > 1:
        print(f"{args.entry!r} is ambiguous; it matches {len(hits)}:", file=sys.stderr)
        for h in hits:
            print(f"    {h.id if isinstance(h, model.Entry) else h.qual}  [{h.file}:{h.line}]", file=sys.stderr)
        print("pick one by its id, or by FILE:LINE of a line inside it", file=sys.stderr)
        return 1
    root = hits[0]
    opts = trace.Options(depth=args.depth, react=not args.no_react, stop=args.stop, collapse=tuple(args.collapse or ()))
    tree = trace.build(idx, root, opts)
    print(json.dumps(trace.to_json(tree), indent=1) if args.json else trace.render(tree))
    return 0


def cmd_signals(args) -> int:
    idx = _index(args)
    subs = model.subscribers(idx)
    wr = model.writers(idx)
    names = sorted(set(subs) | set(wr))
    if args.json:
        print(json.dumps({s: {"writers": sorted({w[0] for w in wr.get(s, [])}), "effects": [e.id for e in subs.get(s, [])]} for s in names}, indent=1))
        return 0
    for s in names:
        print(s)
        for e in subs.get(s, []):
            print(f"    reruns  {e.describe()}  [{e.file}:{e.line}]")
        ws = sorted({w[0] for w in wr.get(s, [])})
        print(f"    written by {len(ws)}: " + ", ".join(ws[:12]) + (" …" if len(ws) > 12 else ""))
    return 0


def cmd_channels(args) -> int:
    idx = _index(args)
    me = idx.cfg.msg_enum
    senders: dict = {}
    for f in idx.fns:
        for v, line in model.fn_edges(idx, f).sends:
            senders.setdefault(v, []).append(f"{f.qual} [{f.file}:{line}]")
    for v in sorted(set(senders) | set(idx.arms)):
        print(f"{me}::{v}")
        for s in senders.get(v, []):
            print(f"    sent by     {s}")
        for a in idx.arms.get(v, []):
            print(f"    received in {a.fn} [{a.file}:{a.line}]")
    return 0


# `src/ui/mod.rs:84` or `src/ui/mod.rs:84-90` written out …
_CITE = re.compile(r"([\w/.-]+\.rs):(\d+)(?:-(\d+))?")
# … or a Markdown link whose text ends in `:line[-line]` and whose target
# is the file: `[mod.rs:84-90](../../src/ui/mod.rs#L84)`
_LINK = re.compile(r"\[[^\]]*?:(\d+)(?:-(\d+))?\]\(([\w/.-]+\.rs)(?:#L\d+(?:-L?\d+)?)?\)")


def _cites_file(path: str, file: str) -> bool:
    """A cited path names an entry's file (relative to the root) when it is
    that path, or ends in `/` + that path (`../../src/ui/mod.rs`)."""
    return path == file or path.endswith("/" + file)


def _citations(text: str):
    """(path, first line, last line) for every citation in `text`."""
    for m in _CITE.finditer(text):
        yield m.group(1), int(m.group(2)), int(m.group(3) or m.group(2))
    for m in _LINK.finditer(text):
        yield m.group(3), int(m.group(1)), int(m.group(2) or m.group(1))


def cmd_coverage(args) -> int:
    """Which entry points no trace document cites (a citation is a
    `path:line` inside the entry's span)."""
    idx = _index(args)
    kinds = args.kind or ["handler", "effect", "env", "dialog", "webview"]
    if not any(e.kind in kinds for e in idx.entries):
        # nothing to check is a failure, not a pass: the root or --src is
        # probably wrong
        print(f"inputtrace: no entry points of kind {', '.join(kinds)} under {args.root} "
              f"(source dirs: {', '.join(idx.cfg.src_dirs)}); check the root and --src", file=sys.stderr)
        return 1
    cites: dict = {}
    import fnmatch
    for dp, _, files in os.walk(args.docs):
        for fn in files:
            # generated inventories cite everything; they are not traces
            if fn.endswith(".md") and not any(fnmatch.fnmatch(fn, x) for x in args.exclude):
                with open(os.path.join(dp, fn), encoding="utf-8") as f:
                    for path, a, b in _citations(f.read()):
                        cites.setdefault(path, []).append((a, b, fn))
    missing, covered = [], 0
    for e in idx.entries:
        if e.kind not in kinds:
            continue
        hit = [doc for path, cs in cites.items() if _cites_file(path, e.file) for a, b, doc in cs if a <= e.end_line and b >= e.line]
        if hit:
            covered += 1
        else:
            missing.append(e)
    total = covered + len(missing)
    print(f"{covered}/{total} entry points cited by a document in {args.docs}")
    for e in missing:
        print(f"  uncited  {e.file}:{e.line}  {e.kind}  {e.describe()}  in {e.fn}")
    return 1 if missing else 0


def main(argv=None) -> int:
    p = argparse.ArgumentParser(prog="inputtrace", description="Map where input enters a Rust/Dioxus program and trace where it goes.")
    p.add_argument("--receiver", action="append", type=_receiver, metavar="NAME=Type", help="resolve `NAME.method(` to `Type::method` (repeatable)")
    p.add_argument("--src", action="append", metavar="DIR", help="source directories (default: src)")
    sub = p.add_subparsers(dest="cmd", required=True)

    s = sub.add_parser("inventory", help="list every entry point")
    s.add_argument("root", nargs="?", default=".")
    s.add_argument("--kind", action="append", choices=KINDS)
    s.add_argument("--json", metavar="FILE")
    s.add_argument("--md", metavar="FILE")
    s.set_defaults(func=cmd_inventory)

    s = sub.add_parser("trace", help="print the path from an entry point (id, file:line, or fn)")
    s.add_argument("root")
    s.add_argument("entry")
    s.add_argument("--depth", type=int, default=6)
    s.add_argument("--no-react", action="store_true", help="do not follow signal writes into effects")
    s.add_argument("--stop", metavar="REGEX", help="do not expand fns whose qual matches")
    s.add_argument("--collapse", action="append", metavar="QUAL", help="do not expand this fn (repeatable)")
    s.add_argument("--json", action="store_true")
    s.set_defaults(func=cmd_trace)

    s = sub.add_parser("signals", help="signals: who writes them, which effects rerun")
    s.add_argument("root", nargs="?", default=".")
    s.add_argument("--json", action="store_true")
    s.set_defaults(func=cmd_signals)

    s = sub.add_parser("channels", help="message variants: senders and receiving arms")
    s.add_argument("root", nargs="?", default=".")
    s.set_defaults(func=cmd_channels)

    s = sub.add_parser("coverage", help="entry points not cited by any trace document")
    s.add_argument("root")
    s.add_argument("docs")
    s.add_argument("--kind", action="append", choices=KINDS)
    s.add_argument("--exclude", action="append", default=["inventory*.md"], metavar="GLOB", help="document names to ignore (default: inventory*.md)")
    s.set_defaults(func=cmd_coverage)

    args = p.parse_args(argv)
    try:
        return args.func(args)
    except model.MissingSourceDir as e:
        print(f"inputtrace: error: source directory not found: {e} (check the root and --src)", file=sys.stderr)
        return 2
