"""refgraph command line."""
import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

from . import names as names_mod


def _log(msg):
    print(msg, file=sys.stderr)


def _load(path):
    with open(path) as f:
        return json.load(f)


def _dump(obj, path):
    if path in (None, "-"):
        json.dump(obj, sys.stdout, indent=1)
        sys.stdout.write("\n")
    else:
        with open(path, "w") as f:
            json.dump(obj, f, indent=1)
            f.write("\n")
        _log(f"wrote {path}")


def _write_text(text, path):
    if path in (None, "-"):
        sys.stdout.write(text)
    else:
        with open(path, "w") as f:
            f.write(text)
        _log(f"wrote {path}")


# ---------------------------------------------------------------- commands

def cmd_index(a):
    root = os.path.abspath(a.root)
    if not os.path.exists(os.path.join(root, "Cargo.toml")):
        _log("refgraph index only drives rust-analyzer (no Cargo.toml found in "
             f"{root}).\nFor other languages run that language's SCIP indexer yourself "
             "(e.g. scip-typescript, scip-python, scip-java, scip-go) and pass the resulting "
             "index.scip to `refgraph graph`.")
        return 2
    ra = shutil.which("rust-analyzer")
    if not ra:
        _log("rust-analyzer not found on PATH (try `rustup component add rust-analyzer`).")
        return 2
    cmd = [ra, "scip", root, "--output", os.path.abspath(a.output)]
    cfg = None
    if a.features or a.no_default_features or a.all_features:
        conf = {"cargo": {}}
        if a.all_features:
            conf["cargo"]["features"] = "all"
        elif a.features:
            conf["cargo"]["features"] = a.features
        if a.no_default_features:
            conf["cargo"]["noDefaultFeatures"] = True
        cfg = tempfile.NamedTemporaryFile("w", suffix=".json", delete=False)
        json.dump(conf, cfg)
        cfg.close()
        cmd += ["--config-path", cfg.name]
    _log("running: " + " ".join(cmd))
    try:
        r = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    finally:
        if cfg:
            os.unlink(cfg.name)
    if a.verbose or r.returncode:
        _log(r.stdout)
    if r.returncode:
        return r.returncode
    _log(f"wrote {a.output}")
    return 0


def cmd_graph(a):
    from .graph import build
    g = build(a.index, test_globs=a.tests if a.tests else None, exclude_globs=a.exclude or [],
              exclude_symbols=a.exclude_symbols, granularity=a.granularity,
              owner_weight=a.owner_weight, impl_weight=a.impl_weight,
              packages=a.package or None, log=_log)
    _log(f"{len(g['nodes'])} nodes, {len(g['edges'])} edges, "
         f"{sum(w for *_, w in g['edges'])} references; packages: {', '.join(g['meta']['packages'])}")
    _dump(g, a.output)
    return 0


def cmd_sweep(a):
    from .cluster import pick_resolution, sweep
    rows = sweep(_load(a.graph), a.resolutions, a.weighting, a.seeds)
    if a.json:
        _dump({"rows": rows, "suggested": pick_resolution(rows)}, "-")
        return 0
    print(f"{'resolution':>10} {'clusters':>8} {'modularity':>10} {'stability':>9}  sizes")
    for r in rows:
        sizes = " ".join(map(str, r["sizes"][:12])) + (" …" if len(r["sizes"]) > 12 else "")
        print(f"{r['resolution']:>10} {r['clusters']:>8} {r['modularity']:>10.3f} {r['stability']:>9.3f}  {sizes}")
    print(f"suggested resolution: {pick_resolution(rows)}")
    return 0


def cmd_cluster(a):
    from .cluster import cluster
    cl = cluster(_load(a.graph), a.resolution, a.weighting, a.seeds)
    _log(f"{len(cl['clusters'])} clusters, modularity {cl['modularity']:.3f}, stability {cl['stability']:.3f}")
    _dump(cl, a.output)
    return 0


def cmd_report(a):
    from .report import markdown
    cl = _load(a.clusters)
    nm = _load(a.names) if a.names else None
    _write_text(markdown(cl, nm, members=a.members), a.output)
    return 0


def cmd_names(a):
    cl = _load(a.clusters)
    if a.carry_from:
        old_cl, old_names = _load(a.carry_from[0]), _load(a.carry_from[1])
        out = names_mod.carry(old_cl, old_names, cl, a.threshold)
    else:
        out = names_mod.template(cl)
    _dump(out, a.output)
    return 0


def cmd_where(a):
    cl = _load(a.clusters)
    nm = _load(a.names) if a.names else None
    q = a.query.lower()
    files = [(f, split) for f, split in cl["file_split"].items() if q in f.lower()]
    for f, split in files[:a.limit]:
        parts = ", ".join(f"C{c} {names_mod.lookup(nm, c)}".rstrip() + f" ({n})" for c, n in split)
        print(f"file {f}: {parts}")
    if len(files) > a.limit:
        print(f"… {len(files) - a.limit} more files (use --limit)")
    hits = [(m, c["id"]) for c in cl["clusters"] for m in c["members"] if q in m.lower()]
    for m, cid in sorted(hits)[:a.limit]:
        print(f"{m}  ->  C{cid} {names_mod.lookup(nm, cid)}".rstrip())
    if len(hits) > a.limit:
        print(f"… {len(hits) - a.limit} more items (use --limit)")
    if not files and not hits:
        _log(f"no item or file matches {a.query!r}")
        return 1
    return 0


def cmd_analyze(a):
    from .cluster import DEFAULT_RESOLUTIONS, cluster, pick_resolution, sweep
    from .graph import build
    from .report import markdown
    os.makedirs(a.out_dir, exist_ok=True)
    idx = a.index or os.path.join(a.out_dir, "index.scip")
    if not a.index:
        rc = cmd_index(argparse.Namespace(root=a.root, output=idx, features=a.features,
                                          no_default_features=a.no_default_features,
                                          all_features=a.all_features, verbose=a.verbose))
        if rc:
            return rc
    g = build(idx, test_globs=a.tests if a.tests else None, exclude_globs=a.exclude or [],
              exclude_symbols=a.exclude_symbols, granularity=a.granularity,
              owner_weight=a.owner_weight, impl_weight=a.impl_weight,
              packages=a.package or None, log=_log)
    _dump(g, os.path.join(a.out_dir, "graph.json"))
    resolutions = DEFAULT_RESOLUTIONS
    if a.resolution is not None and a.resolution not in resolutions:
        resolutions = sorted(resolutions + [a.resolution])
    rows = sweep(g, resolutions, a.weighting, a.seeds)
    res = a.resolution if a.resolution is not None else pick_resolution(rows)
    _dump({"rows": rows, "suggested": pick_resolution(rows)}, os.path.join(a.out_dir, "sweep.json"))
    cl = cluster(g, res, a.weighting, a.seeds)
    _dump(cl, os.path.join(a.out_dir, "clusters.json"))
    names_path = os.path.join(a.out_dir, "names.json")
    nm = None
    if a.carry_names:
        nm = names_mod.carry(_load(a.carry_names[0]), _load(a.carry_names[1]), cl, a.threshold)
        _dump(nm, names_path)
    elif not os.path.exists(names_path):
        _dump(names_mod.template(cl), names_path)
    else:
        nm = _load(names_path)
        _log(f"kept existing {names_path} (ids may be stale; use --carry-names to remap)")
    _write_text(markdown(cl, nm), os.path.join(a.out_dir, "report.md"))
    _log(f"resolution {res}: {len(cl['clusters'])} clusters, modularity {cl['modularity']:.3f}, "
         f"stability {cl['stability']:.3f}")
    if not a.index and not a.keep_index:
        os.unlink(idx)
    return 0


# ------------------------------------------------------------------ parser

def _graph_opts(p):
    """Options shared by `graph` and `analyze`."""
    p.add_argument("--granularity", choices=["item", "type", "file"], default="item",
                   help="node unit: items (default), types with their methods folded in, or files")
    p.add_argument("--tests", action="append", metavar="GLOB",
                   help="documents treated as tests: their references are recorded for coverage but "
                        "they are not part of the graph (repeatable; default: common test paths)")
    p.add_argument("--exclude", action="append", metavar="GLOB",
                   help="documents to ignore entirely (repeatable), e.g. 'src/generated/*'")
    p.add_argument("--exclude-symbols", default=None, metavar="REGEX",
                   help="drop items whose key matches (default drops inline test modules: "
                        "'(^|/)tests?(/|::|$)'; pass '' to keep them)")
    p.add_argument("--package", action="append", metavar="NAME",
                   help="restrict to these packages (default: every package defined in the index)")
    p.add_argument("--owner-weight", type=int, default=2, help="weight of method -> type edges (default 2)")
    p.add_argument("--impl-weight", type=int, default=1,
                   help="weight of trait-impl method -> trait method edges (default 1)")


def parser():
    ap = argparse.ArgumentParser(prog="refgraph", description=(
        "Build a reference graph from a SCIP index and find clusters of code that "
        "reference each other heavily and the rest rarely."))
    sub = ap.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("index", help="run rust-analyzer to produce a SCIP index")
    p.add_argument("root", nargs="?", default=".", help="Cargo project or workspace root")
    p.add_argument("-o", "--output", default="index.scip")
    p.add_argument("--features", nargs="+", metavar="F", help="cargo features to enable")
    p.add_argument("--all-features", action="store_true")
    p.add_argument("--no-default-features", action="store_true")
    p.add_argument("-v", "--verbose", action="store_true", help="show rust-analyzer output")
    p.set_defaults(fn=cmd_index)

    p = sub.add_parser("graph", help="SCIP index -> graph.json")
    p.add_argument("index")
    p.add_argument("-o", "--output", default="graph.json", help="'-' for stdout")
    _graph_opts(p)
    p.set_defaults(fn=cmd_graph)

    def cl_opts(p):
        p.add_argument("--weighting", choices=["log", "raw", "binary"], default="log",
                       help="edge weight from reference count: 1+ln(n) (default), n, or 1")
        p.add_argument("--seeds", type=int, default=30, help="Louvain runs per resolution (default 30)")

    p = sub.add_parser("sweep", help="try several resolutions and report modularity/stability")
    p.add_argument("graph")
    p.add_argument("-r", "--resolutions", type=float, nargs="+")
    p.add_argument("--json", action="store_true", help="machine-readable output")
    cl_opts(p)
    p.set_defaults(fn=cmd_sweep)

    p = sub.add_parser("cluster", help="graph.json -> clusters.json")
    p.add_argument("graph")
    p.add_argument("-r", "--resolution", type=float, default=1.0)
    p.add_argument("-o", "--output", default="clusters.json", help="'-' for stdout")
    cl_opts(p)
    p.set_defaults(fn=cmd_cluster)

    p = sub.add_parser("report", help="clusters.json -> Markdown report")
    p.add_argument("clusters")
    p.add_argument("--names", help="names file to label clusters with")
    p.add_argument("--members", type=int, default=25, help="members listed per cluster (0 for none)")
    p.add_argument("-o", "--output", default="-")
    p.set_defaults(fn=cmd_report)

    p = sub.add_parser("names", help="write a names template, or carry names over from an earlier run")
    p.add_argument("clusters")
    p.add_argument("--carry-from", nargs=2, metavar=("OLD_CLUSTERS", "OLD_NAMES"))
    p.add_argument("--threshold", type=float, default=0.5,
                   help="minimum member overlap (Jaccard) to carry a name (default 0.5)")
    p.add_argument("-o", "--output", default="-")
    p.set_defaults(fn=cmd_names)

    p = sub.add_parser("where", help="find which cluster an item or file belongs to")
    p.add_argument("clusters")
    p.add_argument("query", help="case-insensitive substring of an item key or file path")
    p.add_argument("--names")
    p.add_argument("--limit", type=int, default=50,
                   help="at most this many file lines and this many item lines (default 50 each)")
    p.set_defaults(fn=cmd_where)

    p = sub.add_parser("analyze", help="index + graph + sweep + cluster + report in one go")
    p.add_argument("root", nargs="?", default=".")
    p.add_argument("--out-dir", default="refgraph-out")
    p.add_argument("--index", help="use an existing SCIP index instead of running rust-analyzer")
    p.add_argument("--keep-index", action="store_true", help="keep the generated index.scip")
    p.add_argument("--features", nargs="+", metavar="F", help="cargo features to enable")
    p.add_argument("--all-features", action="store_true")
    p.add_argument("--no-default-features", action="store_true")
    p.add_argument("-v", "--verbose", action="store_true", help="show rust-analyzer output")
    p.add_argument("-r", "--resolution", type=float,
                   help="skip auto-selection (the sweep still runs and includes this resolution)")
    p.add_argument("--carry-names", nargs=2, metavar=("OLD_CLUSTERS", "OLD_NAMES"),
                   help="map names from an earlier run onto the new clusters")
    p.add_argument("--threshold", type=float, default=0.5,
                   help="minimum member overlap (Jaccard) for --carry-names (default 0.5)")
    _graph_opts(p)
    cl_opts(p)
    p.set_defaults(fn=cmd_analyze)
    return ap


def main(argv=None):
    a = parser().parse_args(argv)
    if getattr(a, "exclude_symbols", 0) is None:
        from .graph import DEFAULT_EXCLUDE_SYMBOLS
        a.exclude_symbols = DEFAULT_EXCLUDE_SYMBOLS
    if getattr(a, "exclude_symbols", None):
        try:
            re.compile(a.exclude_symbols)
        except re.error as e:
            _log(f"refgraph: invalid --exclude-symbols regex {a.exclude_symbols!r}: {e}")
            return 2
    try:
        return a.fn(a) or 0
    except BrokenPipeError:
        return 0
    except OSError as e:  # missing or unreadable input, unwritable output
        _log(f"refgraph: {e.filename}: {e.strerror}" if e.filename else f"refgraph: {e}")
        return 2
