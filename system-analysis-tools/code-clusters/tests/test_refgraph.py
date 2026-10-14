"""Tests for refgraph. Run from the project root:

    uv run --quiet --with networkx python -m unittest discover -s system-analysis-tools/code-clusters/tests

(Clustering tests are skipped when networkx is unavailable.)
"""
import json
import os
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, ROOT)

from refgraph_lib import scip  # noqa: E402
from refgraph_lib.graph import build  # noqa: E402
from refgraph_lib.symbols import item_for, parse_descriptors, split_symbol  # noqa: E402

try:
    import networkx  # noqa: F401
    HAVE_NX = True
except ImportError:
    HAVE_NX = False


# ------------------------------------------------------- tiny SCIP encoder

def _varint(n):
    out = bytearray()
    while True:
        b = n & 0x7F
        n >>= 7
        if n:
            out.append(b | 0x80)
        else:
            out.append(b)
            return bytes(out)


def _field(no, payload):
    if isinstance(payload, int):
        return _varint(no << 3) + _varint(payload)
    if isinstance(payload, str):
        payload = payload.encode()
    return _varint(no << 3 | 2) + _varint(len(payload)) + payload


def _packed(no, ints):
    return _field(no, b"".join(_varint(i) for i in ints))


def occ(symbol, rng, definition=False, enclosing=None):
    b = _packed(1, rng) + _field(2, symbol)
    if definition:
        b += _field(3, 1)
    if enclosing:
        b += _packed(7, enclosing)
    return b


def doc(path, occurrences, symbols=()):
    b = _field(1, path)
    for o in occurrences:
        b += _field(2, o)
    for s, kind in symbols:
        b += _field(3, _field(1, s) + _field(5, kind))
    return b


def index(docs):
    return b"".join(_field(2, d) for d in docs)


P = "rust-analyzer cargo demo 0.1.0 "


def fixture():
    """A small crate:

    src/a.rs   struct Thing { x }   impl Thing { fn make() }   impl Display for Thing { fn fmt }
               trait Shape { fn area() }   impl Shape for Thing { fn area() }
               fn helper() -> uses Thing (twice) and Thing::make
               mod tests { fn t() uses helper }   (inline test module: excluded)
    src/b.rs   fn caller() uses helper, Thing::make
    tests/it.rs fn it() uses caller
    """
    a = doc("src/a.rs", [
        occ(P + "a/", [0, 0, 60, 0], True, [0, 0, 60, 0]),
        occ(P + "a/Thing#", [1, 7, 12], True, [1, 0, 3, 1]),
        occ(P + "a/Thing#x.", [2, 4, 5], True, [2, 4, 2, 10]),
        occ(P + "a/impl#[Thing]", [4, 5, 10], True, [4, 0, 8, 1]),
        occ(P + "a/impl#[Thing]make().", [5, 7, 11], True, [5, 4, 7, 5]),
        occ(P + "a/Thing#", [6, 8, 13]),
        occ(P + "a/impl#[Thing][Display]fmt().", [10, 7, 10], True, [10, 4, 12, 5]),
        occ(P + "a/Thing#x.", [11, 13, 14]),
        occ(P + "a/Shape#", [14, 6, 11], True, [14, 0, 16, 1]),
        occ(P + "a/Shape#area().", [15, 7, 11], True, [15, 4, 15, 20]),
        occ(P + "a/impl#[Thing][Shape]area().", [18, 7, 11], True, [18, 4, 18, 30]),
        occ(P + "a/helper().", [20, 3, 9], True, [20, 0, 25, 1]),
        occ(P + "a/Thing#", [21, 10, 15]),
        occ(P + "a/Thing#", [22, 10, 15]),
        occ(P + "a/impl#[Thing]make().", [23, 10, 14]),
        occ("rust-analyzer cargo std 1.0 vec/Vec#", [24, 4, 7]),
        occ("local 3", [24, 10, 11]),
        occ(P + "a/tests/", [30, 4, 9], True, [30, 0, 35, 1]),
        occ(P + "a/tests/t().", [31, 7, 8], True, [31, 4, 33, 5]),
        occ(P + "a/helper().", [32, 8, 14]),
    ], [(P + "a/Thing#", 49), (P + "a/helper().", 17)])
    b = doc("src/b.rs", [
        occ(P + "b/", [0, 0, 10, 0], True, [0, 0, 10, 0]),
        occ(P + "a/helper().", [0, 8, 14]),  # a `use` line: outside any item
        occ(P + "b/caller().", [2, 3, 9], True, [2, 0, 5, 1]),
        occ(P + "a/helper().", [3, 4, 10]),
        occ(P + "a/impl#[Thing]make().", [4, 4, 8]),
    ])
    t = doc("tests/it.rs", [
        occ(P + "it/it().", [0, 3, 5], True, [0, 0, 3, 1]),
        occ(P + "b/caller().", [1, 4, 10]),
    ])
    return index([a, b, t])


class SymbolTests(unittest.TestCase):
    def test_split_symbol(self):
        pkg, d = split_symbol("rust-analyzer cargo my  crate 1.0 a/B#")
        self.assertEqual(pkg, ("cargo", "my crate", "1.0"))
        self.assertEqual(d, "a/B#")
        self.assertIsNone(split_symbol("local 7"))

    def test_descriptors(self):
        self.assertEqual(parse_descriptors("a/b/impl#[`Foo<T>`][Bar]baz()."),
                         [("namespace", "a"), ("namespace", "b"), ("type", "impl"),
                          ("typeparam", "Foo<T>"), ("typeparam", "Bar"), ("method", "baz")])
        self.assertEqual(parse_descriptors("m/f(+1).(x)"),
                         [("namespace", "m"), ("method", "f"), ("param", "x")])

    def test_items(self):
        def key(d, types=frozenset({"Shape"})):
            it = item_for(parse_descriptors(d), types)
            return it and (it.key, it.owner, it.trait)
        self.assertEqual(key("a/T#f."), ("a/T", None, None))
        self.assertEqual(key("a/T#V#f."), ("a/T", None, None))
        self.assertEqual(key("a/impl#[T]new()."), ("a/T::new", "a/T", None))
        self.assertEqual(key("a/impl#[T][Display]fmt()."), ("a/T", None, None))
        self.assertEqual(key("a/impl#[T][Shape]area()."), ("a/T::area", "a/T", "Shape"))
        self.assertEqual(key("a/Shape#area()."), ("a/Shape::area", "a/Shape", None))
        self.assertEqual(key("a/f().g()."), ("a/f", None, None))
        self.assertIsNone(key("a/"))


class GraphTests(unittest.TestCase):
    def setUp(self):
        fd, self.path = tempfile.mkstemp(suffix=".scip")
        with os.fdopen(fd, "wb") as f:
            f.write(fixture())

    def tearDown(self):
        os.unlink(self.path)

    def edges(self, g):
        return {(a, b): w for a, b, w in g["edges"]}

    def test_decoder(self):
        docs = scip.load(self.path)
        self.assertEqual([d["path"] for d in docs], ["src/a.rs", "src/b.rs", "tests/it.rs"])
        self.assertEqual(docs[0]["occurrences"][1]["enclosing"], (1, 0, 3, 1))
        self.assertEqual(docs[0]["occurrences"][1]["range"], (1, 7, 1, 12))

    def test_item_graph(self):
        g = build(self.path)
        self.assertEqual(set(g["nodes"]), {"a/Thing", "a/Thing::make", "a/Shape", "a/Shape::area",
                                           "a/Thing::area", "a/helper", "b/caller"})
        e = self.edges(g)
        self.assertEqual(e[("a/helper", "a/Thing")], 2)          # counted per reference
        self.assertEqual(e[("a/helper", "a/Thing::make")], 1)
        self.assertEqual(e[("a/Thing::make", "a/Thing")], 1 + 2)  # reference + owner edge
        self.assertEqual(e[("a/Thing::area", "a/Shape::area")], 1)  # trait impl edge
        self.assertEqual(e[("b/caller", "a/helper")], 1)        # `use` line not counted
        self.assertNotIn(("a/Thing", "a/Thing"), e)              # Display::fmt folded, no self loop
        self.assertEqual(g["meta"]["packages"], ["cargo demo 0.1.0"])
        self.assertEqual(g["tests"], {"tests/it.rs": {"b/caller": 1}})
        self.assertEqual(g["nodes"]["a/helper"]["lines"], 6)
        self.assertEqual(g["nodes"]["a/helper"]["kind"], 17)

    def test_keep_inline_tests(self):
        g = build(self.path, exclude_symbols="")
        self.assertIn("a/tests/t", g["nodes"])
        self.assertEqual(self.edges(g)[("a/tests/t", "a/helper")], 1)

    def test_tests_as_code(self):
        g = build(self.path, test_globs=[])
        self.assertIn("it/it", g["nodes"])
        self.assertEqual(g["tests"], {})

    def test_granularity(self):
        g = build(self.path, granularity="type")
        self.assertEqual(set(g["nodes"]), {"a/Thing", "a/Shape", "a/helper", "b/caller"})
        self.assertEqual(self.edges(g)[("a/helper", "a/Thing")], 3)
        self.assertEqual(self.edges(g)[("a/Thing", "a/Shape")], 1)
        f = build(self.path, granularity="file")
        self.assertEqual(self.edges(f), {("src/b.rs", "src/a.rs"): 2})

    def test_exclude_glob(self):
        g = build(self.path, exclude_globs=["src/b.rs"])
        self.assertNotIn("b/caller", g["nodes"])

    def test_external_trait_impl_in_other_module(self):
        # Regression: `impl Display for Foo` written in module y, with Foo defined
        # in module x, created a second node `y/Foo` next to `x/Foo`.
        x = doc("src/x.rs", [
            occ(P + "x/Foo#", [0, 7, 10], True, [0, 0, 2, 1]),
            occ(P + "x/make().", [4, 3, 7], True, [4, 0, 6, 1]),
        ])
        y = doc("src/y.rs", [
            occ(P + "y/impl#[Foo]", [0, 5, 8], True, [0, 0, 9, 1]),
            occ(P + "y/impl#[Foo][Display]fmt().", [1, 7, 10], True, [1, 4, 4, 5]),
            occ(P + "x/make().", [2, 8, 12]),
            occ(P + "y/impl#[Foo]new().", [6, 7, 10], True, [6, 4, 8, 5]),
            occ(P + "x/Foo#", [7, 8, 11]),
            # A type the project does not define: falls back to the impl's module.
            occ(P + "y/impl#[Ext][Display]fmt().", [12, 7, 10], True, [12, 4, 14, 5]),
            occ(P + "x/make().", [13, 8, 12]),
        ])
        fd, path = tempfile.mkstemp(suffix=".scip")
        with os.fdopen(fd, "wb") as f:
            f.write(index([x, y]))
        try:
            g = build(path)
        finally:
            os.unlink(path)
        self.assertEqual(set(g["nodes"]), {"x/Foo", "x/make", "y/Foo::new", "y/Ext"})
        self.assertEqual(g["nodes"]["x/Foo"]["file"], "src/x.rs")
        self.assertEqual(g["nodes"]["y/Foo::new"]["owner"], "x/Foo")
        e = self.edges(g)
        self.assertEqual(e[("x/Foo", "x/make")], 1)       # fmt's reference, folded into x/Foo
        self.assertEqual(e[("y/Foo::new", "x/Foo")], 1 + 2)
        self.assertEqual(e[("y/Ext", "x/make")], 1)


def two_cliques():
    nodes = {f"{p}{i}": {"file": f"src/{p}.rs", "lines": 1, "owner": None, "trait": None, "kind": 0}
             for p in "xy" for i in range(5)}
    edges = [[f"{p}{i}", f"{p}{j}", 3] for p in "xy" for i in range(5) for j in range(i + 1, 5)]
    edges.append(["x0", "y0", 1])
    return {"format": "refgraph.graph/1", "meta": {}, "nodes": nodes, "edges": edges,
            "tests": {"tests/t.rs": {"x1": 2, "y2": 1}}}


@unittest.skipUnless(HAVE_NX, "networkx not installed")
class ClusterTests(unittest.TestCase):
    def test_two_cliques(self):
        from refgraph_lib.cluster import cluster
        cl = cluster(two_cliques(), seeds=5)
        self.assertEqual(sorted(sorted(c["members"]) for c in cl["clusters"]),
                         [[f"x{i}" for i in range(5)], [f"y{i}" for i in range(5)]])
        self.assertEqual(cl["between"], [[0, 1, 1]])
        self.assertEqual(cl["stability"], 1.0)
        self.assertEqual(cl["layout_comparison"]["clusters"]["internal"],
                         cl["layout_comparison"]["files"]["internal"])
        c0 = cl["clusters"][0]
        self.assertEqual((c0["internal_refs"], c0["external_refs"]), (30, 1))
        self.assertEqual(sum(w for _, w in cl["tests"]["tests/t.rs"]), 3)

    def test_names_carry(self):
        from refgraph_lib.cluster import cluster
        from refgraph_lib.names import carry
        old = cluster(two_cliques(), seeds=3)
        names = {"clusters": {str(c["id"]): {"name": c["members"][0][0].upper()} for c in old["clusters"]}}
        g = two_cliques()
        g["nodes"]["x5"] = dict(g["nodes"]["x0"])
        g["edges"] += [["x5", "x1", 3], ["x5", "x2", 3]]
        new = cluster(g, seeds=3)
        out = carry(old, names, new)
        got = {c["members"][0][0]: out["clusters"][str(c["id"])]["name"] for c in new["clusters"]}
        self.assertEqual(got, {"x": "X", "y": "Y"})


class CliTests(unittest.TestCase):
    def run_cli(self, *args):
        return subprocess.run([sys.executable, "-c",
                               "import sys; sys.path.insert(0, %r); from refgraph_lib.cli import main; "
                               "sys.exit(main())" % ROOT, *args],
                              capture_output=True, text=True)

    def test_graph_report_where(self):
        with tempfile.TemporaryDirectory() as d:
            idx = os.path.join(d, "i.scip")
            with open(idx, "wb") as f:
                f.write(fixture())
            r = self.run_cli("graph", idx, "-o", os.path.join(d, "g.json"))
            self.assertEqual(r.returncode, 0, r.stderr)
            self.assertIn("7 nodes", r.stderr)
            if not HAVE_NX:
                return
            r = self.run_cli("cluster", os.path.join(d, "g.json"), "-o", os.path.join(d, "c.json"), "--seeds", "3")
            self.assertEqual(r.returncode, 0, r.stderr)
            r = self.run_cli("report", os.path.join(d, "c.json"))
            self.assertIn("# Reference-graph clusters", r.stdout)
            r = self.run_cli("where", os.path.join(d, "c.json"), "helper")
            self.assertRegex(r.stdout, r"a/helper\s+->\s+C\d")
            r = self.run_cli("where", os.path.join(d, "c.json"), "nothing-matches")
            self.assertEqual(r.returncode, 1)
            r = self.run_cli("names", os.path.join(d, "c.json"))
            self.assertEqual(json.loads(r.stdout)["format"], "refgraph.names/1")

    def write_fixture(self, d):
        idx = os.path.join(d, "i.scip")
        with open(idx, "wb") as f:
            f.write(fixture())
        return idx

    @unittest.skipUnless(HAVE_NX, "networkx not installed")
    def test_analyze_applies_existing_names(self):
        # Regression: an existing OUT_DIR/names.json was kept but not used in report.md.
        with tempfile.TemporaryDirectory() as d:
            idx, out = self.write_fixture(d), os.path.join(d, "out")
            args = ("analyze", "--index", idx, "--out-dir", out, "--seeds", "3")
            r = self.run_cli(*args)
            self.assertEqual(r.returncode, 0, r.stderr)
            names_path = os.path.join(out, "names.json")
            with open(names_path) as f:
                names = json.load(f)
            for cid, entry in names["clusters"].items():
                entry["name"] = f"Named{cid}"
            with open(names_path, "w") as f:
                json.dump(names, f)
            r = self.run_cli(*args)
            self.assertEqual(r.returncode, 0, r.stderr)
            self.assertIn("kept existing", r.stderr)
            with open(os.path.join(out, "report.md")) as f:
                self.assertIn("### C0 Named0", f.read())

    @unittest.skipUnless(HAVE_NX, "networkx not installed")
    def test_analyze_step_options(self):
        # Regression: analyze lacked the graph/names options of the step commands,
        # and its sweep ignored -r.
        with tempfile.TemporaryDirectory() as d:
            idx, out = self.write_fixture(d), os.path.join(d, "out")
            r = self.run_cli("analyze", "--index", idx, "--out-dir", out, "--seeds", "3",
                             "--exclude-symbols", "", "--package", "demo", "--owner-weight", "0",
                             "--impl-weight", "0", "-r", "0.7")
            self.assertEqual(r.returncode, 0, r.stderr)

            def load(name):
                with open(os.path.join(out, name)) as f:
                    return json.load(f)
            g = load("graph.json")
            self.assertIn("a/tests/t", g["nodes"])
            self.assertEqual((g["meta"]["owner_weight"], g["meta"]["impl_weight"]), (0, 0))
            self.assertIn(0.7, [row["resolution"] for row in load("sweep.json")["rows"]])
            self.assertEqual(load("clusters.json")["params"]["resolution"], 0.7)
            names = {c: {"name": f"N{c}"} for c in load("names.json")["clusters"]}
            with open(os.path.join(d, "old_names.json"), "w") as f:
                json.dump({"clusters": names}, f)
            out2 = os.path.join(d, "out2")
            r = self.run_cli("analyze", "--index", idx, "--out-dir", out2, "--seeds", "3",
                             "--exclude-symbols", "", "-r", "0.7", "--threshold", "1.01", "--carry-names",
                             os.path.join(out, "clusters.json"), os.path.join(d, "old_names.json"))
            self.assertEqual(r.returncode, 0, r.stderr)
            with open(os.path.join(out2, "names.json")) as f:
                self.assertEqual({e["name"] for e in json.load(f)["clusters"].values()}, {""})

    def test_analyze_index_options(self):
        # Regression: analyze did not accept --all-features / -v for the index step.
        from refgraph_lib import cli
        seen = []
        orig = cli.cmd_index
        cli.cmd_index = lambda a: seen.append(a) or 2
        try:
            with tempfile.TemporaryDirectory() as d:
                rc = cli.main(["analyze", d, "--out-dir", d, "--all-features", "-v"])
        finally:
            cli.cmd_index = orig
        self.assertEqual(rc, 2)
        self.assertTrue(seen[0].all_features)
        self.assertTrue(seen[0].verbose)

    def test_where_limit_applies_to_files(self):
        # Regression: --limit capped item lines but every matching file was printed.
        cl = {"clusters": [{"id": 0, "members": ["src/f0/x", "src/f1/x", "src/f2/x"]}],
              "file_split": {f"src/f{i}.rs": [[0, 1]] for i in range(3)}}
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "c.json")
            with open(p, "w") as f:
                json.dump(cl, f)
            r = self.run_cli("where", p, "src/f", "--limit", "1")
        self.assertEqual(r.returncode, 0, r.stderr)
        lines = r.stdout.splitlines()
        self.assertEqual([x for x in lines if x.startswith("file ")], ["file src/f0.rs: C0 (1)"])
        self.assertIn("… 2 more files (use --limit)", lines)
        self.assertEqual([x for x in lines if "->" in x], ["src/f0/x  ->  C0"])
        self.assertIn("… 2 more items (use --limit)", lines)

    def test_user_errors_are_one_line(self):
        # Regression: a missing input file or a bad --exclude-symbols regex ended in a traceback.
        with tempfile.TemporaryDirectory() as d:
            idx = self.write_fixture(d)
            for args in (("report", os.path.join(d, "missing.json")),
                         ("where", os.path.join(d, "missing.json"), "x"),
                         ("graph", os.path.join(d, "missing.scip"), "-o", os.path.join(d, "g.json")),
                         ("graph", idx, "--exclude-symbols", "(", "-o", os.path.join(d, "g.json"))):
                r = self.run_cli(*args)
                self.assertEqual(r.returncode, 2, (args, r.stderr))
                self.assertNotIn("Traceback", r.stderr, args)
                self.assertEqual(len(r.stderr.strip().splitlines()), 1, (args, r.stderr))
                self.assertTrue(r.stderr.startswith("refgraph: "), r.stderr)


if __name__ == "__main__":
    unittest.main()
