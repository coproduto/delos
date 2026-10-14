"""Build an item-level reference graph for the delos crate from a SCIP index.

Nodes are top-level items: free functions, types (struct/enum/trait/alias,
with their fields and variants folded in), constants/statics, macros, and
methods (as `Type::method`). Edges are counted references from the body of
one item to another.
"""
import json
import re
import sys
from collections import Counter, defaultdict

import scip

P = "rust-analyzer cargo delos 0.1.0 "
IDENT = re.compile(r"`[^`]*`|[A-Za-z_][A-Za-z0-9_]*")

STD_TRAITS = {"Display", "Debug", "Clone", "PartialEq", "Eq", "Hash", "Default",
              "Drop", "From", "Into", "Error", "Copy", "PartialOrd", "Ord",
              "Deref", "DerefMut", "Iterator", "AsRef", "FromStr"}


def split_brackets(s, i):
    """s[i] == '['; return (content, index after matching ']')."""
    depth, j = 0, i
    while True:
        c = s[j]
        if c == "`":
            j = s.index("`", j + 1)
        elif c == "[":
            depth += 1
        elif c == "]":
            depth -= 1
            if depth == 0:
                return s[i + 1:j], j + 1
        j += 1


def base_name(t):
    t = t.strip("`")
    return re.split(r"[<\s]", t)[0]


def parse(sym):
    """Return (node_key, owner_key, impl_trait) or None for a crate symbol."""
    if not sym.startswith(P):
        return None
    d = sym[len(P):]
    # module path
    mods = []
    while True:
        m = re.match(r"([A-Za-z_][A-Za-z0-9_]*)/", d)
        if not m:
            break
        mods.append(m.group(1))
        d = d[m.end():]
    if not d:
        return None  # a module
    mod = "/".join(mods)
    pre = mod + "/" if mod else ""
    if d.startswith("impl#"):
        ty, j = split_brackets(d, 5)
        trait = None
        if j < len(d) and d[j] == "[":
            trait, j = split_brackets(d, j)
            trait = base_name(trait)
        ty = base_name(ty)
        m = IDENT.match(d, j)
        if not m:
            return (pre + ty, None, trait)
        meth = m.group(0)
        if trait in STD_TRAITS or (trait and trait.startswith("PartialEq")):
            return (pre + ty, None, trait)
        return (f"{pre}{ty}::{meth}", pre + ty, trait)
    m = IDENT.match(d)
    if not m:
        return None
    name = m.group(0)
    rest = d[m.end():]
    if rest.startswith("#"):
        m2 = re.match(r"#([A-Za-z_][A-Za-z0-9_]*)\(\)\.", rest)
        if m2:  # trait method declaration
            return (f"{pre}{name}::{m2.group(1)}", pre + name, None)
        return (pre + name, None, None)
    return (pre + name, None, None)


def contains(outer, inner):
    def norm(r):
        return (r[0], r[1], r[0], r[2]) if len(r) == 3 else tuple(r)
    a, b = norm(outer), norm(inner)
    return (a[0], a[1]) <= (b[0], b[1]) and (b[2], b[3]) <= (a[2], a[3])


def area(r):
    r = (r[0], r[1], r[0], r[2]) if len(r) == 3 else r
    return (r[2] - r[0]) * 10000 + (r[3] - r[1])


def main(index, out):
    docs = scip.load(index)
    nodes = {}          # key -> {file, kind, lines}
    kinds = {}
    for d in docs:
        for s in d["syms"]:
            kinds[s["symbol"]] = s["kind"]
    edges = Counter()   # (src, dst) -> count
    impl_edges = Counter()
    test_refs = defaultdict(Counter)  # test file -> node -> count
    for d in docs:
        path = d["path"]
        is_test = path.startswith("tests/")
        defs = []
        for o in d["occ"]:
            if o["roles"] & 1 and o["enclosing"]:
                p = parse(o["symbol"])
                if p is None:
                    continue
                key = p[0]
                if "tests/" in key or key.startswith("tests"):
                    key = None
                defs.append((o["enclosing"], key, p, o["symbol"]))
                if key and not is_test:
                    r = o["enclosing"]
                    n = nodes.setdefault(key, {"file": path, "lines": 0,
                                               "kind": kinds.get(o["symbol"], 0),
                                               "owner": p[1]})
                    span = (r[2] - r[0] + 1) if len(r) == 4 else 1
                    # only count the span of the item itself, not fields
                    if parse(o["symbol"])[0] == key and o["symbol"].endswith(
                            ("().", "#", ".", "!")):
                        n["lines"] = max(n["lines"], span)
        for o in d["occ"]:
            if o["roles"] & 1:
                continue
            tgt = parse(o["symbol"])
            if tgt is None:
                continue
            tkey = tgt[0]
            if "tests/" in tkey:
                continue
            best = None
            for r, key, p, s in defs:
                if contains(r, o["range"]) and (best is None or area(r) < area(best[0])):
                    best = (r, key, p, s)
            if best is None or best[1] is None:
                continue
            skey = best[1]
            if is_test:
                test_refs[path][tkey] += 1
                continue
            if skey != tkey:
                edges[(skey, tkey)] += 1
    # owner edges and trait-impl edges
    for key, n in nodes.items():
        if n["owner"]:
            edges[(key, n["owner"])] += 2
    # trait impl method -> trait method decl
    by_name = defaultdict(list)
    for k in nodes:
        by_name[k.rsplit("/", 1)[-1]].append(k)
    for d in docs:
        for s in d["syms"]:
            p = parse(s["symbol"])
            if p and p[2] and p[2] not in STD_TRAITS and "::" in p[0]:
                meth = p[0].split("::")[-1]
                for cand in by_name.get(f"{p[2]}::{meth}", []):
                    impl_edges[(p[0], cand)] += 1
    for k, v in impl_edges.items():
        edges[k] += v
    edges = {k: v for k, v in edges.items() if k[0] in nodes and k[1] in nodes}
    json.dump({
        "nodes": nodes,
        "edges": [[a, b, w] for (a, b), w in sorted(edges.items())],
        "tests": {t: dict(c) for t, c in test_refs.items()},
    }, open(out, "w"), indent=1)
    print(len(nodes), "nodes", len(edges), "edges", sum(edges.values()), "refs")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
