"""Louvain clustering of a refgraph graph, with cluster metrics.

Requires networkx (imported lazily so the other commands work without it).
"""
import math
from collections import Counter, defaultdict

FORMAT = "refgraph.clusters/1"
DEFAULT_RESOLUTIONS = [0.4, 0.6, 0.8, 1.0, 1.2, 1.5, 2.0]


def _nx():
    try:
        import networkx as nx
        from networkx.algorithms.community import louvain_communities, modularity
    except ImportError as e:  # pragma: no cover
        raise SystemExit("refgraph: clustering needs networkx. Run through `uv run` "
                         "(the launcher declares it) or `pip install networkx`.") from e
    return nx, louvain_communities, modularity


def weight_fn(mode):
    if mode == "log":
        return lambda w: 1 + math.log(w)
    if mode == "raw":
        return float
    if mode == "binary":
        return lambda w: 1.0
    raise ValueError(f"unknown weighting {mode!r}")


def to_nx(graph, weighting="log"):
    """Undirected graph; `raw` holds summed reference counts, `weight` the transformed one."""
    nx, _, _ = _nx()
    f = weight_fn(weighting)
    G = nx.Graph()
    for k, v in graph["nodes"].items():
        G.add_node(k, **{a: v.get(a) for a in ("file", "lines")})
    raw = Counter()
    for a, b, w in graph["edges"]:
        if a != b:
            raw[tuple(sorted((a, b)))] += w
    for (a, b), w in raw.items():
        G.add_edge(a, b, raw=w, weight=f(w))
    return G


def run(G, resolution, seeds=30):
    """Best-modularity partition over `seeds` Louvain runs, plus its stability.

    Stability is the mean, over edges, of the fraction of runs that agree with
    the majority on whether the edge's endpoints share a cluster.
    """
    _, louvain, modularity = _nx()
    parts = [louvain(G, weight="weight", resolution=resolution, seed=s) for s in range(seeds)]
    together = Counter()
    for p in parts:
        m = {n: i for i, c in enumerate(p) for n in c}
        for a, b in G.edges():
            if m[a] == m[b]:
                together[(a, b)] += 1
    # Keep the most modular run. Standard (resolution 1) modularity is used both
    # to choose and to report, so values compare across resolutions.
    q, best = max(((modularity(G, p, weight="weight"), p) for p in parts), key=lambda x: x[0])
    ne = G.number_of_edges()
    stab = (sum(max(together[e], seeds - together[e]) for e in G.edges()) / (seeds * ne)) if ne else 1.0
    best = sorted((sorted(c) for c in best), key=lambda c: (-len(c), c[0]))
    return best, q, stab


def sweep(graph, resolutions=None, weighting="log", seeds=30):
    G = to_nx(graph, weighting)
    rows = []
    for r in resolutions or DEFAULT_RESOLUTIONS:
        p, q, s = run(G, r, seeds)
        rows.append({"resolution": r, "clusters": len(p), "modularity": round(q, 4),
                     "stability": round(s, 4), "sizes": [len(c) for c in p]})
    return rows


def pick_resolution(rows):
    """Highest modularity (to 3 decimals); ties go to the resolution nearest 1.0."""
    return max(rows, key=lambda r: (round(r["modularity"], 3), -abs(r["resolution"] - 1.0)))["resolution"]


def layout_partitions(graph):
    """Partitions implied by the file layout.

    `files`: one group per file. `dirs`: one group per parent directory.
    `prefix@N`: one group per first N path components (a file shallower than N
    is its own group). Partitions with a single group, or identical to one
    already listed, are skipped.
    """
    files = {k: v["file"] for k, v in graph["nodes"].items()}
    cands = [("files", files),
             ("dirs", {k: f.rsplit("/", 1)[0] if "/" in f else "." for k, f in files.items()})]
    maxdepth = max((f.count("/") for f in files.values()), default=0)
    for dpt in range(1, maxdepth + 1):
        cands.append((f"prefix@{dpt}", {k: "/".join(f.split("/")[:dpt]) for k, f in files.items()}))
    out, seen = {}, set()
    for name, part in cands:
        sig = frozenset(frozenset(n for n in part if part[n] == g) for g in set(part.values()))
        if len(sig) < 2 or sig in seen:
            continue
        seen.add(sig)
        out[name] = part
    return out


def score(G, assign):
    """Modularity (resolution 1) and fraction of raw references kept inside groups."""
    _, _, modularity = _nx()
    groups = defaultdict(set)
    for n, g in assign.items():
        if n in G:
            groups[g].add(n)
    tot = inside = 0
    for a, b, d in G.edges(data=True):
        tot += d["raw"]
        if assign[a] == assign[b]:
            inside += d["raw"]
    return {"groups": len(groups), "modularity": round(modularity(G, groups.values(), weight="weight"), 4),
            "internal": round(inside / tot, 4) if tot else 1.0}


def cluster(graph, resolution=1.0, weighting="log", seeds=30, top=12):
    G = to_nx(graph, weighting)
    parts, q, stab = run(G, resolution, seeds)
    member = {n: i for i, c in enumerate(parts) for n in c}

    clusters = []
    for i, c in enumerate(parts):
        internal = external = 0
        nb, iface, deg = Counter(), Counter(), Counter()
        for a in c:
            for b, d in G[a].items():
                deg[a] += d["raw"]
                if member[b] == i:
                    internal += d["raw"]
                else:
                    external += d["raw"]
                    nb[member[b]] += d["raw"]
                    iface[a] += d["raw"]
        internal //= 2
        clusters.append({
            "id": i,
            "size": len(c),
            "lines": sum(G.nodes[n].get("lines") or 0 for n in c),
            "internal_refs": internal,
            "external_refs": external,
            "cohesion": round(internal / max(1, internal + external), 4),
            "files": Counter(G.nodes[n]["file"] for n in c).most_common(),
            "hubs": deg.most_common(top),
            "interface": iface.most_common(top),
            "neighbors": nb.most_common(),
            "members": c,
        })

    bridges = []
    for n in G:
        k = sum(d["raw"] for d in G[n].values())
        if k < 15:
            continue
        per = Counter()
        for b, d in G[n].items():
            per[member[b]] += d["raw"]
        pc = 1 - sum((v / k) ** 2 for v in per.values())
        bridges.append({"item": n, "cluster": member[n], "refs": k, "clusters_touched": len(per),
                        "participation": round(pc, 4)})
    bridges.sort(key=lambda x: -x["participation"] * math.log(x["refs"]))

    between = Counter()
    for a, b, d in G.edges(data=True):
        if member[a] != member[b]:
            between[tuple(sorted((member[a], member[b])))] += d["raw"]

    file_split = defaultdict(Counter)
    for n in G:
        file_split[G.nodes[n]["file"]][member[n]] += 1

    tests = {}
    for t, refs in sorted(graph.get("tests", {}).items()):
        c = Counter()
        for n, w in refs.items():
            if n in member:
                c[member[n]] += w
        tests[t] = c.most_common()

    assign = {n: member[n] for n in G}
    layout = {"clusters": score(G, assign)}
    for name, part in layout_partitions(graph).items():
        layout[name] = score(G, part)

    return {
        "format": FORMAT,
        "graph_meta": graph.get("meta", {}),
        "params": {"resolution": resolution, "weighting": weighting, "seeds": seeds},
        "modularity": round(q, 4),
        "stability": round(stab, 4),
        "totals": {"nodes": G.number_of_nodes(), "edges": G.number_of_edges(),
                   "refs": sum(d["raw"] for *_, d in G.edges(data=True))},
        "clusters": clusters,
        "between": [[a, b, w] for (a, b), w in between.most_common()],
        "bridges": bridges[:40],
        "file_split": {f: c.most_common() for f, c in sorted(file_split.items())},
        "tests": tests,
        "layout_comparison": layout,
    }
