"""Command line for spectrace. Progress goes to stderr, data to stdout."""
from __future__ import annotations

import argparse
import json
import os
import sys

from . import check, matrix, pins, report, rust, spec as specmod, suggest


def _load(a):
    root = os.path.abspath(a.root)
    sp = specmod.load(os.path.join(root, a.spec))
    tree = rust.Tree(root)
    m = matrix.load(a.matrix, root) if getattr(a, "matrix", None) else None
    return root, sp, tree, m


def _out(a, text: str):
    if getattr(a, "output", None):
        os.makedirs(os.path.dirname(os.path.abspath(a.output)), exist_ok=True)
        with open(a.output, "w", encoding="utf-8") as f:
            f.write(text)
        print(f"wrote {a.output}", file=sys.stderr)
    else:
        sys.stdout.write(text)


def cmd_units(a):
    _, sp, _, m = _load(a)
    if a.uncovered:
        if m is None:
            sys.exit("--uncovered needs a matrix directory")
        _, frac, _ = check.coverage(sp, m)
        units = [u for u in sp.units if frac[u.index] < check.COVERED]
    else:
        units = sp.units
    if a.json:
        _out(a, json.dumps([u.to_json() for u in units], indent=1, ensure_ascii=False) + "\n")
    else:
        _out(a, "".join(f"{u.line:4}  {u.item or '-':7} {u.text}\n" for u in units))


def cmd_check(a):
    root, sp, tree, m = _load(a)
    r = check.run(sp, m, tree)
    if a.pins:
        with open(a.pins, encoding="utf-8") as f:
            v = pins.verify(json.load(f), sp, tree)
        for key, st in v["citations"].items():
            if st["state"] == "moved":
                r.warnings.append(f"moved: {key} -> {st['to']} ({', '.join(st['entries'])})")
            elif st["state"] in ("changed", "missing"):
                r.errors.append(f"{st['state']}: {key} ({', '.join(st['entries'])}): re-read these entries")
        for kind in ("changed", "removed"):
            for u in v["spec"][kind]:
                r.errors.append(f"spec {kind}: SPEC line {u['line']} “{u['text'][:60]}” ({', '.join(u['entries']) or 'unclaimed'})")
        for u in v["spec"]["added"]:
            r.warnings.append(f"spec added: line {u['line']} “{u['text'][:60]}”")
    if a.json:
        print(json.dumps(r.to_json(), indent=1, ensure_ascii=False))
    else:
        for e in r.errors:
            print(f"error: {e}")
        for w in r.warnings:
            print(f"warning: {w}")
        if a.verbose:
            for n in r.notes:
                print(f"note: {n}")
        s = r.stats
        print(
            f"\n{s['clauses']} clause entries ({s['entries']} entries) · {s['spec_units']} spec units, {s['uncovered_units']} not quoted · "
            f"{s['citations']} citations, {s['unanchored']} of {s['anchor_checked']} checkable unanchored · {s['tests']} test refs\n"
            f"{len(r.errors)} errors, {len(r.warnings)} warnings, {len(r.notes)} notes" + ("" if a.verbose else " (-v to list notes)"),
        )
    return 1 if r.errors or (a.strict and r.warnings) else 0


def cmd_summary(a):
    _, sp, _, m = _load(a)
    body = report.summary_md(sp, m)
    if a.write:
        path = os.path.join(a.matrix, "README.md")
        with open(path, encoding="utf-8") as f:
            text = f.read()
        if report.extract_block(text, "summary") is None:
            sys.exit("README.md has no <!-- spectrace:summary --> ... <!-- /spectrace:summary --> block")
        with open(path, "w", encoding="utf-8") as f:
            f.write(report.replace_block(text, "summary", body))
        print(f"updated {path}", file=sys.stderr)
    else:
        sys.stdout.write(body)


def cmd_grid(a):
    _, _, _, m = _load(a)
    if a.format == "csv":
        _out(a, report.grid_csv(m, a.by))
    elif a.format == "json":
        cols, rows = report.grid(m, a.by)
        _out(a, json.dumps({"cols": cols, "rows": rows}, indent=1) + "\n")
    else:
        _out(a, report.grid_md(m, a.by))


def cmd_uncited(a):
    _, _, tree, m = _load(a)
    rows = report.uncited(m, tree)
    _out(a, json.dumps(rows, indent=1) + "\n" if a.json else report.uncited_md(rows))


def cmd_index(a):
    _, _, tree, m = _load(a)
    idx = report.code_index(m, tree)
    if a.json:
        _out(a, json.dumps(idx, indent=1) + "\n")
        return
    lines = []
    for f, items in idx.items():
        cited = [i for i in items if i["entries"]]
        if not cited:
            continue
        lines.append(f"## `{f}`\n")
        for i in cited:
            lines.append(f"- `{i['name']}` ({i['start']}-{i['end']}): {', '.join(i['entries'])}")
        lines.append("")
    _out(a, "\n".join(lines) + "\n")


def cmd_why(a):
    root, _, tree, m = _load(a)
    path, _, line = a.location.rpartition(":")
    if not path or not line.isdigit():
        sys.exit(f"why needs FILE:LINE, e.g. src/graph.rs:243 (got {a.location!r})")
    r = report.why(m, tree, os.path.normpath(path), int(line))
    if a.json:
        print(json.dumps(r, indent=1))
        return 0 if r["direct"] or r["same_item"] else 1
    print(f"{r['path']}:{r['line']} in {r['item']} ({r['item_lines']})")
    for label, ids in (("cited directly by", r["direct"]), ("same item cited by", r["same_item"])):
        for i in ids:
            e = m.by_id(i)
            print(f"  {label} {i} · {e.title if e else ''} [{e.status if e else ''}]")
    if not r["direct"] and not r["same_item"]:
        print("  no entry cites this code")
        return 1
    return 0


def cmd_suggest(a):
    root, sp, tree, m = _load(a)
    if a.clause:
        e = m.by_id(a.clause) if m else None
        if not e:
            sys.exit(f"unknown entry {a.clause}")
        text = " ".join(q for q, _ in e.quotes) or e.title
    else:
        text = " ".join(a.text)
    rows = suggest.rank(text, tree, a.top)
    if a.json:
        print(json.dumps(rows, indent=1))
    else:
        for r in rows:
            print(f"{r['score']:6.2f}  {r['file']}:{r['start']}-{r['end']}  {r['kind']} {r['item']}  [{', '.join(r['matched'])}]")


def cmd_pin(a):
    root, sp, tree, m = _load(a)
    data = pins.pin(sp, m, tree, pins.commit_of(a.matrix))
    _out(a, json.dumps(data, indent=1, ensure_ascii=False) + "\n")


def cmd_impact(a):
    root, sp, tree, m = _load(a)
    base = pins.commit_of(a.matrix)
    rev = a.rev or base
    if not rev:
        sys.exit("no --rev and no commit `…` in the matrix README")
    r = pins.impact(root, rev, sp.path, m, a.slack, base=base)
    if a.json:
        print(json.dumps(r, indent=1, ensure_ascii=False))
        return 0
    print(f"changes since {rev}:")
    if r["carried"]:
        print(f"  (citations carried from {base} to {rev})")
    ids = sorted(set(r["code"]) | set(r["spec"]))
    for i in ids:
        e = m.by_id(i)
        print(f"\n{i} · {e.title if e else ''} [{e.status if e else ''}]")
        for x in r["spec"].get(i, []):
            print(f"  spec  {x}")
        for x in r["code"].get(i, []):
            print(f"  code  {x}")
    for i, keys in sorted(r["unmapped"].items()):
        print(f"\n{i}: cited file gone at {rev}: " + ", ".join(keys))
    if r["files_without_citations"]:
        print("\nchanged files no entry cites: " + ", ".join(r["files_without_citations"]))
    if not ids and not r["unmapped"]:
        print("  no entry is affected")
    return 0


def cmd_probe(a):
    from . import probe

    root, _, _, m = _load(a)
    res = probe.run(root, m, a.cargo_args.split() if a.cargo_args else [], a.timeout)
    bad = 0
    for name, r in res.items():
        if "error" in r:
            print(f"{name}: did not build or run\n{r['error']}")
            bad += 1
            continue
        tag = r["entry"] or "no finding cites it"
        print(f"{r['verdict']:20} {name}  [{tag}]")
        if r["message"]:
            print("    " + r["message"].replace("\n", "\n    "))
        if r["verdict"] != "confirms" or not r["entry"]:
            bad += 1
    return 1 if bad else 0


def cmd_ancestry(a):
    from . import ancestry

    _, _, tree, m = _load(a)
    res = ancestry.analyse(m, tree)
    if a.recall:
        r = ancestry.recall(res, m, tree, a.recall)
        r["metrics"] = ancestry.metrics(res)
        if a.json:
            _out(a, json.dumps(r, indent=1) + "\n")
        else:
            lines = [f"{x['id']:5} {'+'.join(x['found_by']) or 'MISSED':18} {x['flagged_lines']:>4}/{x['code_lines']:<4} {x['title']}" for x in r["rows"]]
            mt = r["metrics"]
            lines.append(f"\noverlap: {r['found']} of {r['total']} (lax; compare with random placement)")
            lines.append(f"enrichment: {r['flagged_share']:.0%} of cited code lines flagged vs {r['chance_share']:.0%} of the same files = x{r['enrichment']}")
            lines.append(f"state fields {mt['state_fields']} · spans {mt['spans']} ({mt['span_lines']} code lines) · "
                         f"groups {mt['groups']} ({mt['triggered_groups']} triggered) · "
                         + " · ".join(f"{k} {v['items']} fn/{v['lines']} lines" for k, v in mt["classes"].items()))
            _out(a, "\n".join(lines) + "\n")
        return 0
    _out(a, json.dumps(res, indent=1) + "\n" if a.json else ancestry.report_md(res, a.top))


def cmd_html(a):
    root, sp, tree, m = _load(a)
    _out(a, report.html_page(sp, m, tree, pins.commit_of(a.matrix)))


def cmd_export(a):
    root, sp, tree, m = _load(a)
    _, frac, _ = check.coverage(sp, m)
    data = {"commit": pins.commit_of(a.matrix), "entries": [e.to_json() for e in m.entries],
            "spec_units": [{**u.to_json(), "covered": round(frac[u.index], 2)} for u in sp.units]}
    _out(a, json.dumps(data, indent=1, ensure_ascii=False) + "\n")


def main(argv=None) -> int:
    p = argparse.ArgumentParser(prog="spectrace", description="Spec-to-code traceability matrix tools.")
    p.add_argument("--root", default=".", help="project root (default: .)")
    p.add_argument("--spec", default="SPEC.md", help="spec path relative to the root (default: SPEC.md)")
    sub = p.add_subparsers(dest="cmd", required=True)

    def mk(name, fn, help_, matrix_arg=True):
        s = sub.add_parser(name, help=help_)
        if matrix_arg:
            s.add_argument("matrix", help="matrix directory")
        s.set_defaults(fn=fn)
        return s

    s = sub.add_parser("units", help="split the spec into sentences (the units a matrix must quote)")
    s.add_argument("matrix", nargs="?", help="matrix directory (needed for --uncovered)")
    s.add_argument("--uncovered", action="store_true", help="only units no entry quotes")
    s.add_argument("--json", action="store_true")
    s.add_argument("-o", "--output")
    s.set_defaults(fn=cmd_units)

    s = mk("check", cmd_check, "verify quotes, coverage, citations, tests, links and the summary")
    s.add_argument("--pins", help="pins file: also report drift since pinning")
    s.add_argument("-v", "--verbose", action="store_true", help="list notes (unanchored citations, unquoted code blocks)")
    s.add_argument("--strict", action="store_true", help="warnings are errors")
    s.add_argument("--json", action="store_true")

    s = mk("summary", cmd_summary, "status counts per area (the README block)")
    s.add_argument("--write", action="store_true", help="rewrite the block in README.md")

    s = mk("grid", cmd_grid, "clause × component matrix")
    s.add_argument("--by", choices=["group", "file"], default="group")
    s.add_argument("--format", choices=["md", "csv", "json"], default="md")
    s.add_argument("-o", "--output")

    s = mk("index", cmd_index, "code → spec: each cited item with the entries citing it")
    s.add_argument("--json", action="store_true")
    s.add_argument("-o", "--output")

    s = mk("uncited", cmd_uncited, "functions no entry cites, per file")
    s.add_argument("--json", action="store_true")
    s.add_argument("-o", "--output")

    s = mk("why", cmd_why, "which entries depend on FILE:LINE")
    s.add_argument("location", help="e.g. src/graph.rs:243")
    s.add_argument("--json", action="store_true")

    s = sub.add_parser("suggest", help="rank code items that may implement some spec text")
    s.add_argument("text", nargs="*", help="spec text (or use --clause with a matrix)")
    s.add_argument("--matrix")
    s.add_argument("--clause", help="entry id whose quotes to use")
    s.add_argument("--top", type=int, default=15)
    s.add_argument("--json", action="store_true")
    s.set_defaults(fn=cmd_suggest)

    s = mk("pin", cmd_pin, "record cited code and quoted spec text for drift checks")
    s.add_argument("-o", "--output")

    s = mk("impact", cmd_impact, "entries a diff touches (code or spec) since the analysed commit")
    s.add_argument("--rev", help="compare the working tree with this revision (default: the README's commit)")
    s.add_argument("--slack", type=int, default=0, help="lines of slack around citations")
    s.add_argument("--json", action="store_true")

    s = mk("ancestry", cmd_ancestry, "code with no spec ancestry: unspecified state, uncited spans and unspecified clusters")
    s.add_argument("--top", type=int, default=25)
    s.add_argument("--recall", metavar="PREFIX", help="score against hand-written note entries with this id prefix (e.g. U)")
    s.add_argument("--json", action="store_true")
    s.add_argument("-o", "--output")

    s = mk("probe", cmd_probe, "run <matrix>/probes/*.rs; a failing probe confirms a finding")
    s.add_argument("--cargo-args", default="", help="extra arguments for cargo test, e.g. '--release'")
    s.add_argument("--timeout", type=int, default=1800)

    s = mk("html", cmd_html, "self-contained HTML viewer")
    s.add_argument("-o", "--output")

    s = mk("export", cmd_export, "the whole matrix as JSON")
    s.add_argument("-o", "--output")

    a = p.parse_args(argv)
    try:
        code = a.fn(a) or 0
        sys.stdout.flush()
        return code
    except BrokenPipeError:
        # The reader (e.g. `head`) closed the pipe: stop quietly, as a
        # process killed by SIGPIPE would.
        try:
            os.dup2(os.open(os.devnull, os.O_WRONLY), sys.stdout.fileno())
        except (OSError, ValueError):
            pass
        return 141
    except OSError as e:  # missing matrix directory, spec, README, …
        sys.exit(f"{e.filename}: {e.strerror}" if e.filename else str(e))
