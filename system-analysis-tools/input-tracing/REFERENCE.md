# `inputtrace` reference

The exact behaviour of every command, option, output format and exit code.
For concepts, workflows and limitations, see [README.md](README.md).

All examples run from the Delos project root with
`T=system-analysis-tools/input-tracing/inputtrace`.

## Contents

- [Synopsis](#synopsis)
- [Global options](#global-options)
- [`inventory`](#inventory)
- [`trace`](#trace)
- [`signals`](#signals)
- [`channels`](#channels)
- [`coverage`](#coverage)
- [Exit codes](#exit-codes)
- [How `trace` resolves its `entry` argument](#how-trace-resolves-its-entry-argument)
- [File formats](#file-formats)
- [Detection rules](#detection-rules)
- [Configuration](#configuration)
- [Environment variables](#environment-variables)

## Synopsis

```text
inputtrace [-h] [--receiver NAME=Type]... [--src DIR]... COMMAND [command options]

COMMAND is one of:
  inventory [--kind KIND]... [--json FILE] [--md FILE] [root]
  trace     [--depth N] [--no-react] [--stop REGEX] [--collapse QUAL]... [--json] root entry
  signals   [--json] [root]
  channels  [root]
  coverage  [--kind KIND]... [--exclude GLOB]... root docs
```

`KIND` is one of `handler`, `effect`, `env`, `fs`, `dialog`, `webview`.

Global options must come **before** the command. After it, `argparse`
rejects them with `error: unrecognized arguments`.

Every command builds a fresh index from the source files on each run.
Nothing is cached.

## Global options

| Option | Type | Default | Repeatable | Effect |
|---|---|---|---|---|
| `-h`, `--help` | flag | | | Print help and exit 0. Every command also accepts `-h`. |
| `--receiver NAME=Type` | string | none (adds to the built-in map) | yes | Resolve `NAME.method(` to `Type::method`. Adds to, or overrides, `Config.receivers`. The text is split at the first `=`, and surrounding whitespace is stripped. A value without `=`, or with an empty `NAME` or `Type`, is a usage error (`argument --receiver: expected NAME=Type, got '…'`, exit 2). |
| `--src DIR` | path, relative to `root` | `src` | yes | Directories under `root` to read `.rs` files from, recursively. Giving it once **replaces** the default. Hidden directories and directories named `target` are skipped. A directory that does not exist is an error for every command: `inputtrace: error: source directory not found: PATH (check the root and --src)` on stderr, exit 2. A wrong `root` gives the same error, because `root/src` does not exist. |

## `inventory`

List every entry point.

```text
inputtrace inventory [--kind KIND]... [--json FILE] [--md FILE] [root]
```

| Argument / option | Type | Default | Repeatable | Effect |
|---|---|---|---|---|
| `root` | path | `.` | | Project root. |
| `--kind KIND` | choice | all kinds | yes | Keep only these kinds, in both outputs. |
| `--json FILE` | path | none | | Write the [inventory JSON](#inventory-json). |
| `--md FILE` | path | none | | Write the [inventory Markdown](#inventory-markdown). |

Output:

| Options given | stdout | stderr | Files |
|---|---|---|---|
| neither `--json` nor `--md` | the Markdown inventory | | |
| `--json FILE` | | `wrote N entries to FILE` | FILE |
| `--md FILE` | | `wrote FILE` | FILE |
| both | | both lines | both files |

Files are overwritten. The Markdown file has no trailing newline. Stdout
has one.

## `trace`

Print the tree of edges from one entry point or function.

```text
inputtrace trace [--depth N] [--no-react] [--stop REGEX] [--collapse QUAL]... [--json] root entry
```

| Argument / option | Type | Default | Repeatable | Effect |
|---|---|---|---|---|
| `root` | path | required | | Project root. |
| `entry` | string | required | | What to start from. See [How `trace` resolves its `entry` argument](#how-trace-resolves-its-entry-argument). |
| `--depth N` | int | `6` | | Maximum depth. The root is depth 0. A function, effect or `send` node at depth ≥ N is printed with `  …` and not expanded. |
| `--no-react` | flag | off | | Do not follow signal writes into effects. The `writes` notes are still printed. |
| `--stop REGEX` | Python regex | none | | Do not expand functions whose qualified name matches (`re.search`). They are printed with `  [not expanded]`. |
| `--collapse QUAL` | string | none | yes | Do not expand the function whose qualified name is exactly `QUAL`. Printed with `  [not expanded]`. |
| `--json` | flag | off | | Print the tree as [trace JSON](#trace-json) instead of text. |

Output: the tree on stdout. On no match, the message
`` no entry or fn matches 'ENTRY'; try `inventory` `` on stderr and exit 1.
When several entries or functions match, the message
`'ENTRY' is ambiguous; it matches N:`, one line per candidate
(`    ID_OR_QUAL  [file:line]`) and the hint
`pick one by its id, or by FILE:LINE of a line inside it` on stderr, and
exit 1.

Node markers are checked in this order: `(above)` (already expanded in
this tree), then `[not expanded]`, then `…` (depth).

### Text format

```text
ROOT_LABEL   [file:line]
    · NOTE
├─ CHILD_LABEL   [file:line]
│      · NOTE
│  └─ GRANDCHILD_LABEL   [file:line]
└─ CHILD_LABEL   [file:line]
```

| Node | Label | `[file:line]` is |
|---|---|---|
| root entry | `KIND  DESCRIPTION  in FN` | the entry's line |
| root function | `fn  QUAL` | the function header |
| call | `call  QUAL` | the callee's header (its definition, not the call site) |
| send | `send  Msg::VARIANT → received in FN` | the receiving arm (line of the variant name) |
| react | `react SIGNAL → EFFECT_DESCRIPTION` | the effect's line |

Notes (`· …`) on a node:

| Note | When |
|---|---|
| `writes S1, S2` | The body writes these signals directly. Order: writes inlined from called local closures, then state fields (`App.x`) in order of first appearance, then local signals (`Fn.x`) in declaration order. |
| `hands off to a background thread` | The body contains `thread::spawn`, `thread::Builder` or `run_background`. |
| `hands off to an async task` | The body contains `spawn(async` or `spawn_local(`. |
| `hands off to a background thread + an async task` | Both. |

Children are ordered: calls (by first appearance in the body), then sends
(each variant to each of its arms), then react edges (each written signal
to each subscribing effect). An effect's own body never lists a react
edge back to that effect. A function it calls can, and that edge is then
marked `(above)`.

## `signals`

List every signal with the effects that subscribe to it and the code that
writes it.

```text
inputtrace signals [--json] [root]
```

| Argument / option | Type | Default | Effect |
|---|---|---|---|
| `root` | path | `.` | Project root. |
| `--json` | flag | off | Print [signals JSON](#signals-json) instead of text. |

Text output on stdout, one block per signal, sorted by signal id:

```text
SIGNAL
    reruns  EFFECT_DESCRIPTION  [file:line]      (one line per subscribing effect)
    written by N: W1, W2, …                      (always present)
```

`N` counts distinct writers. Writers are sorted. At most 12 are listed;
more are replaced by ` …`. A writer is a function's qualified name, or the
entry id of a handler or effect. A write inside a handler or effect is
reported for that entry only. A function is a writer only through writes
in its body outside every handler and effect span it contains.

## `channels`

List every message variant with its senders and receiving arms.

```text
inputtrace channels [root]
```

| Argument | Type | Default |
|---|---|---|
| `root` | path | `.` |

Text output on stdout, one block per variant, sorted by variant name:

```text
Msg::VARIANT
    sent by     QUAL [file:line]      (one line per function mentioning Msg::VARIANT outside an arm pattern; line of the first such mention)
    received in QUAL [file:line]      (one line per receiving arm)
```

There is no JSON output.

## `coverage`

Report entry points that no Markdown document in a directory cites.

```text
inputtrace coverage [--kind KIND]... [--exclude GLOB]... root docs
```

| Argument / option | Type | Default | Repeatable | Effect |
|---|---|---|---|---|
| `root` | path | required | | Project root. |
| `docs` | path | required | | Directory to scan, recursively, for `*.md` files. A directory that does not exist is treated as empty (so every entry is uncited). |
| `--kind KIND` | choice | `handler`, `effect`, `env`, `dialog`, `webview` | yes | Kinds to check. Giving it replaces the default. `fs` is checked only when requested. |
| `--exclude GLOB` | `fnmatch` glob on the file name | `inventory*.md` | yes | File names to skip. Values are **added** to the default, so `inventory*.md` is always skipped. |

Output on stdout:

```text
COVERED/TOTAL entry points cited by a document in DOCS
  uncited  file:line  KIND  DESCRIPTION  in FN      (one line per uncited entry, in inventory order)
```

If no entry point of the checked kinds exists, `coverage` prints
`inputtrace: no entry points of kind KINDS under ROOT (source dirs: DIRS); check the root and --src`
on stderr, nothing on stdout, and exits 1.

An entry is **cited** when some citation names the entry's file and the
citation's line range overlaps the entry's span (`first ≤ end_line` and
`last ≥ line`). A citation names the file when its path equals the entry's
`file` (the path relative to the root) or ends in `/` followed by it. See
[Citation syntax](#citation-syntax).

## Exit codes

| Command | 0 | 1 | 2 |
|---|---|---|---|
| any | success | an uncaught Python exception (for example an unwritable `--json` path, an invalid `--stop` regex, a non-UTF-8 `.md` file) | usage error from `argparse` (unknown option, missing argument, invalid `--kind`, `--depth` or `--receiver`); a source directory (`root/DIR` for each `--src`) that does not exist |
| `inventory` | success, including when 0 entries are found | | |
| `trace` | exactly one entry or function matches | no entry or function matches, or several do | |
| `signals` | success | | |
| `channels` | success | | |
| `coverage` | at least one entry of the checked kinds exists and every one is cited | at least one entry is uncited, or there are no entries of the checked kinds | |

## How `trace` resolves its `entry` argument

The rules are tried in order. The first rule that gives any candidate
decides: one candidate is the match, several are an
[ambiguity error](#trace) listing them, and later rules are not tried.

1. **Exact entry id.** `src/ui/dialogs.rs:193:onclick`.
2. **`PATH:LINE`** (the whole argument matches `(.+?):(\d+)`). Entries
   whose file is `PATH` or ends with `/PATH`, and whose span
   `line … end_line` contains `LINE`. So `mod.rs:351` works (if only one
   `mod.rs` has an entry there), and so does any line inside a multi-line
   handler. When the candidates are in one file and their spans nest, the
   innermost (smallest span) wins. If no entry contains the line, the
   **innermost function** whose span (header line to closing brace)
   contains it matches instead. This is how to pick one of several
   functions with the same name (`src/ui/session.rs:28` selects the first
   `Session::project`). **This rule is final:** a `PATH:LINE` inside no
   entry and no function matches nothing (`src/ui/dialogs.rs:13` is an
   error, not the entry at line 133).
3. **Exact qualified function name.** `Graph::add_node`, `pump`. Several
   functions with the same qualified name (free functions in different
   files, such as `kinds` in Delos) are ambiguous.
4. **Function by suffix or name.** Functions whose qualified name ends
   with `::ARG` or whose bare name is `ARG`. `add_node` matches
   `Graph::add_node` if no other type has an `add_node`. `new` matches
   every `Type::new`, so it is ambiguous.
5. **Entry id substring.** Entries whose id contains the argument:
   `env:HOME`, `ondragover`. Function names are tried first, so `new` never
   reaches `src/project.rs:307:fs:ZipArchive::new`.

Otherwise: `` no entry or fn matches 'ARG'; try `inventory` `` and exit 1.

From Python, `Index.lookup(key)` returns the candidate list of these rules,
and `Index.entry(key)` returns the entry when there is exactly one
candidate and it is an entry (else `None`). `Index.fn(key)` is separate:
exact qualified name (the first, if several), else the one function by
suffix or name.

## File formats

### Inventory JSON

Written by `inventory --json FILE` with `json.dump(…, indent=1)`. Non-ASCII
characters are escaped (`“`).

```json
{
 "root": ".",
 "entries": [ ENTRY, … ]
}
```

| Key | Type | Meaning |
|---|---|---|
| `root` | string | The root argument, relative to the current directory. |
| `entries` | array | Entries sorted by file, line, id. Filtered by `--kind`. |

ENTRY keys present for **every** kind:

| Key | Type | Meaning |
|---|---|---|
| `id` | string | Entry id: `file:line:suffix`. See [Entry ids](#entry-ids). |
| `kind` | string | `handler`, `effect`, `env`, `fs`, `dialog` or `webview`. |
| `file` | string | Path relative to the root. |
| `line` | int | First line (1-based). |
| `end_line` | int | Last line of the span. |
| `fn` | string | Qualified name of the enclosing function. |
| `detail` | string | Kind-specific detail. See below. |
| `describe` | string | Human-readable description, as in the Markdown *Input* column. |

Keys present only for some kinds:

| Key | Kinds | Type | Meaning |
|---|---|---|---|
| `element` | handler | object | The rsx element. Omitted when no element was found. Keys: `name` (tag, always), and when present `class`, `title`, `type`, `value`, `accept` (first string literal of that attribute) and `label` (first string literal child that is not an attribute value). |
| `via` | handler | string | Name of the local the handler is bound to (`onclick: start`). Omitted otherwise. |
| `calls` | handler, effect | array of string | Qualified names of resolved direct calls, in order of first appearance. Includes calls inlined from local closures. |
| `writes` | handler, effect | array of string | Signal ids written directly, in the order of the `writes` note of [`trace`](#text-format). |
| `sends` | handler, effect | array of string | `Msg` variant names mentioned. |
| `subscribes` | effect | array of string | Signal ids the effect subscribes to, sorted. Includes reads in called functions (up to 6 deep) and in memos it reads. |

`detail` by kind:

| Kind | `detail` | `describe` |
|---|---|---|
| handler | the event name, e.g. `onclick` | `EVENT on <TAG type="…" class="…"> “LABEL”` (label, else title; `type`/`class`/label only when present), plus `` (via `local`) `` when bound to a local |
| effect | `use_effect in FN` or `use_memo in FN` | the detail, plus `` → `name` `` for `let name = use_memo(…)` |
| env | `env NAME` (`env ?` if no string literal found) | the detail |
| fs | `CALL in FN` | the detail |
| dialog | `Async…Dialog in FN` | the detail |
| webview | `document::eval in FN` or `use_asset_handler in FN` | the detail |

Example (Delos):

```json
{
 "id": "src/ui/dialogs.rs:193:onclick", "kind": "handler",
 "file": "src/ui/dialogs.rs", "line": 193, "end_line": 193,
 "fn": "ExportDialog", "detail": "onclick",
 "describe": "onclick on <button class=\"btn primary\"> “Export…” (via `start`)",
 "element": {"name": "button", "class": "btn primary", "label": "Export…"},
 "via": "start",
 "calls": ["codec_by_name", "choose_save_target", "Worker::export"],
 "writes": ["App.export"], "sends": []
}
```

### Entry ids

| Kind | Id |
|---|---|
| handler | `FILE:LINE:EVENT`, e.g. `src/ui/mod.rs:351:onchange` |
| effect | `FILE:LINE:use_effect` or `FILE:LINE:use_memo` |
| env | `FILE:LINE:env:NAME` |
| fs | `FILE:LINE:fs:CALL`, where `CALL` is `fs::read`, `fs::read_to_string`, `fs::read_dir`, `File::open`, `ZipArchive::new` or `read_bytes` |
| dialog | `FILE:LINE:dialog` |
| webview | `FILE:LINE:webview` |

Ids are unique. When two entries would share an id (same file, line and
suffix), only one is kept.

### Inventory Markdown

Written by `inventory` (stdout) or `inventory --md FILE`:

```markdown
# Input inventory

N entry points found by `inputtrace inventory`.

| Kind | Count |
|---|---|
| handler | 105 |          ← one row per kind with at least one entry
…

## handler                  ← one section per kind with entries, in the order
                              handler, effect, env, fs, dialog, webview
| Where | Component / fn | Input | Calls | Writes |
|---|---|---|---|---|
| `file:line` | `fn` | describe | `Q1`, `Q2` | `S1`, `S2` |    ← `—` when empty

## env

| Where | fn | Input |
|---|---|---|
| `file:line` | `fn` | describe |
```

`handler` and `effect` use the five-column table. The other kinds use the
three-column table. `|` in a handler or effect description is escaped as
`\|`. `sends` and `subscribes` appear only in the JSON.

### Trace JSON

Printed by `trace --json`, with `indent=1`. Each node:

| Key | Type | Meaning |
|---|---|---|
| `label` | string | The node label exactly as in the text format, including markers (`  (above)`, `  …`, `  [not expanded]`). |
| `where` | string | `file:line`, as in the brackets of the text format. |
| `notes` | array of string | The notes, without the `· ` prefix. |
| `children` | array of node | Child nodes, in text order. |

```json
{
 "label": "handler  onchange on <input type=\"file\">  in Toolbar",
 "where": "src/ui/mod.rs:351",
 "notes": [],
 "children": [
  {"label": "call  import  …", "where": "src/ui/session.rs:100", "notes": [], "children": []}
 ]
}
```

### Signals JSON

Printed by `signals --json`, with `indent=1`. An object keyed by signal id,
in sorted order:

| Key | Type | Meaning |
|---|---|---|
| `writers` | array of string | The distinct direct writers (function qualified name or entry id), sorted, as in the text output (without the limit of 12). |
| `effects` | array of string | Entry ids of the subscribing effects. |

```json
{
 "App.backend": {
  "writers": ["src/ui/mod.rs:432:onclick"],
  "effects": ["src/ui/image_view.rs:83:use_effect"]
 }
}
```

### Signal ids

| Form | Source |
|---|---|
| `App.FIELD` | `app.FIELD.OP(` anywhere, or `self.FIELD.OP(` inside `impl App`. `Config.state_type` and `Config.state_receivers` set `App` and `app`. A `.clone()` between the field and a write op is allowed: `app.toast.clone().set(…)`. |
| `FN.NAME` | `let [mut] NAME = use_signal…` or `let [mut] NAME = use_memo…` in function `FN`. |
| `OWNER.NAME` | A parameter `NAME: Signal<…>`, aliased to `OWNER.NAME` when exactly one function in the same file declares the local `NAME`. Otherwise `FN.NAME`. |

Write ops: `set`, `write`, `take`, `toggle`, `with_mut`.
Subscribing reads: `read`, `cloned`, `with`, and, for local signals only,
calling the signal: `NAME()`. `peek` is neither.

### Citation syntax

`coverage` scans every `*.md` file under `docs` (recursively) whose file
name does not match an `--exclude` glob. It recognises two forms. Both are
regular expressions in `cli.py`.

**1. Written path** (`_CITE`):

```text
([\w/.-]+\.rs):(\d+)(?:-(\d+))?
```

Matches `src/ui/mod.rs:84`, `src/ui/mod.rs:84-90` and
`crates/app/src/main.rs:10`, also with a prefix
(`../../src/ui/mod.rs:84`) and inside backticks.

**2. Markdown link** (`_LINK`):

```text
\[[^\]]*?:(\d+)(?:-(\d+))?\]\(([\w/.-]+\.rs)(?:#L\d+(?:-L?\d+)?)?\)
```

Matches `[mod.rs:84](../../src/ui/mod.rs#L84)` and
`[mod.rs:84-90](../../src/ui/mod.rs#L84-L90)`. The line range comes from
the **link text**, which must end in `:N` or `:N-M`. The anchor is
optional and ignored.

A citation of a single line `N` is the range `N-N`. The cited path (the
written path, or the link target) names an entry's file when it equals the
entry's `file` or ends in `/` + `file`. So `../../src/ui/mod.rs` and
`crates/app/src/main.rs` work, under any `--src`. A bare file name
(`mod.rs:84`) does not match `src/ui/mod.rs`.

## Detection rules

Detection runs on the *masked* source: comments and string literals are
blanked, so text inside them never matches. All rules apply inside
function bodies only.

| Kind | Pattern | Extra conditions |
|---|---|---|
| handler | `(?<![\w.])(on[a-z]+)\s*:(?!:)` | The function is a component (`#[component]`, or contains `rsx!` and returns `-> Element`) or contains `rsx!`; and an `rsx!` appears before the match in the body. |
| effect | `\buse_(effect\|memo)\s*\(` | |
| env | `\benv::var(?:_os)?\s*\(` | Name: first string literal in the next 80 characters. |
| fs | `\b(fs::read(?:_to_string\|_dir)?\|File::open\|ZipArchive::new\|read_bytes)\s*\(` | |
| dialog | `\brfd::(Async\w*Dialog)\b` | |
| webview | `\b(document::eval\|use_asset_handler)\s*\(` | |

Receiving arms: in each `match EXPR {` (the keyword `match`, then the
first `{` outside parentheses and brackets, so `match msg {` and
`match rx.recv() {` both count), every arm is read. The arm **pattern** is
the text from the arm's start to its `=>`. Each distinct `Msg::VARIANT` in
a pattern is a receiving arm for that variant, at the line of the variant
name, whatever wraps it (`Ok(Msg::X)`, `Msg::A | Msg::B`, guards). Arms
whose pattern has no `Msg::` (`_`, `Err(_)`) are skipped. The arm body is
the `{ … }` block or the expression up to the next top-level comma.

Sends: every `Msg::VARIANT` in a body that is not inside an arm pattern,
including mentions inside arm bodies.

### Call resolution

A call is `name(` or `name::<…>(` where `name` starts with a lowercase
letter or `_` and is not preceded by a word character or `!`. Keywords
(`as async await dyn else fn for if impl in let loop match move mut ref
return where while`) and function definitions are skipped.

| Preceded by | Rule |
|---|---|
| `IDENT.` (optionally `IDENT.clone().`) | If `IDENT` is in `receivers`: `Type::name` or nothing. If `IDENT` is `self`: the enclosing impl's `Type::name` or nothing. Else: the only method (function inside an impl or trait) named `name`, unless `name` is on the block list. |
| `IDENT.0.` | `newtype_inner::name` (`Worker::name`) or nothing. |
| `).` or `.` | The only method named `name`, unless blocked. |
| `Self::` | The enclosing impl's (or trait's) `Type::name`, or nothing. |
| `Upper::` | `Upper::name` or nothing. |
| `lower::` | The only `name` in a file `lower.rs` or `lower/mod.rs`. |
| nothing | A free function `name` in the same file (first), else the only free function `name` in the crate. Else, if `let [mut] name = move …` or `let [mut] name = |…` appears earlier in the enclosing function, that closure's edges are inlined. |

A function never gets a call edge to itself.

### Fallback block list

Methods with these names are never resolved by crate-uniqueness alone
(they still resolve through a mapped receiver or `self`):

```text
abs after and_then apply as_ref as_str b bytes ceil chars choice clamp clone cloned cmp collect color
contains copied data default dims display ends_with enumerate eq err exists exp extend extension f file
file_name filter find finish flatten floor fmt for_each from get h hash i id insert into into_iter
inverse is_dir is_empty is_file is_none is_some iter iter_mut join keys kind len level load lock map
map_err max min name new ok or_else parent parse peek pixels pop push read rect remove render rev round
rsplit_once run send set size spawn split split_once sqrt starts_with store sum take text to_path_buf
to_string toggle trim unwrap unwrap_or unwrap_or_default unwrap_or_else values w with_mut write x y zip
```

## Configuration

`model.Config` (in `inputtrace_lib/model.py`) holds the project-specific
settings:

| Field | Default | Meaning | CLI |
|---|---|---|---|
| `receivers` | `{"app": "App", "g": "Graph", "graph": "Graph", "ctx": "Ctx", "engine": "Engine", "project": "Project", "img": "SourceImage"}` | Receiver identifier → impl type its methods resolve to. | `--receiver NAME=Type` adds or overrides entries |
| `newtype_inner` | `"Worker"` | Type for receivers written `x.0.` (newtype handles). | no |
| `state_type` | `"App"` | Struct whose fields are signals. Prefix of `App.field` ids. | no |
| `state_receivers` | `("app",)` | Identifiers that refer to the state struct. `self` is added inside `impl App`. | no |
| `msg_enum` | `"Msg"` | The channel message enum. | no |
| `src_dirs` | `("src",)` | Source directories, relative to the root. | `--src DIR` replaces it |

To change the fields that have no option, edit the defaults, or build the
index from Python:

```python
import sys
sys.path.insert(0, "system-analysis-tools/input-tracing")
from inputtrace_lib import model, trace

cfg = model.Config(msg_enum="Msg", src_dirs=("src",))
cfg.receivers["doc"] = "Document"
idx = model.build(".", cfg)
root = idx.entry("src/ui/mod.rs:351")      # or idx.fn("Graph::add_node")
print(trace.render(trace.build(idx, root, trace.Options(depth=1))))
```

```text
handler  onchange on <input type="file">  in Toolbar   [src/ui/mod.rs:351]
└─ call  import  …   [src/ui/session.rs:100]
```

`trace.Options` has `depth` (6), `react` (`True`), `stop` (`None`, a regex
string) and `collapse` (`()`, a tuple of qualified names).

These are fixed in the code and have no setting: the hand-off patterns
(`thread::spawn`, `thread::Builder`, `run_background`, `spawn(async`,
`spawn_local(`), the subscription depth through calls (6), the write and
read op names, and the detection patterns above.

## Environment variables

`inputtrace` reads no environment variables of its own. Output does not
depend on Python's `PYTHONHASHSEED`: local signal ids (`Fn.x`) in `writes`
lists are in declaration order.
