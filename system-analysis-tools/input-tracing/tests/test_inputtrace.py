"""Tests for inputtrace. They write a tiny Dioxus-shaped crate to a temporary
directory, so no cargo or indexer is needed. Run with:

    python3 -m unittest discover -s system-analysis-tools/input-tracing/tests
"""
import contextlib
import io
import json
import os
import subprocess
import sys
import tempfile
import textwrap
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from inputtrace_lib import cli, model, trace  # noqa: E402

STATE_RS = textwrap.dedent('''\
    pub struct App { pub graph: Signal<Graph>, pub selected: Signal<Option<u64>>, pub toast: Signal<Option<String>> }

    impl App {
        pub fn select(mut self, id: Option<u64>) {
            self.selected.set(id);
        }
        pub fn edit(mut self, f: impl FnOnce(&mut Graph)) {
            f(&mut self.graph.write());
        }
        pub fn peek_only(self) -> usize {
            self.graph.peek().len()
        }
    }

    pub struct Pool;
    impl Pool {
        // a crate method that shares its name with an iterator method
        pub fn collect(&self) {}
    }
''')

WORKER_RS = textwrap.dedent('''\
    pub enum Msg { Done { ok: bool } }
    pub struct Worker;
    impl Worker {
        pub fn run(&self) {
            let tx = self.tx.clone();
            run_background(move || {
                let _ = tx.unbounded_send(Msg::Done { ok: true });
            });
        }
    }
    fn run_background(work: impl FnOnce() + Send + 'static) {
        std::thread::spawn(work);
    }
    pub fn home() -> Option<String> {
        std::env::var_os("FAKE_HOME").map(|s| s.to_string_lossy().into_owned())
    }
''')

MOD_RS = textwrap.dedent('''\
    #[component]
    fn Root() -> Element {
        let app = use_context::<App>();
        let worker = use_context::<WorkerHandle>();
        let mut open = use_signal(|| false);
        let set = move |v: i32| app.edit(|g| g.bump(v));
        let handler = move |_| open.set(true);
        use_effect(move || {
            let g = app.graph.read();
            worker.0.run();
        });
        let names: Vec<String> = vec![].into_iter().collect();
        rsx! {
            button { class: "btn", title: "Do it", onclick: move |_| app.select(None), "Go" }
            input { r#type: "text", oninput: move |e| set(1) }
            button { onclick: handler, "Open" }
        }
    }

    async fn pump(mut app: App, mut rx: Rx) {
        while let Some(msg) = rx.next().await {
            match msg {
                Msg::Done { ok } => {
                    app.toast.set(None);
                }
            }
        }
    }
''')


def make_crate(root):
    os.makedirs(os.path.join(root, "src", "ui"))
    for rel, text in [("src/ui/state.rs", STATE_RS), ("src/worker.rs", WORKER_RS), ("src/ui/mod.rs", MOD_RS)]:
        with open(os.path.join(root, rel), "w") as f:
            f.write(text)


def line_of(text, needle):
    return next(i for i, l in enumerate(text.splitlines(), 1) if needle in l)


class InputTraceTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        cls.root = cls.tmp.name
        make_crate(cls.root)
        cls.idx = model.build(cls.root)

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def handler(self, needle):
        line = line_of(MOD_RS, needle)
        e = self.idx.entry(f"src/ui/mod.rs:{line}")
        self.assertIsNotNone(e, needle)
        self.assertEqual(e.kind, "handler")
        return e

    def test_handlers_and_their_elements(self):
        e = self.handler('"Go"')
        self.assertEqual(e.detail, "onclick")
        self.assertEqual(e.element["name"], "button")
        self.assertEqual(e.element["class"], "btn")
        self.assertEqual(e.element["label"], "Go")
        self.assertEqual(e.fn, "Root")
        handlers = [x for x in self.idx.entries if x.kind == "handler"]
        self.assertEqual(len(handlers), 3)

    def test_method_calls_resolve_by_receiver(self):
        ed = model.entry_edges(self.idx, self.handler('"Go"'))
        self.assertEqual([f.qual for f, _ in ed.calls], ["App::select"])

    def test_calls_through_a_local_closure_are_followed(self):
        ed = model.entry_edges(self.idx, self.handler("oninput"))
        self.assertIn("App::edit", [f.qual for f, _ in ed.calls])

    def test_a_handler_bound_to_a_local_uses_that_body(self):
        e = self.handler('"Open"')
        self.assertEqual(e.via, "handler")
        self.assertEqual(model.entry_edges(self.idx, e).writes, ["Root.open"])

    def test_std_method_names_do_not_resolve_to_crate_methods(self):
        root = self.idx.fn("Root")
        calls = [f.qual for f, _ in model.fn_edges(self.idx, root).calls]
        self.assertNotIn("Pool::collect", calls)

    def test_signal_writes_and_subscribing_reads(self):
        self.assertEqual(model.fn_edges(self.idx, self.idx.fn("App::select")).writes, ["App.selected"])
        self.assertEqual(model.fn_edges(self.idx, self.idx.fn("App::edit")).writes, ["App.graph"])
        # .peek() does not subscribe
        self.assertEqual(model.fn_edges(self.idx, self.idx.fn("App::peek_only")).reads, [])
        subs = model.subscribers(self.idx)
        self.assertEqual([e.kind for e in subs["App.graph"]], ["effect"])
        self.assertNotIn("App.selected", subs)

    def test_channel_send_leads_to_the_receiving_arm(self):
        self.assertEqual([a.fn for a in self.idx.arms["Done"]], ["pump"])
        ed = model.fn_edges(self.idx, self.idx.fn("Worker::run"))
        self.assertEqual([v for v, _ in ed.sends], ["Done"])
        self.assertIn("thread", model.fn_edges(self.idx, self.idx.fn("run_background")).spawns)

    def test_trace_crosses_reactive_and_channel_hops(self):
        tree = trace.render(trace.build(self.idx, self.handler("oninput")))
        self.assertIn("call  App::edit", tree)
        self.assertIn("react App.graph → use_effect in Root", tree)
        self.assertIn("call  Worker::run", tree)
        self.assertIn("send  Msg::Done → received in pump", tree)
        self.assertIn("writes App.toast", tree)

    def test_env_reads(self):
        env = [e for e in self.idx.entries if e.kind == "env"]
        self.assertEqual([e.detail for e in env], ["env FAKE_HOME"])

    def test_coverage_reads_both_citation_styles_and_skips_inventories(self):
        docs = os.path.join(self.root, "docs")
        os.makedirs(docs)
        go = line_of(MOD_RS, '"Go"')
        inp = line_of(MOD_RS, "oninput")
        with open(os.path.join(docs, "trace.md"), "w") as f:
            f.write(f"Clicking Go ([mod.rs:{go}](../src/ui/mod.rs#L{go})); typing: src/ui/mod.rs:{inp}.\n")
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            rc = cli.main(["coverage", self.root, docs, "--kind", "handler"])
        self.assertEqual(rc, 1)
        self.assertIn("2/3", out.getvalue())
        self.assertIn('"Open"'.strip('"'), out.getvalue())
        # an inventory that cites everything does not count
        with open(os.path.join(docs, "inventory.md"), "w") as f:
            f.write(" ".join(f"src/ui/mod.rs:{e.line}" for e in self.idx.entries))
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(cli.main(["coverage", self.root, docs, "--kind", "handler"]), 1)


LAUNCHER = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "inputtrace")


def write_crate(root, files):
    for rel, text in files.items():
        path = os.path.join(root, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as f:
            f.write(textwrap.dedent(text))


def run_cli(argv):
    """(exit code, stdout, stderr) of `cli.main(argv)`; an argparse error
    gives its SystemExit code."""
    out, err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        try:
            rc = cli.main(argv)
        except SystemExit as e:
            rc = e.code
    return rc, out.getvalue(), err.getvalue()


RELAY_RS = '''\
    pub enum Msg { Done, Again }
    async fn pump(mut rx: Rx, tx: Tx) {
        while let Some(msg) = rx.next().await {
            match msg {
                Msg::Done => {
                    let _ = tx.unbounded_send(Msg::Again);
                }
                Msg::Again => {}
            }
        }
    }
'''

RECV_CALL_RS = '''\
    pub enum Msg { Done }
    fn pump(rx: Receiver<Msg>) {
        loop {
            match rx.recv() {
                Msg::Done => break,
            }
        }
    }
'''

ARM_SHAPES_RS = '''\
    pub enum Msg { A, B, C }
    fn pump(rx: Receiver<Msg>) {
        loop {
            match rx.try_recv() {
                Err(_) => break,
                Ok(Msg::A) | Ok(Msg::B) => {}
                Ok(Msg::C) => {}
            }
        }
    }
'''

SELF_RS = '''\
    pub struct Graph;
    impl Graph {
        pub fn new() -> Self { Graph }
    }
    impl Default for Graph {
        fn default() -> Self { Self::new() }
    }
'''

HANDLER_WRITES_RS = '''\
    #[component]
    fn Root() -> Element {
        let mut open = use_signal(|| false);
        let mut count = use_signal(|| 0);
        count.set(1);
        let close = move |_| open.set(false);
        rsx! {
            button { onclick: move |_| open.set(true), "Open" }
            button { onclick: close, "Close" }
        }
    }
'''

TWO_BUTTONS_RS = '''\
    #[component]
    fn App() -> Element {
        rsx! {
            button { onclick: move |_| {}, "A" }
            button { onclick: move |_| {}, "B" }
        }
    }
'''


class RegressionTest(unittest.TestCase):
    """One test per bug fixed after the documentation audit."""

    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.root = self._tmp.name

    def tearDown(self):
        self._tmp.cleanup()

    def crate(self, files):
        write_crate(self.root, files)
        return model.build(self.root)

    def docs(self, files=None):
        d = os.path.join(self.root, "docs")
        os.makedirs(d)
        for name, text in (files or {}).items():
            with open(os.path.join(d, name), "w") as f:
                f.write(text)
        return d

    # 1: local signals were kept in a set, so their order followed the hash seed
    def test_output_does_not_depend_on_the_hash_seed(self):
        names = ["pan", "zoom", "drag", "hover", "menu", "size", "tick", "focus"]
        lets = "".join(f"    let mut {n} = use_signal(|| 0);\n" for n in names)
        sets = " ".join(f"{n}.set(1);" for n in names)
        write_crate(self.root, {"src/ui/view.rs": (
            "#[component]\nfn View() -> Element {\n" + lets
            + "    rsx! { div { onclick: move |_| { " + sets + " }, \"x\" } }\n}\n")})
        handler = f"src/ui/view.rs:{len(names) + 3}"
        outs = set()
        for seed in ("1", "2", "3", "4", "5"):
            env = dict(os.environ, PYTHONHASHSEED=seed)
            res = []
            for cmd in (["inventory", self.root], ["signals", self.root, "--json"], ["trace", self.root, handler]):
                p = subprocess.run([sys.executable, LAUNCHER] + cmd, env=env, capture_output=True, text=True)
                self.assertEqual(p.returncode, 0, p.stderr)
                res.append(p.stdout)
            outs.add(tuple(res))
        self.assertEqual(len(outs), 1, "output differs between PYTHONHASHSEED values")
        # source order
        self.assertIn(", ".join(f"`View.{n}`" for n in names), next(iter(outs))[0])

    # 2: a wrong root or --src gave `0/0 …` and exit 0
    def test_a_missing_source_directory_is_an_error(self):
        write_crate(self.root, {"src/lib.rs": "pub fn f() {}\n"})
        docs = self.docs()
        missing = os.path.join(self.root, "nope")
        for argv in (["coverage", missing, docs], ["--src", "crates/app/src", "coverage", self.root, docs],
                     ["inventory", missing], ["signals", missing], ["channels", missing], ["trace", missing, "f"]):
            rc, out, err = run_cli(argv)
            self.assertEqual(rc, 2, argv)
            self.assertIn("source directory not found", err, argv)
            self.assertNotIn("0/0", out, argv)

    def test_coverage_with_no_entry_points_fails(self):
        write_crate(self.root, {"src/lib.rs": "pub fn f() {}\n"})
        rc, out, err = run_cli(["coverage", self.root, self.docs()])
        self.assertEqual(rc, 1, out)
        self.assertIn("no entry points", err)

    # 3: only `src/…` and `tests/…` citations were recognised
    def test_coverage_accepts_citations_outside_src(self):
        write_crate(self.root, {"crates/app/src/main.rs": TWO_BUTTONS_RS})
        docs = self.docs({"t.md": "A: crates/app/src/main.rs:4. B: [main.rs:5](../crates/app/src/main.rs#L5)\n"})
        rc, out, _ = run_cli(["--src", "crates/app/src", "coverage", self.root, docs])
        self.assertEqual(out.splitlines()[0], f"2/2 entry points cited by a document in {docs}")
        self.assertEqual(rc, 0)

    # 4: an unmatched file:line fell back to id substrings; ids beat fn names
    def test_trace_does_not_fall_back_to_substrings_or_guess(self):
        lines = ["#[component]", "fn Root() -> Element {", "    rsx! {"]
        lines += ["        div {}"] * 10
        lines += ['        input { onchange: move |_| {}, "x" }', "    }", "}",
                  "pub fn new() {}",
                  "pub struct A;", "impl A { pub fn go(&self) {} }",
                  "pub struct B;", "impl B { pub fn go(&self) {} }",
                  "pub fn load() { let _ = ZipArchive::new(f); }"]
        write_crate(self.root, {"src/ui/dialogs.rs": "\n".join(lines) + "\n"})
        # line 1 is inside no entry: no match, not the entry at line 14
        rc, out, err = run_cli(["trace", self.root, "src/ui/dialogs.rs:1"])
        self.assertEqual(rc, 1, out)
        self.assertIn("no entry or fn matches", err)
        rc, out, _ = run_cli(["trace", self.root, "src/ui/dialogs.rs:14"])
        self.assertEqual(rc, 0)
        self.assertTrue(out.startswith("handler  onchange"), out)
        # an exact fn name beats the id substring of `fs:ZipArchive::new`
        rc, out, _ = run_cli(["trace", self.root, "new"])
        self.assertEqual(rc, 0)
        self.assertTrue(out.startswith("fn  new"), out)
        # an ambiguous name is an error that lists the candidates
        rc, out, err = run_cli(["trace", self.root, "go"])
        self.assertEqual(rc, 1, out)
        self.assertIn("A::go", err)
        self.assertIn("B::go", err)

    # follow-up to 4: a duplicate name is reachable by FILE:LINE on the fn
    def test_file_line_outside_any_entry_selects_the_enclosing_fn(self):
        lines = ["#[component]", "fn Root() -> Element {", "    let x = 1;", "    rsx! {",
                 '        input { onchange: move |_| {}, "x" }', "    }", "}",
                 "pub struct A;", "impl A { pub fn go(&self) {} }",
                 "pub struct B;", "impl B {", "    pub fn go(&self) {", "        let y = 2;", "    }", "}"]
        write_crate(self.root, {"src/ui/dialogs.rs": "\n".join(lines) + "\n"})
        rc, out, _ = run_cli(["trace", self.root, "src/ui/dialogs.rs:13"])
        self.assertEqual(rc, 0, out)
        self.assertTrue(out.startswith("fn  B::go"), out)
        # inside a component but outside its handlers: the component
        rc, out, _ = run_cli(["trace", self.root, "src/ui/dialogs.rs:3"])
        self.assertEqual(rc, 0, out)
        self.assertTrue(out.startswith("fn  Root"), out)
        # an entry still wins over its enclosing fn
        rc, out, _ = run_cli(["trace", self.root, "src/ui/dialogs.rs:5"])
        self.assertTrue(out.startswith("handler  onchange"), out)
        # a line in an impl header but in no fn: still no match
        rc, _, err = run_cli(["trace", self.root, "src/ui/dialogs.rs:11"])
        self.assertEqual(rc, 1)
        self.assertIn("no entry or fn matches", err)

    # follow-up to 7/8: arms that wrap or alternate variants
    def test_wrapped_and_alternative_arm_patterns_receive(self):
        idx = self.crate({"src/pump.rs": ARM_SHAPES_RS})
        received = {v: [a.fn for a in arms] for v, arms in idx.arms.items()}
        self.assertEqual(received, {"A": ["pump"], "B": ["pump"], "C": ["pump"]})
        self.assertEqual(model.fn_edges(idx, idx.fn("pump")).sends, [])

    # 5: `Self::name(` was never resolved
    def test_self_paths_resolve_to_the_impl_type(self):
        idx = self.crate({"src/graph.rs": SELF_RS})
        self.assertEqual([f.qual for f, _ in model.fn_edges(idx, idx.fn("Graph::default")).calls], ["Graph::new"])
        self.assertIn("call  Graph::new", trace.render(trace.build(idx, idx.fn("Graph::default"))))

    # 6: a send node cut off by --depth had no `…`
    def test_a_send_cut_off_by_depth_is_marked(self):
        make_crate(self.root)
        idx = model.build(self.root)
        tree = trace.render(trace.build(idx, idx.fn("Worker::run"), trace.Options(depth=1)))
        self.assertIn("send  Msg::Done → received in pump  …", tree)

    # 7: a fn with a receiving match was never a sender
    def test_a_receiving_arm_that_sends_is_a_sender(self):
        idx = self.crate({"src/pump.rs": RELAY_RS})
        done = idx.arms["Done"][0]
        self.assertEqual([v for v, _ in model.arm_edges(idx, done).sends], ["Again"])
        self.assertEqual([v for v, _ in model.fn_edges(idx, idx.fn("pump")).sends], ["Again"])
        self.assertIn("send  Msg::Again → received in pump", trace.render(trace.build(idx, idx.fn("pump"))))
        _, out, _ = run_cli(["channels", self.root])
        self.assertIn("Msg::Again\n    sent by     pump [src/pump.rs:6]", out)
        self.assertNotIn("Msg::Done\n    sent by", out)

    # 8: `match rx.recv() {` was not a receiving match
    def test_a_match_on_a_call_receives(self):
        idx = self.crate({"src/pump.rs": RECV_CALL_RS})
        self.assertEqual([(a.fn, a.line) for a in idx.arms.get("Done", [])], [("pump", 5)])
        self.assertEqual(model.fn_edges(idx, idx.fn("pump")).sends, [])

    # 9: `--receiver NAME` silently disabled the mapping
    def test_a_receiver_without_a_type_is_a_usage_error(self):
        write_crate(self.root, {"src/lib.rs": "pub fn f() {}\n"})
        for bad in ("app", "=App", "app="):
            rc, _, err = run_cli(["--receiver", bad, "inventory", self.root])
            self.assertEqual(rc, 2, bad)
            self.assertIn("NAME=Type", err)

    # 10: `signals --json` repeated writers
    def test_signals_json_lists_each_writer_once(self):
        reset = "pub fn reset(app: App) { app.toast.set(None); }\n"
        self.crate({"src/a.rs": reset, "src/b.rs": reset})
        _, out, _ = run_cli(["signals", self.root, "--json"])
        self.assertEqual(json.loads(out)["App.toast"]["writers"], ["reset"])

    # 11: a write in a handler was listed for the component too
    def test_a_write_in_a_handler_is_attributed_to_the_handler_only(self):
        idx = self.crate({"src/ui/mod.rs": HANDLER_WRITES_RS})
        wr = model.writers(idx)
        self.assertEqual(sorted(w[0] for w in wr["Root.open"]), ["src/ui/mod.rs:8:onclick", "src/ui/mod.rs:9:onclick"])
        self.assertEqual([w[0] for w in wr["Root.count"]], ["Root"])


if __name__ == "__main__":
    unittest.main()
