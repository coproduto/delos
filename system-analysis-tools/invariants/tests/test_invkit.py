"""Tests for invkit. They build a tiny Rust project and catalogue on disk, so
no cargo or indexer is needed (except for the probe output parser, which
works on canned text)."""
import contextlib
import io
import json
import os
import subprocess
import sys
import tempfile
import textwrap
import unittest
from unittest import mock

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from invkit_lib import catalog, check, cli, pins, probe, query, relate, rustsrc, scan  # noqa: E402

LAUNCHER = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "invkit")

GRAPH_RS = textwrap.dedent('''\
    //! A tiny graph.
    use std::hash::{Hash, Hasher};

    #[derive(Clone, Copy)]
    pub struct Link {
        pub node: u64,
        pub output: usize,
    }

    impl From<u64> for Link {
        fn from(node: u64) -> Self {
            Link { node, output: 0 }
        }
    }

    #[derive(Clone)]
    pub struct Graph {
        pub inputs: Vec<Option<Link>>,
        next_id: u64,
    }

    impl Graph {
        pub fn new() -> Self {
            Graph { inputs: Vec::new(), next_id: 1 }
        }

        /// Ids must be unique within one graph.
        fn fresh_id(&mut self) -> u64 {
            let id = self.next_id;
            self.next_id += 1;
            id
        }

        pub fn connect(&mut self, from: Link, port: usize) -> Result<(), String> {
            if port >= self.inputs.len() {
                return Err("no such port".into());
            }
            self.inputs[port] = Some(from);
            Ok(())
        }

        pub fn hash_of(&self, h: &mut impl Hasher) {
            self.next_id.hash(h);
        }
    }

    pub fn rewire(g: &mut Graph) {
        g.inputs[0] = None;
    }

    pub const WGSL: &str = r#"
    fn kernel(p: vec2<f32>) -> vec4<f32> {
        return vec4<f32>(0.0);
    }
    "#;

    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    pub fn tricky<'a>(s: &'a str) -> char {
        let _c = '{';
        let _q = "}{ // not a comment";
        /* nested /* comment */ { */
        'x'
    }
    ''')

TEST_RS = textwrap.dedent('''\
    #[test]
    fn connect_rejects_bad_ports() {}

    #[test]
    fn other() {}
    ''')

AREA_MD = textwrap.dedent('''\
    # 1 · Graph

    ## G1 · Ports exist {#g1}

    **Statement.** Every edge goes to a port that exists.

    **Maintained by**
    - `connect` refuses bad ports ([graph.rs:34-38](../../src/graph.rs#L34)).
    - `rewire` writes directly ([graph.rs:48](../../src/graph.rs#L48)).

    **Tests.** `tests/graph.rs::connect_rejects_bad_ports`.

    **Status.** Holds.

    ---

    ## G2 · Ids are unique {#g2}

    **Statement.** `fresh_id` hands out increasing ids ([graph.rs:28-32](../../src/graph.rs#L28)). See [G1](#g1).

    **Depended on by.** Hashing ([graph.rs:42-44](../../src/graph.rs#L42)), and [G1](01-graph.md#g1).

    **Status: VIOLATED.** Ids repeat after a clone.
    ''')

FINDINGS_MD = textwrap.dedent('''\
    # Findings

    ## Finding 1 · Ids repeat {#finding-1}

    **Invariant.** [G2](01-graph.md#g2): ids are unique.

    **Confirmed.** Probe `ids_differ_after_clone`: they do not.

    ## Fragile spots

    | # | Invariant | What could break it | Detected by |
    |---|---|---|---|
    | 1 | [G1](01-graph.md#g1): ports | direct writes | nothing |
    ''')

README_MD = textwrap.dedent('''\
    # Toy invariants

    The model covers 2 invariants across one areas.

    One invariants turned out to be **violated**.

    The analysis is of commit `abc1234`.

    | Mechanism | Invariants |
    |---|---|
    | API | G1 |
    ''')

PROBE_RS = textwrap.dedent('''\
    #[test]
    fn ids_differ_after_clone() {}
    ''')


def write(root, rel, text):
    p = os.path.join(root, rel)
    os.makedirs(os.path.dirname(p), exist_ok=True)
    with open(p, "w") as f:
        f.write(text)


class Project(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = self.tmp.name
        write(self.root, "src/graph.rs", GRAPH_RS)
        write(self.root, "tests/graph.rs", TEST_RS)
        self.cat_dir = os.path.join(self.root, "models/inv")
        write(self.cat_dir, "01-graph.md", AREA_MD)
        write(self.cat_dir, "findings.md", FINDINGS_MD)
        write(self.cat_dir, "README.md", README_MD)
        write(self.cat_dir, "probes/p.rs", PROBE_RS)

    def tearDown(self):
        self.tmp.cleanup()

    def cat(self):
        return catalog.load(self.cat_dir, self.root)

    def cli(self, *argv):
        """Run cli.main in-process; returns (exit code, stdout)."""
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf), contextlib.redirect_stderr(io.StringIO()):
            rc = cli.main(list(argv))
        return rc, buf.getvalue()

    def launch(self, *argv, cwd=None, env=None):
        """Run the launcher as a user would; returns the CompletedProcess."""
        return subprocess.run([sys.executable, LAUNCHER, *argv], cwd=cwd or self.root, env=env,
                              capture_output=True, text=True, timeout=60)


class Lexer(Project):
    def test_masks_comments_strings_and_chars(self):
        src = rustsrc.load(self.root, "src/graph.rs")
        tricky = [it for it in src.items if it.name == "tricky"][0]
        body = "\n".join(src.masked_lines[tricky.line - 1:tricky.end])
        self.assertNotIn("not a comment", body)
        self.assertEqual(body.count("{"), body.count("}"))  # braces in strings/chars/comments are blanked
        self.assertEqual(tricky.end, len(GRAPH_RS.rstrip().split("\n")))
        self.assertTrue(any(s.raw and "fn kernel" in s.text for s in src.strings))
        self.assertTrue(any(c.doc and "must be unique" in c.text for c in src.comments))

    def test_items_are_qualified_and_impl_in_type_position_is_ignored(self):
        src = rustsrc.load(self.root, "src/graph.rs")
        quals = [it.qual for it in src.items]
        self.assertIn("Graph::fresh_id", quals)
        self.assertIn("Link::from", quals)
        self.assertIn("rewire", quals)
        self.assertNotIn("Hasher", [it.name for it in src.items if it.kind == "impl"])
        g = [it for it in src.items if it.kind == "struct" and it.name == "Graph"][0]
        self.assertIn("Clone", g.derives)
        self.assertEqual(src.where(30), "Graph::fresh_id")

    def test_tests_are_listed(self):
        src = rustsrc.load(self.root, "tests/graph.rs")
        self.assertEqual([t.name for t in rustsrc.tests(src)], ["connect_rejects_bad_ports", "other"])


class Catalogue(Project):
    def test_parses_invariants_sections_and_links(self):
        cat = self.cat()
        self.assertEqual([i.id for i in cat.invariants], ["G1", "G2"])
        g1, g2 = cat.invariants
        self.assertEqual(g1.area, "Graph")
        self.assertIn("maintained", g1.sections)
        self.assertEqual([(c.file, c.start, c.end) for c in g1.citations],
                         [("src/graph.rs", 34, 38), ("src/graph.rs", 48, 48)])
        self.assertEqual(g1.citations[0].names, ["connect"])
        self.assertEqual([(t.file, t.name) for t in g1.tests], [("tests/graph.rs", "connect_rejects_bad_ports")])
        self.assertEqual(g1.status_class, "holds")
        self.assertEqual(g2.status_class, "violated")
        self.assertEqual({(x.md_target, x.target) for x in g2.xrefs}, {("", "g1"), ("01-graph.md", "g1")})

    def test_findings_fragile_and_probes(self):
        cat = self.cat()
        self.assertEqual(cat.findings[0].invariants, ["G2"])
        self.assertEqual(cat.findings[0].probes, ["ids_differ_after_clone"])
        self.assertEqual(cat.fragile[0]["invariants"], ["G1"])
        self.assertEqual(cat.probes, {"probes/p.rs": ["ids_differ_after_clone"]})

    def test_status_classes(self):
        self.assertEqual(catalog.classify_status("Status", "Holds."), "holds")
        self.assertEqual(catalog.classify_status("Status", "Holds for PNG only."), "qualified")
        self.assertEqual(catalog.classify_status("Status: VIOLATED", "x"), "violated")
        self.assertEqual(catalog.classify_status("Status", "Inherits the [I1](#i1) violation."), "inherited")
        self.assertEqual(catalog.classify_status("Status", "Holds. Values from files can violate it."), "qualified")


class Check(Project):
    def errors(self, rep):
        return [(i.kind, i.message) for i in rep.issues if i.level == "error"]

    def test_clean_catalogue_has_no_errors(self):
        rep = check.run(self.cat())
        self.assertEqual(self.errors(rep), [])
        self.assertEqual(rep.stats["invariants"], 2)

    def test_detects_broken_references(self):
        bad = AREA_MD.replace("graph.rs:48](../../src/graph.rs#L48)", "graph.rs:480](../../src/graph.rs#L480)")
        bad = bad.replace("connect_rejects_bad_ports", "no_such_test").replace("(#g1)", "(#g9)")
        write(self.cat_dir, "01-graph.md", bad)
        kinds = {k for k, _ in self.errors(check.run(self.cat()))}
        self.assertTrue({"bad-line", "missing-test", "bad-xref"} <= kinds, kinds)

    def test_detects_count_and_finding_mismatches(self):
        write(self.cat_dir, "README.md", README_MD.replace("2 invariants", "3 invariants") + "\n| x | G7 |\n")
        write(self.cat_dir, "findings.md", FINDINGS_MD.replace("[G2](01-graph.md#g2)", "[G1](01-graph.md#g1)"))
        msgs = " ".join(m for _, m in self.errors(check.run(self.cat())))
        self.assertIn("says 3 invariants", msgs)
        self.assertIn("G2 is VIOLATED but no finding names it", msgs)
        self.assertIn("G7", msgs)

    def test_anchoring_separates_right_and_wrong_lines(self):
        cat = self.cat()
        code = check.Code(self.root)
        vocab = check.Vocabulary(code, rustsrc.rust_files(self.root))
        good = cat.invariants[0].citations[0]  # `connect` at 34-38
        self.assertEqual(check.anchored(code, good, vocab)[0], "name")
        import dataclasses
        wrong = dataclasses.replace(good, start=5, end=7)
        self.assertEqual(check.anchored(code, wrong, vocab)[0], "unanchored")


class Pins(Project):
    def test_same_moved_changed(self):
        p = pins.pin(self.cat())
        self.assertTrue(all(r["state"] == "same" for r in pins.verify(self.root, p)))
        # insert two lines at the top: everything moves
        write(self.root, "src/graph.rs", "// a\n// b\n" + GRAPH_RS)
        rows = pins.verify(self.root, p)
        moved = [r for r in rows if r["cite"] == "graph.rs:48"][0]
        self.assertEqual((moved["state"], moved["now"]), ("moved", "50"))
        # change the rewired line: that citation is now changed
        write(self.root, "src/graph.rs", GRAPH_RS.replace("g.inputs[0] = None;", "g.inputs.clear();"))
        rows = pins.verify(self.root, p)
        self.assertEqual([r for r in rows if r["cite"] == "graph.rs:48"][0]["state"], "changed")


class Scan(Project):
    def test_detectors(self):
        res = scan.run(self.root)
        kinds = {(c["kind"], c["line"]) for c in res["candidates"]}
        self.assertIn(("guard", 36), kinds)
        self.assertIn(("claim", 27), kinds)
        self.assertIn(("field-write", 48), kinds)  # rewire bypasses Graph's API
        self.assertNotIn(("field-write", 38), kinds)  # connect is Graph's own impl
        self.assertIn(("default", 10), kinds)
        self.assertIn(("dual", 51), kinds)
        self.assertIn(("key", 42), kinds)

    def test_identity_scopes(self):
        res = scan.run(self.root)
        notes = {s["note"] for s in res["identity"]["sources"]}
        self.assertTrue(any(n.startswith("Graph.next_id: per value") for n in notes), notes)
        self.assertIn("static atomic: unique for the life of the process", notes)
        self.assertIn("Graph::fresh_id", res["identity"]["issuers"])

    def test_recall_against_catalogue(self):
        res = scan.run(self.root)
        rc = scan.recall(res, self.cat())
        self.assertEqual(rc["invariants"], {"covered": 2, "total": 2})


class Query(Project):
    def test_writes_and_refs(self):
        w = query.writes(self.root, "inputs")
        self.assertEqual(sorted((r["item"], r["line"]) for r in w), [("Graph::connect", 38), ("rewire", 48)])
        r = query.refs(self.root, "fresh_id")
        self.assertEqual(r, [])  # defined but never called
        r = query.refs(self.root, "connect")
        self.assertEqual(r, [])


class Relate(Project):
    def test_edges_follow_sections(self):
        es = relate.edges(self.cat())
        # G2's statement mentions G1 (G2 rests on G1); its "Depended on by" mentions G1 (G1 rests on G2)
        self.assertIn(("G2", "G1", "statement"), es)
        self.assertIn(("G1", "G2", "depended"), es)


class Regressions(Project):
    """One test per bug found in the documentation audit (October 2026)."""

    # 1. --cargo-args took one argument per `=` form; a quoted multi-word value
    #    reached cargo as a single argument.
    def test_cargo_args_quoted_value_is_split(self):
        seen = []

        def fake_run(cat, cargo_args=(), **kw):
            seen.append(tuple(cargo_args))
            return {"probes": [], "ok": True}

        with mock.patch.object(probe, "run", fake_run):
            self.cli("probe", self.cat_dir, "--root", self.root, "--cargo-args=--features x")
            self.cli("probe", self.cat_dir, "--root", self.root, "--cargo-args", "--features x --release")
            self.cli("probe", self.cat_dir, "--root", self.root, "--cargo-args=--release")
            self.cli("probe", self.cat_dir, "--root", self.root)
        self.assertEqual(seen, [("--features", "x"), ("--features", "x", "--release"), ("--release",), ()])

    # 2. A bullet starting with bold text opened a `note:` section, so the
    #    citations and links under "Depended on by" were filed in the wrong one.
    def test_bold_bullet_stays_in_its_section(self):
        write(self.cat_dir, "01-graph.md", AREA_MD + textwrap.dedent("""\

            ---

            ## G3 · Hashes are stable {#g3}

            **Statement.** `hash_of` hashes the id.

            **Depended on by**
            - **Hashing.** `hash_of` ([graph.rs:42-44](../../src/graph.rs#L42)).
            - **Ports.** [G1](#g1) relies on it.

            **Status.** Holds.
            """))
        cat = self.cat()
        g3 = cat.by_id()["G3"]
        self.assertEqual([c.section for c in g3.citations], ["depended"])
        self.assertFalse([s for s in g3.sections if s.startswith("note:")], g3.sections)
        self.assertIn(("G1", "G3", "depended"), relate.edges(cat))

    # 3. "2 invariants turned out to be **violated**" always reported a count error.
    def test_violated_count_may_be_a_digit(self):
        def count_errors():
            return [i.message for i in check.run(self.cat()).issues if i.kind == "count" and i.level == "error"]

        write(self.cat_dir, "README.md", README_MD.replace("One invariants", "1 invariants"))
        self.assertEqual(count_errors(), [])
        write(self.cat_dir, "README.md", README_MD.replace("One invariants", "3 invariants"))
        self.assertEqual(len(count_errors()), 1)

    # 4. The same pair linked from two sections was drawn and counted twice, and
    #    an invariant in a cycle counted itself among its transitive dependents.
    def test_relate_deduplicates_pairs_and_ignores_self_in_cycles(self):
        write(self.cat_dir, "01-graph.md", AREA_MD.replace(
            "**Status: VIOLATED.**", "**Depends on.** [the port rule](#g1).\n\n**Status: VIOLATED.**"))
        cat = self.cat()
        es = relate.edges(cat)
        self.assertIn(("G2", "G1", "statement"), es)
        self.assertIn(("G2", "G1", "depends"), es)
        self.assertEqual(relate.mermaid(cat, es).count("G1 --> G2"), 1)
        rc, out = self.cli("relate", self.cat_dir, "--root", self.root)
        self.assertTrue(out.startswith("2 explicit relations"), out)
        for r in relate.roots(cat, es):  # G1 and G2 rest on each other
            self.assertNotIn(r["id"], r["transitive"])

    # 5a. `writes --init` listed function parameters as initialisations.
    def test_writes_init_skips_function_parameters(self):
        write(self.root, "src/gpu.rs", textwrap.dedent("""\
            pub struct GpuInput;
            pub struct Job {
                pub inputs: Vec<u8>,
            }
            pub fn dispatch(
                device: u32,
                inputs: &[GpuInput],
            ) -> Job {
                Job { inputs: Vec::new() }
            }
            pub fn short(inputs: &[GpuInput]) -> usize { 0 }
            pub fn tail(device: u32,
                inputs: &[GpuInput]) -> Job {
                Job {
                    inputs: vec![1],
                }
            }
            """))
        rows = [(r["line"], r["kind"]) for r in query.writes(self.root, "inputs", init=True) if r["file"] == "src/gpu.rs"]
        self.assertEqual(rows, [(9, "init"), (15, "init")])

    # 5b. `refs Type::name` returned the same rows as `refs name`.
    def test_refs_type_member_excludes_other_types(self):
        write(self.root, "src/two.rs", textwrap.dedent("""\
            pub struct A;
            pub struct B;
            impl A {
                pub fn go(&self) {}
                pub fn run(&self) {
                    self.go();
                }
            }
            impl B {
                pub fn go(&self) {}
                pub fn run(&self) {
                    self.go();
                    Self::go(self);
                }
            }
            pub fn main() {
                A::go(&A);
                B::go(&B);
                let x = A;
                x.go();
            }
            """))

        def lines(name):
            return sorted(r["line"] for r in query.refs(self.root, name) if r["file"] == "src/two.rs")

        self.assertEqual(lines("go"), [6, 12, 13, 17, 18, 20])
        self.assertEqual(lines("A::go"), [6, 17, 20])  # x.go(): receiver type unknown, kept
        self.assertEqual(lines("B::go"), [12, 13, 18, 20])

    # 6. File and git errors ended in a Python traceback.
    def assertCleanError(self, p):
        self.assertEqual(p.returncode, 1, p.stderr)
        self.assertNotIn("Traceback", p.stderr)
        errs = [l for l in p.stderr.strip().split("\n") if not l.startswith("$ ")]  # `$ cargo …` is progress
        self.assertEqual(len(errs), 1, p.stderr)
        self.assertTrue(errs[0].startswith("invkit: error: "), p.stderr)

    def test_missing_files_are_clean_errors(self):
        self.assertCleanError(self.launch("check", "models/nope"))
        self.assertCleanError(self.launch("check", "models/inv", "--pins", "nope.json"))
        self.assertCleanError(self.launch("recall", "nope.json", "models/inv"))
        write(self.root, "bad.json", "{not json")
        self.assertCleanError(self.launch("check", "models/inv", "--pins", "bad.json"))

    def test_bad_revision_is_a_clean_error(self):
        subprocess.run(["git", "init", "-q", self.root], check=True)
        self.assertCleanError(self.launch("impact", "models/inv", "--rev", "no-such-rev"))

    def test_probe_timeout_is_a_clean_error(self):
        write(self.root, "Cargo.toml", "[package]\nname = \"toy\"\n")
        bin_dir = os.path.join(self.root, "fakebin")
        write(bin_dir, "cargo", "#!/bin/sh\nsleep 20\n")
        os.chmod(os.path.join(bin_dir, "cargo"), 0o755)
        env = {**os.environ, "PATH": bin_dir + os.pathsep + os.environ.get("PATH", "")}
        p = self.launch("probe", "models/inv", "--timeout", "1", env=env)
        self.assertCleanError(p)
        self.assertIn("timed out", p.stderr)
        self.assertFalse(os.path.exists(os.path.join(self.root, "tests", "zz_invkit_p.rs")))

    # 7. `recall` re-read sources relative to the directory `scan` was run in.
    def test_recall_works_from_another_directory(self):
        self.assertEqual(self.launch("scan", "-o", "scan.json").returncode, 0)
        with open(os.path.join(self.root, "scan.json")) as f:
            self.assertEqual(json.load(f)["root"], ".")  # what existing scan files hold
        elsewhere = tempfile.TemporaryDirectory()
        self.addCleanup(elsewhere.cleanup)
        p = self.launch("recall", os.path.join(self.root, "scan.json"), self.cat_dir, "--root", self.root,
                        cwd=elsewhere.name)
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn("citations near a candidate", p.stdout)
        self.assertNotIn("\n0 cited items", p.stdout)


class Probe(unittest.TestCase):
    OUT = textwrap.dedent('''\
        running 2 tests
        test ids_differ_after_clone ... FAILED
        test fine ... ok

        failures:

        ---- ids_differ_after_clone stdout ----

        thread 'ids_differ_after_clone' panicked at tests/zz.rs:3:5:
        assertion `left != right` failed: same ids
          left: 1
         right: 1
        note: run with `RUST_BACKTRACE=1`

        failures:
            ids_differ_after_clone
        ''')

    def test_parse(self):
        self.assertEqual(probe.parse_results(self.OUT), {"ids_differ_after_clone": "FAILED", "fine": "ok"})
        self.assertIn("same ids", probe.panic_messages(self.OUT)["ids_differ_after_clone"])


if __name__ == "__main__":
    unittest.main()
