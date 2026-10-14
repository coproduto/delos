# input-tracing: `inputtrace`

`inputtrace` maps where input enters a Rust + Dioxus program and follows
each input to where it ends. It reads source text only. It needs no cargo
build, no indexer and no third-party Python packages.

- Exact options, schemas and exit codes: [REFERENCE.md](REFERENCE.md).
- Step-by-step procedure for an AI agent: [AGENTS.md](AGENTS.md).

## Contents

- [Purpose](#purpose)
- [Concepts and terminology](#concepts-and-terminology)
- [Requirements and installation](#requirements-and-installation)
- [Quick start](#quick-start)
- [Typical workflows](#typical-workflows)
- [Interpreting results](#interpreting-results)
- [Limitations and accuracy](#limitations-and-accuracy)
- [Troubleshooting](#troubleshooting)
- [Development](#development)

## Purpose

**The question it answers:** "When this input arrives, what runs, what
state changes, and where does the work end?"

It supports an **input-trace model**: a set of documents that explain a
system by following every input from the moment it enters (a click, a key,
a dropped file, an environment variable, a dialog answer) to where it ends
(a changed document, a render, a file on disk, a message on screen). A
human uses the model to decide things: what an input changes, which steps
are undoable or saved, where validation happens, and where work crosses a
thread. The Delos model built with this tool is in
[`system-models/input-tracing/`](../../system-models/input-tracing/README.md).

The tool does four jobs:

1. **Inventory**: list every entry point (`inventory`).
2. **Trace**: print the tree of everything that *may* run from one entry
   point or function (`trace`).
3. **Show the shared machinery**: which signals rerun which effects and who
   writes them (`signals`); which message variants are sent where and
   received where (`channels`).
4. **Keep the model honest**: report entry points that no model document
   cites (`coverage`). Use it as a CI gate.

**Use it when** you want to:

- build or update an input-trace model of a Dioxus application;
- answer "who writes this signal?" or "what does this button really do?";
- find writers of shared state that bypass the official edit path;
- check that every UI input is documented after a UI change.

**Do not use it when** you need:

- **exact** call graphs or type resolution. Use rust-analyzer or a
  compiler-backed tool. `inputtrace` resolves calls heuristically and drops
  what it cannot resolve.
- what *does* run. The tree shows what *may* run: it ignores `cfg`
  attributes and branches. Read the code before you write a trace.
- the computational core (render pipelines, algorithms). Traces stop
  being informative once they leave the application layer.
- a non-Dioxus program. Other frameworks need code changes (see
  [Extending to other frameworks](#other-frameworks)).

## Concepts and terminology

### Entry points

An **entry point** is a place in the source where something outside the
program reaches it. Each has a **kind**:

| Kind | What it is | Detected from |
|---|---|---|
| `handler` | A UI event handler. | An `on<letters>:` attribute inside an `rsx!` block, for example `onclick: move \|e\| …` or `onclick: some_local`. The tool also records the element's tag and its `class`, `title`, `type`, `value`, `accept` and first text child. |
| `effect` | A reactive closure. It is not an input itself, but it is where edits turn into work. | `use_effect(…)` and `use_memo(…)` calls. |
| `env` | An environment variable read. | `env::var(…)` and `env::var_os(…)`. The variable name is the first string literal after the call. |
| `fs` | A file-system read. | `fs::read(`, `fs::read_to_string(`, `fs::read_dir(`, `File::open(`, `ZipArchive::new(`, `read_bytes(`. |
| `dialog` | A native dialog whose answer flows back in. | `rfd::Async…Dialog` (for example `rfd::AsyncFileDialog`). |
| `webview` | JavaScript evaluation or a custom protocol handler. | `document::eval(`, `use_asset_handler(`. |

An **entry id** names one entry point: `file:line:kind-specific-suffix`.
Examples from Delos:

| Kind | Example id |
|---|---|
| handler | `src/ui/dialogs.rs:193:onclick` (suffix: the event name) |
| effect | `src/ui/image_view.rs:42:use_memo` (suffix: `use_effect` or `use_memo`) |
| env | `src/engine/gpu.rs:94:env:DELOS_NO_GPU` (suffix: `env:` + variable name) |
| fs | `src/project.rs:307:fs:ZipArchive::new` (suffix: `fs:` + the call) |
| dialog | `src/ui/platform.rs:123:dialog` |
| webview | `src/ui/platform.rs:40:webview` |

Each entry point also has a **span**: the lines `line`–`end_line` that
`coverage` and `trace FILE:LINE` match against. For a handler it is the
attribute and its value. For an effect it is the whole `use_effect(…)` call.
The other kinds are one line.

A handler written as `onclick: start` is **bound via a local**. The tool
finds the last `let start = …` before it in the same function and uses that
expression as the handler's body. The description then ends in
``(via `start`)``.

### Edges

An **edge** leads from one body of code (a handler, an effect, a function,
a `match` arm) to another. A trace follows three kinds:

| Edge | Label in a trace | Meaning |
|---|---|---|
| call | `call` | A call that resolves to a function of the crate. Calls to `std` and other crates are not shown. |
| send | `send` | The body mentions `Msg::X`. The edge leads to the `match` arm that receives `Msg::X`. |
| react | `react` | The body writes a signal. The edge leads to every effect that **subscribes** to that signal. |

**Call resolution** is heuristic. The tool looks at what precedes the call:

| Call shape | Resolved to |
|---|---|
| `recv.name(` with `recv` in the receiver map (`app` → `App`, `g` → `Graph`, …) | `Type::name`, if that method exists. Otherwise dropped. |
| `x.0.name(` (a newtype handle) | `Worker::name` |
| `self.name(` | `Type::name` for the impl that contains the caller |
| `other.name(` (unmapped receiver) | The only crate method called `name`, unless `name` is on the block list of common `std` and signal names. Otherwise dropped. |
| `Type::name(` | `Type::name` |
| `Self::name(` | `Type::name` for the impl that contains the caller |
| `module::name(` | The only `name` defined in `module.rs` or `module/mod.rs` |
| `name(` | A free function `name` in the same file, else the only free function `name` in the crate. Else a local closure (see below). |

A call to a **local closure** (`let set = move |v| …; set(x)`) is *inlined*:
the closure's calls, sends and writes count as the caller's. A closure
passed as an argument (`app.edit(|g| g.bump())`) is part of the caller's
body, so `Graph::bump` appears as a call of the caller.

### Signals

A **signal id** names one piece of reactive state:

| Form | Meaning | Example |
|---|---|---|
| `App.field` | A `Signal` field of the state struct (`App` by default), written as `app.field.…` or, inside `impl App`, `self.field.…`. | `App.graph` |
| `Component.local` | A local `let x = use_signal(…)` or `let x = use_memo(…)` in a function. | `GraphView.drag`, `ImageView.disp` |

A parameter `x: Signal<…>` is **aliased** to the local signal `x` of the
one function in the same file that declares it. That is usually the
component that passes it down. If no function or more than one declares
`x`, the parameter gets its own id `Fn.x`.

A body **writes** a signal when it calls `.set(`, `.write(`, `.take(`,
`.toggle(` or `.with_mut(` on it.

An effect **subscribes** to a signal when it reads it with `.read(`,
`.cloned(` or `.with(`, or calls a local signal as a function (`x()`). The
read can be direct, inside a function the effect calls (up to 6 calls
deep), or inside a memo the effect reads. **`.peek()` does not subscribe.**
That matches Dioxus: `peek` reads without registering the reader.

### Channels

The tool assumes **one message enum**, `Msg`. A **receiving arm** is an
arm `Msg::X … => …` of a `match EXPR {` block whose first arm is a `Msg::`
pattern. The scrutinee can be any expression: `match msg {`,
`match rx.recv() {`. A **send** is any other mention of `Msg::X`, including
one inside a receiving arm's body (an arm that re-sends another message).

### Hand-offs

The tool notes when a body **hands off** work:

- **to a background thread**: `thread::spawn`, `thread::Builder` or
  `run_background` appears in the body;
- **to an async task**: `spawn(async …` or `spawn_local(` appears in the
  body.

### May-paths

A trace shows every path that **may** run. It ignores `cfg` attributes,
`if`/`match` branches and early returns. Treat it as a map of where to
read, not as the trace itself.

## Requirements and installation

- Python 3.9 or later, standard library only. Tested with 3.9.6 and 3.14.3
  on macOS. Other platforms have not been tested.

There is nothing to install. Run the launcher from this directory, or call
it through a symbolic link on your `PATH` (it resolves its real location):

```sh
ln -s "$PWD/system-analysis-tools/input-tracing/inputtrace" ~/bin/inputtrace
```

If the launcher is not executable, run it with `python3 path/to/inputtrace …`.

## Quick start

All commands run from the Delos project root.

```sh
T=system-analysis-tools/input-tracing/inputtrace
```

**1. List every entry point.**

```console
$ $T inventory . | head -18
# Input inventory

140 entry points found by `inputtrace inventory`.

| Kind | Count |
|---|---|
| handler | 105 |
| effect | 6 |
| env | 8 |
| fs | 14 |
| dialog | 5 |
| webview | 2 |

## handler

| Where | Component / fn | Input | Calls | Writes |
|---|---|---|---|---|
| `src/ui/dialogs.rs:42` | `ConflictDialog` | onclick on <button class="btn primary"> “Use {fmt_size(sa)} (resize {nb})” | `App::edit`, `Graph::insert_resize` | `App.conflict` |
```

**2. Trace one input end to end** (the toolbar Import button):

```console
$ $T trace . src/ui/mod.rs:351
handler  onchange on <input type="file">  in Toolbar   [src/ui/mod.rs:351]
└─ call  import   [src/ui/session.rs:100]
       · writes App.project_prompt, App.session
   ├─ call  start_imports   [src/ui/session.rs:276]
   │      · writes App.importing, App.imports
   │      · hands off to an async task
   │  ├─ call  Worker::import   [src/worker.rs:242]
   │  │      · hands off to a background thread
   │  │  ├─ call  decode   [src/formats.rs:183]
   …
   │  │  └─ send  Msg::Imported → received in pump   [src/ui/mod.rs:180]
   │  │         · writes App.importing, App.imports
   │  │     ├─ call  finish_import   [src/ui/mod.rs:251]
   …
   ├─ react App.session → use_effect in Root   [src/ui/mod.rs:63]
   └─ react App.session → use_effect in use_autosave   [src/ui/session.rs:251]
          · writes App.save_state
      ├─ call  Worker::save_project   [src/worker.rs:356]
   …
```

**3. See which signals rerun which effects, and who writes them.**

```console
$ $T signals . | sed -n '/^App.graph$/,/written/p'
App.graph
    reruns  use_memo in ImageView → `disp`  [src/ui/image_view.rs:42]
    reruns  use_effect in ImageView  [src/ui/image_view.rs:51]
    reruns  use_effect in ImageView  [src/ui/image_view.rs:70]
    reruns  use_effect in ImageView  [src/ui/image_view.rs:83]
    reruns  use_effect in use_autosave  [src/ui/session.rs:251]
    written by 8: App::edit, App::edit_coalesced, App::fit_canvas_to_window, App::redo, App::replace_document, App::undo, src/ui/graph_view.rs:272:onpointermove, src/ui/mod.rs:432:onclick
```

**4. See each message variant's senders and receivers.**

```console
$ $T channels . | head -6
Msg::ExportDone
    sent by     Worker::export [src/worker.rs:319]
    received in pump [src/ui/mod.rs:199]
Msg::ExportProgress
    sent by     Worker::export [src/worker.rs:290]
    received in pump [src/ui/mod.rs:188]
```

**5. Check that the model cites every entry point.**

```console
$ $T coverage . system-models/input-tracing
126/126 entry points cited by a document in system-models/input-tracing
$ echo $?
0
```

## Typical workflows

### Build a model from scratch

1. Generate the inventory into the model directory:

   ```sh
   M=system-models/input-tracing
   $T inventory . --md $M/inventory.md --json $M/inventory.json
   ```

   The output is deterministic, so regenerated files diff cleanly.
2. Run `$T signals .` and `$T channels .`. Write the shared machinery down
   once (in Delos: `00-spine.md`).
3. Group entry points by where the user is (a toolbar, a view, a dialog).
   For each group, trace one representative with `$T trace . FILE:LINE`,
   then read every branch the tree passes through and write numbered steps
   that cite `path:line`.
4. Run `$T coverage . $M` until it exits 0.

[AGENTS.md](AGENTS.md) has the full procedure, with what to look for.

### Keep the model current after UI changes

```sh
$T inventory . --md $M/inventory.md --json $M/inventory.json
git diff --stat $M/inventory.md        # which entry points appeared, moved or vanished
$T coverage . $M                       # which ones no document cites
```

Line numbers shift when code moves, so citations go stale. `coverage`
reports an entry as `uncited` when no citation overlaps its span. Fix the
citation, or trace the new input and add it to the right document.
`coverage` does not detect citations that point at the wrong code.

### CI gate

`coverage` exits 0 only when every checked entry point is cited:

```sh
system-analysis-tools/input-tracing/inputtrace coverage . system-models/input-tracing
```

It exits 1 when any entry point is uncited, and also when it finds no
entry points of the checked kinds at all (nothing to check is a failure,
not a pass). A root or `--src` directory that does not exist is an error
(exit 2). So a wrong path cannot pass the gate.

By default `coverage` counts `handler`, `effect`, `env`, `dialog` and
`webview` entries. It skips `fs`, because file reads are usually covered
by the handler that leads to them. Add `--kind fs` to check them.

### Use it on another Dioxus project

Global options go **before** the subcommand.

- **Source roots.** `--src DIR` replaces the default `src`. Repeat it for
  several roots:

  ```sh
  inputtrace --src crates/app/src --src crates/ui/src inventory /path/to/project
  ```

- **Receiver names.** The default receiver map is tuned to Delos (`app`,
  `g`, `graph`, `ctx`, `engine`, `project`, `img`). Add your own with
  `--receiver NAME=Type`. On a scratch crate where both `Document` and
  `Cache` define `save_all`, the call `doc.save_all()` is ambiguous until
  you map `doc`:

  ```console
  $ inputtrace --src crates/app/src trace /tmp/toy main.rs:10
  handler  onclick on <button> “Save”  in Editor   [crates/app/src/main.rs:10]
  $ inputtrace --src crates/app/src --receiver doc=Document trace /tmp/toy main.rs:10
  handler  onclick on <button> “Save”  in Editor   [crates/app/src/main.rs:10]
  └─ call  Document::save_all   [crates/app/src/main.rs:3]
  ```

- **What you cannot set from the command line:** the state struct
  (`App`), its receiver name (`app`), the newtype inner type (`Worker`)
  and the message enum (`Msg`). Change the `Config` defaults in
  `inputtrace_lib/model.py`, or build the index from Python (see
  [REFERENCE.md](REFERENCE.md#configuration)).
- **Coverage on other layouts.** Cite files by their path from the
  project root, for example `crates/app/src/main.rs:10`. Any `.rs` path
  works (see [REFERENCE.md](REFERENCE.md#citation-syntax)).

## Interpreting results

### The trace tree

```text
handler  onchange on <input type="file">  in Toolbar   [src/ui/mod.rs:351]   ← root
└─ call  import   [src/ui/session.rs:100]                                     ← a call; brackets: where `import` is defined
       · writes App.project_prompt, App.session                                ← note on that node
   ├─ call  start_imports   [src/ui/session.rs:276]
   │  │  └─ send  Msg::Imported → received in pump   [src/ui/mod.rs:180]      ← brackets: the receiving arm
   │  │     │  ├─ react App.pinned → use_effect in ImageView  …   [src/ui/image_view.rs:51]
   │  └─ call  App::toast  (above)   [src/ui/state.rs:540]
```

- **Root line.** For an entry point: `KIND  DESCRIPTION  in FN   [file:line]`.
  For a function: `fn  Type::name   [file:line]`. Check the location: it
  tells you which entry the argument matched.
- **`call  Q`**: the body calls `Q`. The brackets give where `Q` is
  **defined**, not the call site.
- **`send  Msg::X → received in F`**: the body mentions `Msg::X`. The
  brackets give the receiving arm. The arm's own calls and writes follow
  as children.
- **`react  S → EFFECT`**: the body writes signal `S`, and `EFFECT`
  subscribes to `S`. The brackets give the effect. One write can rerun
  several effects. One effect can appear once per signal that reruns it.
- **`· writes A, B`**: signals this body writes directly (not through
  callees).
- **`· hands off to a background thread`** / **`an async task`**: the body
  spawns work. Everything after it may run on another thread or later.
- **`(above)`**: this function, arm or effect was already expanded earlier
  in this tree. Look for it higher up.
- **`…`**: the depth limit (`--depth`, default 6) was reached. This applies
  to `call`, `send` and `react` nodes. Trace that node on its own, or raise
  `--depth`.
- **`[not expanded]`**: the node matched `--stop` or `--collapse`.

Children appear in this order: calls (in order of first appearance), then
sends, then react edges.

### Inventory tables

The Markdown inventory has one table per kind. For `handler` and `effect`:

| Column | Meaning |
|---|---|
| Where | `file:line` of the attribute or the `use_effect`/`use_memo` call |
| Component / fn | The function that contains it |
| Input | The description: event, element, attributes, label (handlers); `use_effect in Fn` or ``use_memo in Fn → `name` `` (effects) |
| Calls | Resolved direct calls of the body, or `—` |
| Writes | Signals the body writes directly, or `—` |

For the other kinds the columns are *Where*, *fn* and *Input*. Labels keep
format placeholders as written, for example
“Use {fmt_size(sa)} (resize {nb})”.

### `signals` output

```text
App.selected
    reruns  use_memo in ImageView → `disp`  [src/ui/image_view.rs:42]
    …
    written by 5: App::delete_node, App::fix_selection, App::replace_document, App::select, src/ui/graph_view.rs:306:onpointerdown
```

- `reruns` lines list the effects that subscribe to the signal.
- `written by N:` lists functions (by qualified name) and handler or effect
  entries (by entry id) that write it directly. A write inside a handler or
  effect is listed once, under that entry's id. The component that contains
  it is listed only for writes in its own body, outside every handler and
  effect. The list shows at most 12 names, then `…`.
- A memo id (`ImageView.disp`) shows `written by 0:`. Memos are recomputed,
  not written.

### `channels` output

`sent by` gives the function and the line that mentions `Msg::X`.
`received in` gives the function and the line of the arm. A variant with
no `received in` line has no recognised receiving arm. A variant with no
`sent by` line is mentioned only in receiving arm patterns.

### `coverage` output

```text
6/14 entry points cited by a document in system-models/input-tracing
  uncited  src/project.rs:70  fs  fs::read_dir in Project::create  in Project::create
```

Each `uncited` line gives the entry's location, kind, description and
containing function.

## Limitations and accuracy

**Detection**

- Handlers are any `on<letters>:` key after the first `rsx!` in a function.
  A struct field such as `onset:` after an `rsx!` block would also count.
- Two entry points with the same id (same file, line and suffix) are
  reported once.
- `env` names come from the first string literal within 80 characters. A
  name held in a constant shows as `?` or as a wrong literal.
- Only `rfd::Async…Dialog` written with the `rfd::` prefix is found. Sync
  `rfd::FileDialog` and imported names are not.
- Not detected: `use_resource`, `use_future`, `use_coroutine`, stdin,
  network, timers, `fs::metadata` and other inputs not listed above.
- Only the directories in `--src` are read (default `src`). `tests/`,
  `build.rs` and other crates are ignored unless you add them. A `--src`
  directory that does not exist is an error.

**Call resolution**

- Resolution is by name. Ambiguous calls are **dropped**, not guessed, so
  a trace can miss real calls.
- Trait-object calls (`kind.size(…)`) resolve only if one crate method has
  that name and the name is not on the block list. The block list
  (`set`, `get`, `new`, `len`, `run`, `apply`, …) is in
  [REFERENCE.md](REFERENCE.md#fallback-block-list).
- Function names carry no module path. Two free functions `kinds` in
  different files share the qualified name `kinds`. `trace . kinds` is
  then ambiguous: it lists both and exits 1. Trace one by a line inside it
  (`trace . src/nodes/basic.rs:10`). A call to `kinds(` from a third file is
  dropped.
- A mapped receiver wins over everything: if `app` maps to `App` and
  `App::name` does not exist, the call is dropped.

**Reactivity**

- One state struct (`App`) with receiver `app` or `self`. Signals on other
  structs are not tracked.
- Parameter aliasing works only when exactly one function in the same file
  declares a same-named local signal.
- A state field subscribes only through `app.field.read(`,
  `app.field.cloned(` or `app.field.with(` (or `self.field.…` inside
  `impl App`). Other forms, such as `(app.field)()` or reading through a
  copied handle (`let g = app.graph; g.read()`), are missed.
- **No re-render edges.** A write also re-renders components that read the
  signal. Re-renders do not run side effects, so they are not modelled.

**Channels**

- One message enum, `Msg`.
- Any `Msg::X` mention outside a receiving arm's pattern counts as a send,
  including patterns such as `if let Msg::X = …`.
- Every arm of every `match` is read. Each `Msg::X` in an arm's pattern is
  a receive, whatever the pattern's shape: `Msg::X`, `Ok(Msg::X)`,
  `Msg::A | Msg::B` (both received). Arms without `Msg::` (`_`, `Err(_)`)
  are skipped.

**Tracing**

- May-paths only: `cfg`, branches, early returns and error paths are
  ignored.
- Each function, arm and effect is expanded once per trace, so the tree is
  not a list of separate paths.

**Coverage**

- A citation must name a `.rs` file by its path from the project root,
  optionally preceded by other directories (`../../src/ui/mod.rs:84`). A
  bare file name (`mod.rs:84`) does not count.
- A citation counts when its line range overlaps the entry's span. It does
  not check that the document describes the right code.
- Only `.md` files count. Files whose name matches `inventory*.md` are
  ignored.

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| `` no entry or fn matches 'X'; try `inventory` `` (exit 1) | No rule matches. A `file:line` whose line is inside no entry's span matches nothing (`file:line` matches entries only, not functions). | Use the `file:line` shown by `inventory`, a full entry id, or a qualified name (`Graph::add_node`). |
| `'X' is ambiguous; it matches N:` and a list (exit 1) | Several entries or functions match, for example `new` or `add_node` (methods of several types), or a free function name defined in several files. | Pick one from the list: use its entry id, its qualified name `Type::name`, or `FILE:LINE` of a line inside the function (the list shows where each one is). |
| `inputtrace: error: source directory not found: …` (exit 2) | Wrong root, or `--src` names a directory that does not exist. | Run from the project root, pass the root as the first positional argument, and check `--src`. |
| `0 entry points found`, or `coverage` says `inputtrace: no entry points of kind …` (exit 1) | The source directories exist but contain no entry points: `--src` points at the wrong directory of a project, or the project is not a Dioxus application. | Check `--src`. |
| `inputtrace: error: argument --receiver: expected NAME=Type, got 'X'` (exit 2) | `--receiver` without `=`, or with an empty name or type. | Write `--receiver doc=Document`. |
| `inputtrace: error: unrecognized arguments: --src src` | Global options were put after the subcommand. | Put `--src` and `--receiver` before the subcommand. |
| Everything uncited, although the documents cite it | The `docs` directory does not exist (it is not an error), or the citations give a bare file name instead of the path from the project root, or a link's text does not end in `:LINE`. | Check the path. Write citations as `src/ui/mod.rs:84` or `[mod.rs:84](../../src/ui/mod.rs#L84)`. |
| A call you expected is missing | The receiver is unmapped and the method name is ambiguous or on the block list. | Add `--receiver NAME=Type`. Otherwise, read the code. |
| A trace is huge | Low-level functions are expanded. | `--collapse Q` or `--stop REGEX` (for example `--stop '^(Resident\|SourceImage)::'`), or a lower `--depth`. |
| A Python traceback ending in `re.PatternError` (`re.error` before 3.13) | `--stop` is not a valid regular expression. | Quote and fix the pattern. |

## Development

### Module map

| File | Role |
|---|---|
| `inputtrace` | Launcher. Adds its own directory to `sys.path` and calls `cli.main`. |
| `inputtrace_lib/rustsrc.py` | Lexer and item finder. Masks comments and strings, finds `fn`/`impl`/`trait`/`mod`/`struct`/`enum` items with their brace spans and qualifies methods as `Type::name`. |
| `inputtrace_lib/model.py` | `Config`, the `Index` (functions, entries, arms, local signals, memos), entry detection, call resolution, `edges()`, and reactivity (`effect_reads`, `subscribers`, `writers`). |
| `inputtrace_lib/trace.py` | Builds the trace tree (`build`), renders it as text (`render`) or JSON (`to_json`). |
| `inputtrace_lib/cli.py` | `argparse` front end and the five commands. Inventory Markdown and JSON, coverage citation parsing. |
| `tests/test_inputtrace.py` | Unit tests on a tiny Dioxus-shaped crate written to a temporary directory. |

### Design decisions

- **Source text, not a compiler.** The tool must run on any checkout in
  under a second, without building the project. Heuristics are acceptable
  because the output is a map that a human checks by reading code.
- **Vendored lexer.** `rustsrc.py` is copied from
  `../invariants/invkit_lib/rustsrc.py` so each tool stays self-contained.
  The one change: items also record character offsets (`start`, `body`,
  `stop`) so function bodies can be sliced from the masked text. The
  masked text blanks comments and string literals but keeps every
  character's line and column, so regular expressions match only code and
  offsets map back to lines.
- **Drop, do not guess.** An ambiguous call is left out. A missing edge is
  easier to notice and check than a wrong one.
- **`.peek()` is not a subscription.** In Dioxus, `peek` reads a signal
  without subscribing the current scope. An effect that only peeks does not
  rerun when the signal changes. Counting it would invent reactive edges.
- **Effects are entry points.** In Dioxus the important hop (signal write
  → effect) is not a call. Listing effects as entries makes them visible
  in the inventory and in coverage.
- **Expand once.** Each function, arm and effect is expanded once per
  trace. It keeps trees readable and stops cycles.

### Running the tests

From the project root:

```sh
python3 -m unittest discover -s system-analysis-tools/input-tracing/tests
```

The tests build small crates in temporary directories. They need no cargo
and do not read Delos. `RegressionTest` has one test per fixed bug.

### Extending

**A new entry kind.**

1. In `model._entries`, add a loop that finds the construct in the body
   `m[a:b]` of each function and appends an `Entry`. Give it an id
   `file:line:kind[:detail]` and a `span`.
2. Add the kind to `KINDS` in `cli.py`. It then works with `--kind` and
   gets an inventory section.
3. If documents must cite it, add it to the default `kinds` list in
   `cmd_coverage`.
4. If the kind has a body whose edges matter, extend the
   `e.kind in ("handler", "effect")` checks in `_entry_json` and
   `writers`.
5. Add a test to `tests/test_inputtrace.py`.

**A new edge kind.**

1. Add a list to `model.Edges` and fill it in `model.edges()`. Copy the
   closure-inlining merge for it.
2. In `trace.build.kids`, turn each item into a `Node` with a new label.
   Use the `expanded` set to avoid repeats.
3. If it is a property of a body rather than a hop, add it to
   `trace._notes` instead.
4. Expose it in `_entry_json` if the inventory should show it.

**A new receiver mapping.** Use `--receiver NAME=Type` for one run. To
change the default, edit `Config.receivers` in `model.py`. A receiver that
is a newtype handle (`x.0.`) maps to `Config.newtype_inner`.

**A different state struct or message enum.** Change `Config.state_type`,
`Config.state_receivers` and `Config.msg_enum`, or build the index from
Python with your own `Config` (see
[REFERENCE.md](REFERENCE.md#configuration)).

<a id="other-frameworks"></a>
**Other frameworks.** The Dioxus-specific parts are:

| Part | Where |
|---|---|
| Handler detection (`rsx!`, `on…:` attributes) | `_HANDLER` and the handler loop in `model._entries`; `model._element` |
| Effect detection (`use_effect`, `use_memo`) | the effect loop in `model._entries` |
| Local signals (`use_signal`, `use_memo`, `Signal<…>` parameters) | `model._local_signals` |
| Write and read method names | `SIGNAL_WRITE`, `SIGNAL_READ` in `model.py` |
| Spawn patterns | the end of `model.edges()` |

For Leptos, for example, you would detect `view!` and `on:click`, treat
`create_effect`/`Effect::new` as effects, use `.set`/`.update` as writes
and `.get`/`.with`/`.track` as reads, and treat `.get_untracked()` like
`.peek()`.
