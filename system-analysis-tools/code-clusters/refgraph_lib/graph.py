"""Build a weighted reference graph from a SCIP index.

Each reference occurrence inside the body of item A to item B adds 1 to the
directed edge A -> B. References outside any item (imports, module-level
attributes) are ignored. Two structural edges are added: method -> owning
type, and trait-impl method -> trait method declaration.
"""
import fnmatch
import re
from collections import Counter, defaultdict

from . import scip
from .symbols import item_for, parse_descriptors, split_symbol

FORMAT = "refgraph.graph/1"

# fnmatch's `*` also matches `/`, so these cover nested paths.
DEFAULT_TEST_GLOBS = ["tests/*", "*/tests/*", "test/*", "*/test/*", "*_test.*",
                      "test_*.*", "*/test_*.*", "*.test.*", "*.spec.*"]
DEFAULT_EXCLUDE_SYMBOLS = r"(^|/)tests?(/|::|$)"


def _last(key):
    """Item name without module path or package prefix: `pkg:a/b/T::m` -> `T::m`."""
    head, sep, tail = key.rsplit("/", 1)[-1].partition("::")
    return head.split(":", 1)[-1] + sep + tail


def _match(path, globs):
    return any(fnmatch.fnmatch(path, g) for g in globs)


def _innermost(defs, occs):
    """For each occurrence (sorted by start), yield (occ, innermost containing def).

    `defs` are (range, payload) sorted by (start asc, end desc); ranges are
    (sl, sc, el, ec). Uses a stack sweep, so the cost is O((D + O) log).
    """
    stack, i = [], 0
    for o in occs:
        r = o["range"]
        start, end = (r[0], r[1]), (r[2], r[3])
        while i < len(defs) and (defs[i][0][0], defs[i][0][1]) <= start:
            d = defs[i]
            ds = (d[0][0], d[0][1])
            while stack and (stack[-1][0][2], stack[-1][0][3]) < ds:
                stack.pop()
            stack.append(d)
            i += 1
        while stack and (stack[-1][0][2], stack[-1][0][3]) < start:
            stack.pop()
        best = None
        for d in reversed(stack):
            if (d[0][2], d[0][3]) >= end:
                best = d
                break
        yield o, best


def build(index_path, *, test_globs=None, exclude_globs=(), exclude_symbols=DEFAULT_EXCLUDE_SYMBOLS,
          granularity="item", owner_weight=2, impl_weight=1, packages=None, log=None):
    test_globs = DEFAULT_TEST_GLOBS if test_globs is None else test_globs
    excl_re = re.compile(exclude_symbols) if exclude_symbols else None
    docs = [d for d in scip.load(index_path) if not _match(d["path"], exclude_globs)]

    parsed = {}

    def split(sym):
        if sym not in parsed:
            s = split_symbol(sym)
            if s is None:
                parsed[sym] = None
            else:
                try:
                    parsed[sym] = (s[0], parse_descriptors(s[1]))
                except (ValueError, KeyError, IndexError):
                    parsed[sym] = None
        return parsed[sym]

    # Pass 1: which packages are "the project" (they define symbols in these docs),
    # which type names they define (decides trait-impl folding) and where (resolves impl self types).
    defined_pkgs = Counter()
    for d in docs:
        for o in d["occurrences"]:
            if o["roles"] & scip.DEFINITION_ROLE:
                s = split(o["symbol"])
                if s:
                    defined_pkgs[s[0]] += 1
    if packages:
        project = {p for p in defined_pkgs if p[1] in packages}
    else:
        project = set(defined_pkgs)
    multi = len({p[1] for p in project}) > 1
    project_types = set()
    type_keys = defaultdict(lambda: defaultdict(set))  # package -> type name -> keys
    for sym, s in parsed.items():
        if s and s[0] in project:
            descs = s[1]
            for i, (k, n) in enumerate(descs):
                if k == "namespace":
                    continue
                if k == "type" and n != "impl":
                    project_types.add(n)
                    type_keys[s[0]][n].add("".join(m + "/" for _, m in descs[:i]) + n)
                break

    kinds = {}
    for d in docs:
        for s in d["symbols"]:
            kinds[s["symbol"]] = s["kind"]

    items = {}

    def item(sym):
        if sym in items:
            return items[sym]
        s = split(sym)
        it = None
        if s and s[0] in project:
            it = item_for(s[1], project_types, type_keys[s[0]])
            if it is not None:
                if multi:
                    pfx = s[0][1] + ":"
                    it.key = pfx + it.key
                    it.owner = pfx + it.owner if it.owner else None
                if excl_re and excl_re.search(it.key):
                    it = False  # excluded: blocks attribution, never a node
        items[sym] = it
        return it

    nodes = {}
    edges = Counter()
    test_refs = defaultdict(Counter)
    no_enclosing = 0
    for d in docs:
        path = d["path"]
        is_test = _match(path, test_globs)
        defs = []
        for o in d["occurrences"]:
            if not o["roles"] & scip.DEFINITION_ROLE:
                continue
            it = item(o["symbol"])
            if it is None:
                continue
            if o["enclosing"] is None:
                no_enclosing += 1
                continue
            defs.append((o["enclosing"], it))
            if it and not is_test:
                n = nodes.setdefault(it.key, {"file": path, "kind": kinds.get(o["symbol"], 0),
                                              "lines": 0, "owner": it.owner, "trait": it.trait})
                if it.exact:
                    r = o["enclosing"]
                    n["lines"] = max(n["lines"], r[2] - r[0] + 1)
                    n["kind"] = kinds.get(o["symbol"], n["kind"])
        defs.sort(key=lambda x: (x[0][0], x[0][1], -x[0][2], -x[0][3]))
        refs = [o for o in d["occurrences"]
                if not o["roles"] & scip.DEFINITION_ROLE and o["range"]]
        refs.sort(key=lambda o: o["range"])
        for o, best in _innermost(defs, refs):
            if best is None or not best[1]:
                continue
            tgt = item(o["symbol"])
            if not tgt:
                continue
            src = best[1].key
            if is_test:
                test_refs[path][tgt.key] += 1
            elif src != tgt.key:
                edges[(src, tgt.key)] += 1

    # Structural edges. Owners are resolved by key, then by unique simple name
    # (an impl block may live in a different module than its type).
    by_simple = defaultdict(list)
    for k in nodes:
        if "::" not in _last(k):
            by_simple[_last(k)].append(k)
    for k, n in nodes.items():
        o = n["owner"]
        if not o:
            continue
        if o not in nodes:
            cands = by_simple.get(_last(o), [])
            o = cands[0] if len(cands) == 1 else None
            n["owner"] = o
        if o and owner_weight:
            edges[(k, o)] += owner_weight
    if impl_weight:
        decl = defaultdict(list)  # "Trait::method" -> keys
        for k in nodes:
            decl[_last(k)].append(k)
        for k, n in nodes.items():
            if n["trait"]:
                meth = k.rsplit("::", 1)[-1]
                for cand in decl.get(f"{n['trait']}::{meth}", []):
                    if cand != k:
                        edges[(k, cand)] += impl_weight

    edges = {e: w for e, w in edges.items() if e[0] in nodes and e[1] in nodes}
    tests = {t: {k: v for k, v in c.items() if k in nodes} for t, c in test_refs.items()}

    if granularity != "item":
        nodes, edges, tests = _coarsen(nodes, edges, tests, granularity)

    if log and no_enclosing:
        log(f"warning: {no_enclosing} definitions had no enclosing range and were skipped; "
            "the indexer may not emit enclosing_range (references cannot be attributed without it)")
    return {
        "format": FORMAT,
        "meta": {
            "index": str(index_path),
            "packages": sorted(f"{m} {n} {v}" for m, n, v in project),
            "granularity": granularity,
            "test_globs": list(test_globs),
            "exclude_globs": list(exclude_globs),
            "exclude_symbols": exclude_symbols,
            "owner_weight": owner_weight,
            "impl_weight": impl_weight,
        },
        "nodes": dict(sorted(nodes.items())),
        "edges": [[a, b, w] for (a, b), w in sorted(edges.items())],
        "tests": tests,
    }


def _coarsen(nodes, edges, tests, granularity):
    if granularity == "type":
        def to(k):
            return nodes[k]["owner"] or k
    elif granularity == "file":
        def to(k):
            return nodes[k]["file"]
    else:
        raise ValueError(f"unknown granularity {granularity!r}")
    new = {}
    for k, n in nodes.items():
        t = to(k)
        m = new.setdefault(t, {"file": n["file"], "kind": n["kind"] if t == k else 0,
                               "lines": 0, "owner": None, "trait": None, "items": 0})
        m["lines"] += n["lines"]
        m["items"] += 1
        if t == k:
            m["kind"] = n["kind"]
    ne = Counter()
    for (a, b), w in edges.items():
        ta, tb = to(a), to(b)
        if ta != tb:
            ne[(ta, tb)] += w
    nt = {t: dict(sum((Counter({to(k): v}) for k, v in c.items()), Counter())) for t, c in tests.items()}
    return new, dict(ne), nt
