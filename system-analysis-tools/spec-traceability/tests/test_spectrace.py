"""Tests for spectrace. They build a tiny spec, Rust crate and matrix in a
temporary directory, so no cargo is needed (except for nothing: `probe`
is not exercised here)."""
import io
import json
import os
import subprocess
import sys
import tempfile
import textwrap
import unittest
from contextlib import redirect_stdout

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from spectrace_lib import ancestry, check, cli, matrix, pins, report, rust, spec, suggest  # noqa: E402

SPEC = textwrap.dedent("""\
    # Tiny

    ## Graph

    1. Nodes are connected with edges. Cycles are refused.
       - 1.1. Saving is atomic (e.g. via rename).
    2. The value `(a b. c)` is a list.

    ```text
    an illustration
    ```
""")

GRAPH_RS = textwrap.dedent('''\
    //! A tiny graph.

    /// Connects two nodes, refusing cycles.
    pub fn connect(a: u64, b: u64) -> Result<(), String> {
        if a == b {
            return Err("cycle".into());
        }
        let s = "fn not_an_item() { }";
        let _ = s;
        Ok(())
    }

    fn lum(c: [f32; 3]) -> f32 {
        c[0] + c[1]
    }

    pub fn write_atomic(path: &str) {
        let _ = path; // writes a temporary file, then renames it
    }

    pub fn unused() {}

    #[cfg(test)]
    mod tests {
        #[test]
        fn inline_test() {}
    }
''')

TEST_RS = textwrap.dedent('''\
    #[test]
    fn cycles_are_refused() {}
''')


def matrix_md(extra_quote: str = "") -> str:
    return textwrap.dedent(f"""\
        # 1 · Graph

        ## G1 · Edges without cycles {{#g1}}

        > Nodes are connected with edges.
        > Cycles are refused.{extra_quote}

        **Status.** Implemented.

        **Implemented by.**
        - `connect` refuses equal ends ([graph.rs:4-11](../src/graph.rs#L4)).

        **Tests.** `tests/graph.rs::cycles_are_refused`.

        ## G1.1 · Atomic saves {{#g1-1}}

        > Saving is atomic (e.g. via rename).

        **Status.** Partial. No fsync; see [G1](#g1).

        **Implemented by.**
        - `write_atomic` ([graph.rs:17-19](../src/graph.rs#L17)).

        **Tests.** None.

        ## G2 · Lists {{#g2}}

        > The value `(a b. c)` is a list.

        **Status.** Missing.
        """)


README = textwrap.dedent("""\
    # Tiny matrix

    Commit `abc1234`.

    <!-- spectrace:summary -->
    <!-- /spectrace:summary -->
""")


class Fixture(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = self.tmp.name
        self.write("SPEC.md", SPEC)
        self.write("src/graph.rs", GRAPH_RS)
        self.write("tests/graph.rs", TEST_RS)
        self.write("m/01-graph.md", matrix_md())
        self.write("m/README.md", README)

    def tearDown(self):
        self.tmp.cleanup()

    def write(self, rel, text):
        p = os.path.join(self.root, rel)
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "w") as f:
            f.write(text)

    def load(self):
        sp = spec.load(os.path.join(self.root, "SPEC.md"))
        m = matrix.load(os.path.join(self.root, "m"), self.root)
        return sp, m, rust.Tree(self.root)

    def fill_summary(self):
        sp, m, _ = self.load()
        p = os.path.join(self.root, "m/README.md")
        text = open(p).read()
        open(p, "w").write(report.replace_block(text, "summary", report.summary_md(sp, m)))


class SpecTests(Fixture):
    def test_units_keep_numbering_and_code_spans(self):
        sp, _, _ = self.load()
        texts = [(u.item, u.kind, u.text) for u in sp.units]
        self.assertIn(("1", "text", "Nodes are connected with edges."), texts)
        self.assertIn(("1", "text", "Cycles are refused."), texts)
        # nested numbering and an abbreviation that must not split
        self.assertIn(("1.1", "text", "Saving is atomic (e.g. via rename)."), texts)
        # no split inside a code span
        self.assertIn(("2", "text", "The value `(a b. c)` is a list."), texts)
        self.assertEqual([u.kind for u in sp.units].count("code"), 1)

    def test_find_with_ellipsis(self):
        sp, _, _ = self.load()
        self.assertIsNotNone(sp.find("Nodes are … refused."))
        self.assertIsNone(sp.find("Nodes are never connected"))


class RustTests(Fixture):
    def test_items_ignore_strings_and_brackets(self):
        src = rust.Tree(self.root).get("src/graph.rs")
        names = {(i.name, i.start, i.end) for i in src.items}
        self.assertIn(("connect", 4, 11), names)
        self.assertIn(("lum", 13, 15), names)  # `;` inside [f32; 3] does not end it
        self.assertNotIn("not_an_item", {i.name for i in src.items})
        self.assertTrue(next(i for i in src.items if i.name == "inline_test").is_test)
        self.assertEqual(next(i for i in src.items if i.name == "connect").doc, "Connects two nodes, refusing cycles.")


class CheckTests(Fixture):
    def test_clean_matrix(self):
        self.fill_summary()
        sp, m, tree = self.load()
        r = check.run(sp, m, tree)
        self.assertEqual(r.errors, [])
        self.assertEqual(r.stats["unanchored"], 0)
        self.assertEqual(r.stats["uncovered_units"], 1)  # the code block, a note
        self.assertTrue(any("illustration" in n for n in r.notes))

    def test_errors(self):
        md = matrix_md(" Typo here.").replace("graph.rs:17-19](../src/graph.rs#L17)", "graph.rs:17-99](../src/graph.rs#L18)")
        md = md.replace("cycles_are_refused", "cycles_are_gone").replace("**Status.** Missing.", "**Status.** Maybe.")
        md += "\n## G3 · Broken link {#g3}\n\nSee [nowhere](#zz).\n"
        self.write("m/01-graph.md", md)
        self.fill_summary()
        self.write("m/01-graph.md", md.replace("Edges without", "Edges sans"))  # summary stays valid; titles don't count
        sp, m, tree = self.load()
        errors = "\n".join(check.run(sp, m, tree).errors)
        self.assertIn("not in the spec", errors)          # the typo quote
        self.assertIn("outside the file", errors)         # 17-99
        self.assertIn("cycles_are_gone", errors)          # missing test
        self.assertIn("needs **Status.**", errors)        # "Maybe"
        self.assertIn("#zz", errors)                      # broken link
        self.assertIn("no entry quotes", errors)          # the typo quote no longer covers "Cycles are refused."

    def test_unquoted_sentence_is_an_error(self):
        self.write("m/01-graph.md", matrix_md().replace("> Cycles are refused.\n", ""))
        self.fill_summary()
        sp, m, tree = self.load()
        errors = check.run(sp, m, tree).errors
        self.assertTrue(any("Cycles are refused" in e for e in errors), errors)

    def test_unanchored_citation(self):
        self.write("m/01-graph.md", matrix_md().replace("`write_atomic` ([graph.rs:17-19]", "`renamer` ([graph.rs:17-19]"))
        self.fill_summary()
        sp, m, tree = self.load()
        r = check.run(sp, m, tree)
        self.assertEqual(r.stats["unanchored"], 1)

    def test_stale_summary(self):
        sp, m, tree = self.load()
        self.assertIn("summary block is stale", "\n".join(check.run(sp, m, tree).errors))


class ReportTests(Fixture):
    def test_summary_grid_index_uncited_why(self):
        sp, m, tree = self.load()
        s = report.summary_md(sp, m)
        self.assertIn("| 1 · Graph | 3 | 1 | 1 |  | 1 |", s)
        cols, rows = report.grid(m)
        self.assertEqual(cols, ["graph"])
        idx = report.code_index(m, tree)
        cited = {i["name"]: i["entries"] for i in idx["src/graph.rs"]}
        self.assertEqual(cited["connect"], ["G1"])
        self.assertEqual(cited["unused"], [])
        un = report.uncited(m, tree)[0]
        self.assertIn("unused (21-21)", un["uncited"])
        w = report.why(m, tree, "src/graph.rs", 6)
        self.assertEqual((w["item"], w["direct"]), ("connect", ["G1"]))
        html = report.html_page(sp, m, tree, "abc1234")
        self.assertIn('"G1.1"', html)
        self.assertNotIn("/*DATA*/null", html)

    def test_suggest(self):
        rows = suggest.rank("Saving is atomic via rename", rust.Tree(self.root), 3)
        self.assertEqual(rows[0]["item"], "write_atomic")


class PinTests(Fixture):
    def test_pin_and_verify(self):
        sp, m, tree = self.load()
        data = pins.pin(sp, m, tree, "abc1234")
        self.assertEqual(data["commit"], "abc1234")
        # move connect down two lines, change write_atomic, change a spec sentence
        self.write("src/graph.rs", "\n\n" + GRAPH_RS.replace("then renames it", "then copies it"))
        self.write("SPEC.md", SPEC.replace("Cycles are refused.", "Cycles are rejected."))
        sp2, _, tree2 = spec.load(os.path.join(self.root, "SPEC.md")), None, rust.Tree(self.root)
        v = pins.verify(data, sp2, tree2)
        states = {k: s["state"] for k, s in v["citations"].items()}
        self.assertEqual(states["src/graph.rs:4-11"], "moved")
        self.assertEqual(v["citations"]["src/graph.rs:4-11"]["to"], "6-13")
        self.assertEqual(states["src/graph.rs:17-19"], "changed")
        self.assertEqual([u["text"] for u in v["spec"]["changed"]], ["Cycles are refused."])

    def test_impact(self):
        def git(*a):
            subprocess.run(["git", "-C", self.root, *a], check=True, capture_output=True)
        git("init", "-q")
        git("-c", "user.email=t@t", "-c", "user.name=t", "add", ".")
        git("-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", "base")
        self.write("src/graph.rs", GRAPH_RS.replace("if a == b {", "if a == b || a == 0 {"))
        self.write("SPEC.md", SPEC.replace("Saving is atomic", "Saving is durable"))
        sp, m, _ = self.load()
        r = pins.impact(self.root, "HEAD", sp.path, m)
        self.assertIn("G1", r["code"])
        self.assertIn("G1.1", r["spec"])


APP_RS = textwrap.dedent('''\
    pub struct History {
        undo: Vec<u64>,
        last: u64,
    }

    pub struct App {
        history: History,
        count: u64,
    }

    impl App {
        pub fn edit(&mut self) {
            self.record();
            self.count += 1;
        }

        fn record(&mut self) {
            self.history.undo.push(self.count);
        }

        pub fn undo(&mut self) {
            if let Some(c) = self.history.undo.pop() {
                self.count = c;
            }
        }

        pub fn on_key(&mut self, k: char) {
            if k == 'e' {
                self.edit();
            }
            if k == 'z' {
                let before = self.count;
                self.undo();
                self.history.last = before;
            }
        }
    }
''')


def line_of(text: str, needle: str, nth: int = 0) -> int:
    hits = [i for i, l in enumerate(text.splitlines(), 1) if needle in l]
    return hits[nth]


L_EDIT, L_ONKEY, L_UNDO = line_of(APP_RS, "fn edit"), line_of(APP_RS, "fn on_key"), line_of(APP_RS, "pub fn undo")
ANC_MD = textwrap.dedent(f"""\
    # 1 · Editing

    ## A1 · Edits count {{#a1}}

    > Nodes are connected with edges.

    **Status.** Implemented.

    **Implemented by.**
    - `on_key` handles `e` by calling `edit` ([app.rs:{L_ONKEY}-{L_ONKEY + 3}](../src/app.rs#L{L_ONKEY})), which bumps `count` ([app.rs:{L_EDIT}-{L_EDIT + 3}](../src/app.rs#L{L_EDIT})).

    **Notes.** The `undo` method exists ([app.rs:{L_UNDO}-{L_UNDO + 4}](../src/app.rs#L{L_UNDO})).
    """)


class AncestryTests(Fixture):
    def setUp(self):
        super().setUp()
        self.write("src/app.rs", APP_RS)
        self.write("m/02-app.md", ANC_MD)

    def test_unspecified_function_state_and_span(self):
        _, m, tree = self.load()
        a = ancestry.analyse(m, tree)
        cls = {v["name"]: v["class"] for v in a["items"].values()}
        self.assertEqual(cls["App::edit"], "specified")
        self.assertEqual(cls["App::record"], "support")  # reached from cited `edit`
        # cited only in Notes: not spec; reached only from the uncited `z` branch
        self.assertEqual(cls["App::undo"], "unspecified")
        undo = next(g for g in a["clusters"] if any("App::undo" in x for x in g["members"]))
        self.assertEqual(undo["triggers"][0]["line"], line_of(APP_RS, "self.undo();"))
        self.assertEqual(undo["noted_in"], ["A1"])
        fields = {s["field"]: s for s in a["state"]}
        self.assertIn("History.undo", fields)            # written on the cited path, used on no cited line
        self.assertEqual(fields["History.undo"]["clauses_through_writers"], ["A1"])
        self.assertIn("History.last", fields)
        self.assertNotIn("App.count", fields)            # used on a cited line
        span = next(s for s in a["spans"] if s["item"] == "App::on_key")
        self.assertEqual(span["start"], line_of(APP_RS, "if k == 'z'"))
        self.assertIn("App::undo", span["calls"])
        self.assertIn("History.undo", ancestry.report_md(a))

    def test_recall_scores_note_entries(self):
        z = line_of(APP_RS, "if k == 'z'")
        self.write("m/unspecified.md", textwrap.dedent(f"""\
            # Unspecified

            ## U1 · Undo on z {{#u1}}

            **What.** `on_key` undoes on `z` ([app.rs:{z}-{z + 4}](../src/app.rs#L{z})).

            ## U2 · Edit bumps the count {{#u2}}

            **What.** `edit` ([app.rs:{L_EDIT}-{L_EDIT + 3}](../src/app.rs#L{L_EDIT})).

            ## X1 · Not a U entry {{#x1}}

            **What.** `undo` ([app.rs:{L_UNDO}-{L_UNDO + 4}](../src/app.rs#L{L_UNDO})).
            """))
        _, m, tree = self.load()
        a = ancestry.analyse(m, tree)
        r = ancestry.recall(a, m, tree, "U")
        self.assertEqual((r["found"], r["total"]), (1, 2))
        rows = {x["id"]: x for x in r["rows"]}
        self.assertIn("span", rows["U1"]["found_by"])
        self.assertEqual((rows["U1"]["flagged_lines"], rows["U1"]["code_lines"]), (4, 4))  # the closing `}` is not code
        self.assertEqual((rows["U2"]["found_by"], rows["U2"]["flagged_lines"], rows["U2"]["code_lines"]), ([], 0, 3))
        self.assertEqual(r["flagged_share"], round(4 / 7, 3))
        self.assertTrue(0 < r["chance_share"] < r["flagged_share"])
        self.assertAlmostEqual(r["enrichment"], r["flagged_share"] / r["chance_share"], delta=0.01)
        self.assertGreater(r["enrichment"], 1)

    def test_field_access_is_not_a_method_call(self):
        idx = ancestry.Index(rust.Tree(self.root))
        names = [r.name for r in idx.refs_on_line("src/app.rs", line_of(APP_RS, "undo.pop()"))]
        self.assertNotIn("App::undo", names)
        self.assertIn("App::undo", [r.name for r in idx.refs_on_line("src/app.rs", line_of(APP_RS, "self.undo();"))])


class CliTests(Fixture):
    def test_check_exit_codes(self):
        out = io.StringIO()
        with redirect_stdout(out):
            code = cli.main(["--root", self.root, "summary", os.path.join(self.root, "m"), "--write"])
            self.assertEqual(code, 0)
            code = cli.main(["--root", self.root, "check", os.path.join(self.root, "m")])
        self.assertEqual(code, 0, out.getvalue())
        with redirect_stdout(io.StringIO()):
            code = cli.main(["--root", self.root, "units", os.path.join(self.root, "m"), "--uncovered", "--json"])
        self.assertEqual(code, 0)


LAUNCHER = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "spectrace")


def git(root, *a):
    subprocess.run(["git", "-C", root, "-c", "user.email=t@t", "-c", "user.name=t", *a], check=True, capture_output=True)


class RegressionTests(Fixture):
    """One test per bug fixed after the documentation audit."""

    def commit(self):
        git(self.root, "init", "-q")
        git(self.root, "add", ".")
        git(self.root, "commit", "-qm", "base")

    def run_cli(self, *args, env=None):
        return subprocess.run([sys.executable, LAUNCHER, *args], cwd=self.root, capture_output=True, text=True, env=env, timeout=60)

    def assertCleanError(self, p):
        self.assertNotIn("Traceback", p.stderr)
        self.assertEqual(p.returncode, 1, p.stderr)
        self.assertEqual(len(p.stderr.strip().splitlines()), 1, p.stderr)

    # 1. The ancestry docstring and help named an "effects" report analyse() never produces.
    def test_ancestry_text_names_the_reports_it_produces(self):
        self.write("src/app.rs", APP_RS)
        self.write("m/02-app.md", ANC_MD)
        _, m, tree = self.load()
        a = ancestry.analyse(m, tree)
        headings = [l[3:] for l in ancestry.report_md(a).splitlines() if l.startswith("## ")]
        self.assertEqual(len(headings), 3)
        for h in headings:
            self.assertIn(h, ancestry.__doc__)
        self.assertNotIn("effects", ancestry.__doc__.lower())
        out = io.StringIO()
        with redirect_stdout(out), self.assertRaises(SystemExit):
            cli.main(["-h"])
        help_ = " ".join(out.getvalue().split())
        line = help_[help_.index(" ancestry "):help_.index(" probe ")]
        self.assertNotIn("effects", line)
        for key in ("clusters", "state", "spans"):
            self.assertIn(key, a)
            self.assertIn(key, line)

    # 2. impact diffed only src/, tests/ and the spec, so citations into other files were never reported.
    def test_impact_reports_citations_outside_src_and_tests(self):
        self.write("Cargo.toml", '[package]\nname = "tiny"\nedition = "2021"\n')
        self.write("m/01-graph.md", matrix_md().replace(
            "**Status.** Missing.", "**Status.** Missing. The crate's `name` ([Cargo.toml:2-2](../Cargo.toml#L2)) is unrelated."))
        self.commit()
        self.write("Cargo.toml", '[package]\nname = "tinier"\nedition = "2021"\n')
        sp, m, _ = self.load()
        r = pins.impact(self.root, "HEAD", sp.path, m)
        self.assertEqual(r["code"].get("G2"), ["Cargo.toml:2-2 ∩ 2-2"])

    # 3. impact labelled spec hits `SPEC.md:` whatever --spec was.
    def test_impact_spec_label_uses_the_spec_path(self):
        self.write("docs/spec.md", SPEC)
        self.commit()
        self.write("docs/spec.md", SPEC.replace("Saving is atomic", "Saving is durable"))
        _, m, _ = self.load()
        r = pins.impact(self.root, "HEAD", os.path.join(self.root, "docs/spec.md"), m)
        self.assertTrue(r["spec"]["G1.1"])
        for hit in r["spec"]["G1.1"]:
            self.assertTrue(hit.startswith("docs/spec.md:"), hit)

    # 4. An insertion before line 1 mapped to line 0 and matched no citation.
    def test_impact_insertion_before_line_one(self):
        self.write("m/01-graph.md", matrix_md().replace(
            "**Status.** Missing.", "**Status.** Missing. The module doc ([graph.rs:1-1](../src/graph.rs#L1)) says nothing."))
        self.commit()
        self.write("src/graph.rs", "// new first line\n" + GRAPH_RS)
        self.assertEqual(pins.diff_hunks(self.root, "HEAD", ["src"]), {"src/graph.rs": [(1, 1)]})
        _, m, _ = self.load()
        r = pins.impact(self.root, "HEAD", os.path.join(self.root, "SPEC.md"), m)
        self.assertIn("G2", r["code"])

    # 5. Tracebacks instead of one-line errors.
    def test_missing_matrix_dir_is_a_clean_error(self):
        p = self.run_cli("check", "nope")
        self.assertCleanError(p)
        self.assertIn("nope", p.stderr)

    def test_missing_spec_is_a_clean_error(self):
        p = self.run_cli("--spec", "NOPE.md", "check", "m")
        self.assertCleanError(p)
        self.assertIn("NOPE.md", p.stderr)

    def test_bad_rev_is_a_clean_error(self):
        self.commit()
        p = self.run_cli("impact", "m", "--rev", "nosuchrev")
        self.assertCleanError(p)
        self.assertIn("nosuchrev", p.stderr)

    def test_why_without_line_is_a_clean_error(self):
        p = self.run_cli("why", "m", "src/graph.rs")
        self.assertCleanError(p)
        self.assertIn("FILE:LINE", p.stderr)

    def test_summary_write_without_readme_is_a_clean_error(self):
        os.remove(os.path.join(self.root, "m/README.md"))
        p = self.run_cli("summary", "m", "--write")
        self.assertCleanError(p)
        self.assertIn("README.md", p.stderr)

    def test_probe_timeout_is_a_clean_error(self):
        self.write("m/probes/slow.rs", "#[test]\nfn slow() {}\n")
        self.write("bin/cargo", "#!/bin/sh\nexec sleep 30\n")
        os.chmod(os.path.join(self.root, "bin/cargo"), 0o755)
        env = {**os.environ, "PATH": os.path.join(self.root, "bin") + os.pathsep + os.environ["PATH"]}
        p = self.run_cli("probe", "m", "--timeout", "1", env=env)
        self.assertCleanError(p)
        self.assertIn("timed out", p.stderr)
        self.assertEqual([f for f in os.listdir(os.path.join(self.root, "tests")) if f.startswith("zz_")], [])

    def test_broken_pipe_exits_quietly(self):
        self.write("SPEC.md", "".join(f"Sentence number {i} is long enough to fill a pipe buffer quickly.\n\n" for i in range(5000)))
        p = subprocess.Popen([sys.executable, LAUNCHER, "units"], cwd=self.root, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        p.stdout.close()
        err = p.stderr.read()
        p.stderr.close()
        code = p.wait(timeout=60)
        self.assertEqual(err, "")
        self.assertEqual(code, 141)

    # 6. why --json exited 0 when nothing cites the line; text mode exits 1.
    def test_why_json_exit_code_matches_text_mode(self):
        uncited = line_of(GRAPH_RS, "pub fn unused")
        for extra in ([], ["--json"]):
            with redirect_stdout(io.StringIO()):
                self.assertEqual(cli.main(["--root", self.root, "why", os.path.join(self.root, "m"), f"src/graph.rs:{uncited}", *extra]), 1, extra)
                self.assertEqual(cli.main(["--root", self.root, "why", os.path.join(self.root, "m"), "src/graph.rs:6", *extra]), 0, extra)

    # 8. The commit regex was case-sensitive; the fixture README says "Commit `abc1234`".
    def test_commit_of_is_case_insensitive(self):
        self.assertEqual(pins.commit_of(os.path.join(self.root, "m")), "abc1234")
        self.write("m/README.md", "Analysed at commit `def5678`.\n")
        self.assertEqual(pins.commit_of(os.path.join(self.root, "m")), "def5678")


if __name__ == "__main__":
    unittest.main()


class ImpactThroughDiffs(Fixture):
    """`impact --rev X` with X other than the analysed commit A: citations
    (line numbers at A) are carried through the A→X diff before they are
    compared with the X→working-tree diff."""

    def commit(self, msg):
        if not os.path.isdir(os.path.join(self.root, ".git")):
            git(self.root, "init", "-q")
        git(self.root, "add", "-A")
        git(self.root, "commit", "-qm", msg)
        return subprocess.run(["git", "-C", self.root, "rev-parse", "HEAD"], capture_output=True, text=True, check=True).stdout.strip()

    # -- the line mapping (hunks are (a_start, a_count, b_start, b_count))
    def test_lines_shift_past_an_insertion_at_the_top(self):
        self.assertEqual(pins.map_range([(0, 0, 1, 3)], 4, 11), (7, 14))

    def test_an_insertion_after_a_line_shifts_only_later_lines(self):
        self.assertEqual(pins.map_range([(5, 0, 6, 2)], 4, 11), (4, 13))
        self.assertEqual(pins.map_range([(5, 0, 6, 2)], 1, 5), (1, 5))

    def test_a_changed_line_widens_to_its_new_lines(self):
        self.assertEqual(pins.map_range([(6, 2, 6, 3)], 4, 11), (4, 12))
        self.assertEqual(pins.map_range([(6, 2, 6, 3)], 6, 6), (6, 8))

    def test_deleted_lines_become_the_point_where_they_were(self):
        self.assertEqual(pins.map_range([(6, 2, 5, 0)], 6, 7), (5, 5))
        self.assertEqual(pins.map_range([(6, 2, 5, 0)], 4, 11), (4, 9))
        self.assertEqual(pins.map_range([(1, 2, 0, 0)], 1, 1), (1, 1))
        self.assertEqual(pins.map_range([(1, 2, 0, 0)], 3, 5), (1, 3))

    def test_several_hunks_accumulate(self):
        hunks = [(0, 0, 1, 2), (3, 1, 5, 0), (9, 0, 11, 4)]
        self.assertEqual(pins.map_range(hunks, 5, 8), (6, 9))
        self.assertEqual(pins.map_range(hunks, 10, 10), (15, 15))

    # -- end to end
    def test_impact_from_a_later_rev_carries_citations_through_the_diff(self):
        base = self.commit("analysed")
        self.write("src/graph.rs", "// pad\n" * 10 + GRAPH_RS)  # connect → 14-21, write_atomic → 27-29
        later = self.commit("later")
        lines = ("// pad\n" * 10 + GRAPH_RS).splitlines()
        lines[27] = "    let _ = path; // now fsyncs too"  # write_atomic's body (X line 28)
        lines[4] = "// changed padding"                   # X line 5: inside A's 4-11, but not connect
        self.write("src/graph.rs", "\n".join(lines) + "\n")
        _, m, _ = self.load()
        r = pins.impact(self.root, later, os.path.join(self.root, "SPEC.md"), m, base=base)
        self.assertEqual(sorted(r["code"]), ["G1.1"], r["code"])
        self.assertEqual(r["code"]["G1.1"], ["src/graph.rs:17-19 → 27-29 ∩ 28-28"])
        self.assertEqual(r["base"], base)
        self.assertEqual(r["unmapped"], {})

    def test_impact_at_the_analysed_commit_is_unchanged(self):
        base = self.commit("analysed")
        self.write("src/graph.rs", GRAPH_RS.replace("// writes", "// atomically writes"))
        _, m, _ = self.load()
        r = pins.impact(self.root, base, os.path.join(self.root, "SPEC.md"), m, base=base)
        self.assertEqual(r["code"], {"G1.1": ["src/graph.rs:17-19 ∩ 18-18"]})

    def test_a_cited_file_deleted_before_the_rev_is_unmapped(self):
        base = self.commit("analysed")
        os.remove(os.path.join(self.root, "src/graph.rs"))
        later = self.commit("drop graph.rs")
        _, m, _ = self.load()
        r = pins.impact(self.root, later, os.path.join(self.root, "SPEC.md"), m, base=base)
        self.assertEqual(r["unmapped"], {"G1": ["src/graph.rs:4-11"], "G1.1": ["src/graph.rs:17-19"]})
        self.assertEqual(r["code"], {})

    def test_cli_carries_citations_from_the_readme_commit(self):
        base = self.commit("analysed")
        self.write("m/README.md", README.replace("abc1234", base[:12]))
        self.write("src/graph.rs", "// pad\n" * 10 + GRAPH_RS)
        later = self.commit("later")
        self.write("src/graph.rs", ("// pad\n" * 10 + GRAPH_RS).replace("// writes", "// atomically writes"))
        p = subprocess.run([sys.executable, LAUNCHER, "impact", "m", "--rev", later], cwd=self.root, capture_output=True, text=True)
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn(f"citations carried from {base[:12]} to {later}", p.stdout)
        self.assertIn("G1.1", p.stdout)
        self.assertNotIn("\nG1 ·", p.stdout)
