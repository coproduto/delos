"""Cluster the item reference graph with Louvain and report cluster metrics."""
import json
import math
import sys
from collections import Counter, defaultdict

import networkx as nx
from networkx.algorithms.community import louvain_communities, modularity

g = json.load(open(sys.argv[1]))
res = float(sys.argv[2]) if len(sys.argv) > 2 else 1.0
mode = sys.argv[3] if len(sys.argv) > 3 else "log"

G = nx.Graph()
for k, v in g["nodes"].items():
    G.add_node(k, **v)
raw = Counter()
for a, b, w in g["edges"]:
    raw[tuple(sorted((a, b)))] += w
for (a, b), w in raw.items():
    G.add_edge(a, b, raw=w, weight=(1 + math.log(w)) if mode == "log" else w)


def run(resolution, seeds=30):
    parts = [louvain_communities(G, weight="weight", resolution=resolution, seed=s)
             for s in range(seeds)]
    # consensus: co-assignment frequency
    co = Counter()
    for p in parts:
        for c in p:
            cl = sorted(c)
            for i, a in enumerate(cl):
                for b in cl[i + 1:]:
                    co[(a, b)] += 1
    best = max(parts, key=lambda p: modularity(G, p, weight="weight"))
    # stability: mean fraction of edges whose endpoints co-assigned consistently
    stab = sum(max(co[tuple(sorted((a, b)))], seeds - co[tuple(sorted((a, b)))])
               for a, b in G.edges()) / (seeds * G.number_of_edges())
    return best, modularity(G, best, weight="weight"), stab


if __name__ == "__main__":
    if res == 0:
        for r in [0.4, 0.6, 0.8, 1.0, 1.2, 1.5, 2.0]:
            p, q, s = run(r)
            sizes = sorted((len(c) for c in p), reverse=True)
            print(f"res={r} clusters={len(p)} Q={q:.3f} stability={s:.3f} sizes={sizes[:15]}")
        sys.exit()
    p, q, s = run(res)
    p = sorted(p, key=len, reverse=True)
    member = {n: i for i, c in enumerate(p) for n in c}
    out = {"resolution": res, "modularity": q, "stability": s, "clusters": []}
    for i, c in enumerate(p):
        internal = external = 0
        nb = Counter()
        for a in c:
            for b, d in G[a].items():
                if member[b] == i:
                    internal += d["raw"]
                else:
                    external += d["raw"]
                    nb[member[b]] += d["raw"]
        internal //= 2
        files = Counter(G.nodes[n]["file"] for n in c)
        deg = Counter({n: sum(d["raw"] for d in G[n].values()) for n in c})
        # nodes with most edges leaving the cluster: the cluster's interface
        iface = Counter()
        for a in c:
            for b, d in G[a].items():
                if member[b] != i:
                    iface[a] += d["raw"]
        out["clusters"].append({
            "id": i, "size": len(c),
            "lines": sum(G.nodes[n]["lines"] for n in c),
            "internal": internal, "external": external,
            "cohesion": internal / max(1, internal + external),
            "files": files.most_common(),
            "hubs": deg.most_common(12),
            "interface": iface.most_common(10),
            "neighbors": nb.most_common(6),
            "members": sorted(c),
        })
    # participation coefficient: how evenly a node's links spread over clusters
    pc = {}
    for n in G:
        k = sum(d["raw"] for d in G[n].values())
        if k < 15:
            continue
        per = Counter()
        for b, d in G[n].items():
            per[member[b]] += d["raw"]
        pc[n] = (1 - sum((v / k) ** 2 for v in per.values()), k, len(per))
    out["bridges"] = sorted(pc.items(), key=lambda x: -x[1][0] * math.log(x[1][1]))[:25]
    # file-level view: how many clusters each file is split across
    fsplit = defaultdict(Counter)
    for n in G:
        fsplit[G.nodes[n]["file"]][member[n]] += 1
    out["file_split"] = {f: c.most_common() for f, c in sorted(fsplit.items())}
    # test file -> clusters exercised
    tc = {}
    for t, refs in g["tests"].items():
        c = Counter()
        for n, w in refs.items():
            if n in member:
                c[member[n]] += w
        tc[t] = c.most_common()
    out["tests"] = tc
    # cluster-to-cluster matrix
    mat = Counter()
    for a, b, d in G.edges(data=True):
        if member[a] != member[b]:
            mat[tuple(sorted((member[a], member[b])))] += d["raw"]
    out["between"] = [[a, b, w] for (a, b), w in mat.most_common()]
    json.dump(out, open(sys.argv[4] if len(sys.argv) > 4 else "clusters.json", "w"), indent=1)
    print(f"Q={q:.3f} stability={s:.3f}")
    for c in out["clusters"]:
        print(f"\n#{c['id']} size={c['size']} lines={c['lines']} cohesion={c['cohesion']:.2f} "
              f"int={c['internal']} ext={c['external']} nb={c['neighbors']}")
        print("  files:", c["files"])
        print("  hubs:", [h for h, _ in c["hubs"]])
