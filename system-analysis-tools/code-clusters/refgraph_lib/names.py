"""Cluster names: templates for naming, and carrying names across re-runs.

A names file looks like:

    {"format": "refgraph.names/1",
     "clusters": {"0": {"name": "...", "region": "...", "tags": [...], "summary": "..."}}}

Cluster ids are only meaningful for one clusters.json, so when the code
changes, `carry` maps old names onto the new clusters by member overlap.
"""
FORMAT = "refgraph.names/1"


def _jaccard(a, b):
    a, b = set(a), set(b)
    return len(a & b) / len(a | b) if a | b else 0.0


def template(clusters, hints=8):
    out = {}
    for c in clusters["clusters"]:
        out[str(c["id"])] = {
            "name": "",
            "region": "",
            "tags": [],
            "summary": "",
            "_hint": {
                "size": c["size"],
                "cohesion": c["cohesion"],
                "files": [f"{f} ({n})" for f, n in c["files"][:6]],
                "hubs": [h for h, _ in c["hubs"][:hints]],
            },
        }
    return {"format": FORMAT, "clusters": out}


def carry(old_clusters, old_names, new_clusters, threshold=0.5):
    """Names for new_clusters, copied from the best-overlapping old cluster.

    Clusters whose best overlap is below `threshold` are left unnamed (with a
    hint), so a human or agent decides whether the old name still fits.
    """
    old = {str(c["id"]): c["members"] for c in old_clusters["clusters"]}
    names = old_names.get("clusters", {})
    tpl = template(new_clusters)
    for c in new_clusters["clusters"]:
        cid = str(c["id"])
        best, j = None, 0.0
        for oid, members in old.items():
            s = _jaccard(c["members"], members)
            if s > j:
                best, j = oid, s
        entry = tpl["clusters"][cid]
        entry["_hint"]["closest_old"] = {"id": best, "overlap": round(j, 3),
                                         "name": names.get(best, {}).get("name", "")}
        if best is not None and j >= threshold and best in names:
            for k in ("name", "region", "tags", "summary"):
                if k in names[best]:
                    entry[k] = names[best][k]
    return tpl


def lookup(names, cid):
    if not names:
        return ""
    return names.get("clusters", {}).get(str(cid), {}).get("name", "")
