"""invkit command line."""
from __future__ import annotations

import argparse
import json
import os
import shlex
import subprocess
import sys

from . import catalog, check, pins, probe, query, relate, report, scan


def _log(*a):
    print(*a, file=sys.stderr)


def _out(text: str, path: str | None):
    if path:
        os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
        with open(path, "w", encoding="utf-8") as f:
            f.write(text if text.endswith("\n") else text + "\n")
        _log(f"wrote {path}")
    else:
        sys.stdout.write(text if text.endswith("\n") else text + "\n")


def _dump(obj, path):
    _out(json.dumps(obj, indent=1, ensure_ascii=False, default=list), path)


def _cat(args):
    if not os.path.isdir(args.catalogue):
        raise SystemExit(f"invkit: error: catalogue directory not found: {args.catalogue}")
    return catalog.load(args.catalogue, args.root)


def _json(path, what):
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except json.JSONDecodeError as e:
        raise SystemExit(f"invkit: error: {what} {path} is not valid JSON: {e}")


def cmd_scan(args):
    res = scan.run(args.root, tuple(args.dirs), tuple(args.exclude))
    _log(f"{len(res['candidates'])} candidates in {len(res['files'])} files: {res['counts']}")
    if args.md:
        _out(report.scan_markdown(res, args.top), args.md)
    if args.output or not args.md:
        _dump(res, args.output)


def cmd_catalog(args):
    cat = _cat(args)
    d = cat.to_json()
    _log(f"{d['counts']}")
    _dump(d, args.output)


def cmd_check(args):
    cat = _cat(args)
    rep = check.run(cat, strict_sections=args.strict)
    rc = 1 if rep.count("error") else 0
    pin_rows = None
    if args.pins:
        pin_rows = pins.verify(cat.root, _json(args.pins, "pins file"))
        if any(r["state"] != "same" for r in pin_rows):
            rc = rc or 1
    if args.json:
        _dump({"stats": rep.stats, "issues": [i.__dict__ for i in rep.issues],
               "anchoring": rep.anchoring, "pins": pin_rows}, args.output)
    else:
        text = check.format_text(rep, verbose=args.verbose)
        if pin_rows is not None:
            text += "\npins: " + pins.format_text(pin_rows)
        _out(text, args.output)
    return rc


def cmd_pin(args):
    cat = _cat(args)
    p = pins.pin(cat)
    _log(f"pinned {len(p['pins'])} citations at {p['commit']}")
    _dump(p, args.output)


def cmd_probe(args):
    cat = _cat(args)
    rep = probe.run(cat, cargo_args=tuple(args.cargo_args), timeout=args.timeout, keep=args.keep, log=_log)
    if args.json:
        _dump(rep, args.output)
    else:
        _out(probe.format_text(rep), args.output)
    return 0 if rep.get("ok", True) else 1


def cmd_relate(args):
    cat = _cat(args)
    es = relate.edges(cat)
    if args.mermaid:
        _out(relate.mermaid(cat, es), args.output)
        return
    rows = relate.roots(cat, es)
    chosen = relate.readme_roots(cat)
    if args.json:
        _dump({"edges": es, "ranking": rows, "readme_roots": chosen}, args.output)
        return
    out = [f"{len(relate.pairs(es))} explicit relations between {len(cat.invariants)} invariants",
           "", "Most relied on (by explicit links; an aid for choosing roots, not a substitute):"]
    for r in rows[: args.top]:
        if not r["transitive"]:
            break
        out.append(f"  {r['id']:4} {len(r['direct']):2} direct, {len(r['transitive']):2} transitive  {r['title'][:60]}")
    if chosen:
        ids = cat.by_id()
        missing = [c for c in chosen if c not in ids]
        out += ["", f"README roots: {', '.join(chosen)}" + (f" (undefined: {', '.join(missing)})" if missing else "")]
    unlinked = [i.id for i in cat.invariants if not any(i.id in (a, b) for a, b, _ in es)]
    out += ["", f"Invariants with no explicit relation ({len(unlinked)}): {', '.join(unlinked)}"]
    _out("\n".join(out), args.output)


def cmd_impact(args):
    cat = _cat(args)
    if args.rev is None:
        import re

        m = re.search(r"commit `([0-9a-f]{7,40})`", cat.readme)
        args.rev = m.group(1) if m else "HEAD"
        _log(f"comparing with {args.rev} ({'from README.md' if m else 'no commit named in README.md'})")
    rows = pins.impact(cat, args.rev, args.slack)
    if args.json:
        _dump(rows, args.output)
        return
    if not rows:
        _out(f"no cited line changed since {args.rev}", args.output)
        return
    out = [f"{len(rows)} invariant(s) cite lines changed since {args.rev}:"]
    for r in rows:
        secs = sorted({h["section"] for h in r["hits"]})
        out.append(f"  {r['id']:4} [{r['status']}] {r['title'][:70]}")
        out.append(f"       via {', '.join(h['cite'] for h in r['hits'][:6])} (sections: {', '.join(secs)})")
    out.append("Re-read each one: does the change alter its statement's premise, or the mechanism that maintains it?")
    _out("\n".join(out), args.output)


def cmd_writes(args):
    rows = query.writes(args.root, args.field, tuple(args.dirs), init=args.init)
    if args.json:
        _dump(rows, None)
    else:
        _out(query.format_rows(rows, f"writes to .{args.field}"), None)
    return 0 if rows else 1


def cmd_refs(args):
    rows = query.refs(args.root, args.name, tuple(args.dirs))
    if args.json:
        _dump(rows, None)
    else:
        _out(query.format_rows(rows, f"uses of {args.name}"), None)
    return 0 if rows else 1


def cmd_recall(args):
    sc = _json(args.scan, "scan file")
    cat = _cat(args)
    rc = scan.recall(sc, cat)
    cited = set()
    from . import rustsrc

    cache = {}
    for inv in cat.invariants:
        for c in inv.citations:
            if c.start and c.file in sc["files"]:
                # scan file paths are relative to the project root, which is the catalogue's
                # --root; the scan's own "root" is relative to wherever `scan` was run
                s = cache.setdefault(c.file, rustsrc.load(cat.root, c.file))
                cited.add((c.file, s.where(c.start)))
    prec = {}
    for n in (25, 50, 100):
        top = {(h["file"], h["item"]) for h in sc["hotspots"][:n]}
        prec[n] = len(top & cited) / max(1, min(n, len(top)))
    if args.json:
        _dump({**rc, "hotspot_precision": prec}, args.output)
        return
    c, i = rc["citations"], rc["invariants"]
    out = [f"citations near a candidate: {c['covered']}/{c['total']}",
           f"invariants with at least one: {i['covered']}/{i['total']}",
           "hotspot precision (share of top-N items the catalogue cites): "
           + ", ".join(f"@{n} {p:.0%}" for n, p in prec.items()),
           f"{len(cited)} cited items; {rc['candidates']} candidates",
           "", "Missed citations:"]
    for m in rc["missed"]:
        out.append(f"  {m['invariant']:4} {m['cite']:28} {m['context'][:80]}")
    _out("\n".join(out), args.output)


def main(argv=None):
    ap = argparse.ArgumentParser(prog="invkit", description="Invariant-catalogue tools. See README.md and AGENTS.md.")
    sub = ap.add_subparsers(dest="cmd", required=True)

    def with_root(p):
        p.add_argument("--root", default=".", help="project root (default: .)")

    def with_cat(p):
        p.add_argument("catalogue", help="catalogue directory, e.g. system-models/invariants")
        with_root(p)

    p = sub.add_parser("scan", help="find candidate invariant sites in Rust source")
    with_root(p)
    p.add_argument("--dirs", nargs="+", default=["src"], help="source dirs to scan (default: src)")
    p.add_argument("--exclude", nargs="*", default=[], help="path prefixes to skip")
    p.add_argument("-o", "--output", help="write scan JSON here")
    p.add_argument("--md", help="write the Markdown reading list here")
    p.add_argument("--top", type=int, default=40, help="hotspots in the Markdown report")
    p.set_defaults(fn=cmd_scan)

    p = sub.add_parser("catalog", help="parse a catalogue into JSON")
    with_cat(p)
    p.add_argument("-o", "--output")
    p.set_defaults(fn=cmd_catalog)

    p = sub.add_parser("check", help="verify a catalogue against the code")
    with_cat(p)
    p.add_argument("--pins", help="also verify a pins file from `invkit pin`")
    p.add_argument("--strict", action="store_true", help="warn (not note) when expected sections are missing")
    p.add_argument("-v", "--verbose", action="store_true", help="also print notes")
    p.add_argument("--json", action="store_true")
    p.add_argument("-o", "--output")
    p.set_defaults(fn=cmd_check)

    p = sub.add_parser("pin", help="record the exact text of every cited line range")
    with_cat(p)
    p.add_argument("-o", "--output")
    p.set_defaults(fn=cmd_pin)

    p = sub.add_parser("probe", help="run probe tests; violations are confirmed when probes FAIL")
    with_cat(p)
    p.add_argument("--cargo-args", type=shlex.split, default=[], metavar="ARGS",
                   help="extra cargo test arguments, as one shell-quoted string (e.g. --cargo-args='--features x')")
    p.add_argument("--timeout", type=int, default=1800)
    p.add_argument("--keep", action="store_true", help="leave the copied probe in tests/ (for debugging)")
    p.add_argument("--json", action="store_true")
    p.add_argument("-o", "--output")
    p.set_defaults(fn=cmd_probe)

    p = sub.add_parser("relate", help="relations between invariants from cross-references")
    with_cat(p)
    p.add_argument("--mermaid", action="store_true", help="print a Mermaid graph")
    p.add_argument("--top", type=int, default=12)
    p.add_argument("--json", action="store_true")
    p.add_argument("-o", "--output")
    p.set_defaults(fn=cmd_relate)

    p = sub.add_parser("impact", help="invariants whose cited lines a diff touches")
    with_cat(p)
    p.add_argument("--rev", help="the revision the catalogue's line numbers refer to "
                   "(default: the commit README.md names, else HEAD); compared with the working tree")
    p.add_argument("--slack", type=int, default=0, help="also count hunks this many lines away")
    p.add_argument("--json", action="store_true")
    p.add_argument("-o", "--output")
    p.set_defaults(fn=cmd_impact)

    p = sub.add_parser("writes", help="every write to a field (.FIELD = …, .FIELD.push(…), …)")
    p.add_argument("field")
    with_root(p)
    p.add_argument("--dirs", nargs="+", default=["src"])
    p.add_argument("--init", action="store_true", help="include struct-literal initialisations")
    p.add_argument("--json", action="store_true")
    p.set_defaults(fn=cmd_writes)

    p = sub.add_parser("refs", help="every use of a name outside its definition")
    p.add_argument("name", help="name or Type::name")
    with_root(p)
    p.add_argument("--dirs", nargs="+", default=["src"])
    p.add_argument("--json", action="store_true")
    p.set_defaults(fn=cmd_refs)

    p = sub.add_parser("recall", help="score a scan against a finished catalogue (calibration)")
    p.add_argument("scan", help="scan JSON from `invkit scan -o`")
    with_cat(p)
    p.add_argument("--json", action="store_true")
    p.add_argument("-o", "--output")
    p.set_defaults(fn=cmd_recall)

    args = ap.parse_args(argv)
    try:
        return args.fn(args) or 0
    except OSError as e:  # a missing or unreadable catalogue, pins or scan file
        raise SystemExit(f"invkit: error: {e.filename or ''}{': ' if e.filename else ''}{e.strerror or e}")
    except subprocess.CalledProcessError as e:  # git diff with a bad --rev, not a git work tree
        why = (e.stderr or "").strip().split("\n")[0] or f"exit status {e.returncode}"
        raise SystemExit(f"invkit: error: `{' '.join(map(str, e.cmd))}` failed: {why}")
    except subprocess.TimeoutExpired as e:  # probe --timeout
        raise SystemExit(f"invkit: error: `{' '.join(map(str, e.cmd))}` timed out after {e.timeout:g} s")
