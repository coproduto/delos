"""Markdown report of a clusters.json (optionally with a names file)."""
from .names import lookup


def _label(cid, names):
    n = lookup(names, cid)
    return f"C{cid} {n}" if n else f"C{cid}"


def _short(path):
    return path[4:] if path.startswith("src/") else path


def markdown(cl, names=None, members=25, bridges=15, seams=15):
    t = cl["totals"]
    p = cl["params"]
    out = [
        "# Reference-graph clusters",
        "",
        f"- Graph: {t['nodes']} nodes, {t['edges']} undirected edges, {t['refs']} references "
        f"({cl['graph_meta'].get('granularity', 'item')} granularity; packages: "
        f"{', '.join(cl['graph_meta'].get('packages', [])) or '?'})",
        f"- Louvain: resolution {p['resolution']}, weighting `{p['weighting']}`, best of {p['seeds']} seeds",
        f"- {len(cl['clusters'])} clusters, modularity {cl['modularity']:.3f}, stability {cl['stability']:.3f}",
        "",
        "## Partition vs. file layout",
        "",
        "| Partition | Groups | Modularity | Refs kept inside |",
        "|---|---:|---:|---:|",
    ]
    for k, v in cl["layout_comparison"].items():
        out.append(f"| {k} | {v['groups']} | {v['modularity']:.3f} | {v['internal']:.0%} |")
    out += ["", "## Clusters", ""]
    for c in cl["clusters"]:
        cid = c["id"]
        entry = (names or {}).get("clusters", {}).get(str(cid), {})
        out.append(f"### {_label(cid, names)}")
        out.append("")
        if entry.get("tags"):
            out.append("Tags: " + ", ".join(f"`{x}`" for x in entry["tags"]))
            out.append("")
        if entry.get("summary"):
            out.append(entry["summary"])
            out.append("")
        out.append(f"- {c['size']} items, ~{c['lines']} lines, cohesion {c['cohesion']:.2f} "
                   f"({c['internal_refs']} internal / {c['external_refs']} external refs)")
        out.append("- Files: " + ", ".join(f"{_short(f)} ({n})" for f, n in c["files"][:8])
                   + (" …" if len(c["files"]) > 8 else ""))
        out.append("- Hubs: " + ", ".join(f"`{h}`" for h, _ in c["hubs"][:10]))
        if c["interface"]:
            out.append("- Interface (most referenced from outside): "
                       + ", ".join(f"`{h}` ({w})" for h, w in c["interface"][:8]))
        if c["neighbors"]:
            out.append("- Neighbours: " + ", ".join(f"{_label(n, names)} ({w})" for n, w in c["neighbors"][:6]))
        if members:
            shown = c["members"][:members]
            more = len(c["members"]) - len(shown)
            out.append("- Members: " + ", ".join(f"`{m}`" for m in shown) + (f" … (+{more})" if more > 0 else ""))
        out.append("")
    out += ["## Seams (references between clusters)", "", "| Between | Refs |", "|---|---:|"]
    for a, b, w in cl["between"][:seams]:
        out.append(f"| {_label(a, names)} ↔ {_label(b, names)} | {w} |")
    out += ["", "## Bridge items (links spread over many clusters)", "",
            "| Item | Home | Refs | Clusters touched | Participation |", "|---|---|---:|---:|---:|"]
    for b in cl["bridges"][:bridges]:
        out.append(f"| `{b['item']}` | {_label(b['cluster'], names)} | {b['refs']} | "
                   f"{b['clusters_touched']} | {b['participation']:.2f} |")
    out += ["", "## Files split across clusters", "", "| File | Items per cluster |", "|---|---|"]
    for f, split in cl["file_split"].items():
        if len(split) > 1:
            out.append(f"| {_short(f)} | " + " · ".join(f"C{c} {n}" for c, n in split) + " |")
    if cl["tests"]:
        out += ["", "## Test references per cluster", "", "| Cluster | Refs | Test files |", "|---|---:|---|"]
        per = {}
        for t, cs in cl["tests"].items():
            for c, w in cs:
                per.setdefault(c, []).append((w, t))
        for c in cl["clusters"]:
            rows = sorted(per.get(c["id"], []), reverse=True)
            out.append(f"| {_label(c['id'], names)} | {sum(w for w, _ in rows)} | "
                       + (", ".join(f"{t} ({w})" for w, t in rows[:4]) or "—") + " |")
    return "\n".join(out) + "\n"
