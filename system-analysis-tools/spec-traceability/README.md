# spectrace: spec ↔ code traceability matrices

`spectrace` builds, checks and maintains a **traceability matrix**: a set
of Markdown files that link every sentence of a Markdown specification to
the code that implements it and the tests that check it. It is one of the
research tools in [`system-analysis-tools/`](../). Each tool there builds a
*system model* of a codebase that helps a human make decisions. This one
supports the model in
[`system-models/spec-traceability/`](../../system-models/spec-traceability/),
a matrix between Delos's `SPEC.md` and its Rust code.

- [REFERENCE.md](REFERENCE.md) is the exact reference: every option, exit code and file format.
- [AGENTS.md](AGENTS.md) is the step-by-step procedure for an AI agent.

## Contents

1. [Purpose](#purpose)
2. [Concepts and terminology](#concepts-and-terminology)
3. [Requirements and installation](#requirements-and-installation)
4. [Quick start](#quick-start)
5. [Typical workflows](#typical-workflows)
6. [Interpreting results](#interpreting-results)
7. [Results on Delos](#results-on-delos)
8. [Limitations and accuracy](#limitations-and-accuracy)
9. [Troubleshooting](#troubleshooting)
10. [Development](#development)

## Purpose

### The question it answers

*For each promise the spec makes, where does the code keep it, how well,
and what checks it?* And the reverse: *what does the code do that no
promise asks for?*

A human uses the answers to make decisions:

- Which promises does this change touch? (`impact`, `why`)
- Which promises are only partly kept, and what decision does each gap need? (the statuses, `findings.md`)
- What behaviour does the code have that nobody specified? (`ancestry`, `uncited`)
- Has the code or the spec drifted since the matrix was written? (`check --pins`)
- Does a known defect still reproduce? (`probe`)

### What the tool does and does not do

The matrix itself is written by a person or an agent who reads the spec
and the code. `spectrace` does not decide whether code implements a
sentence. It does the mechanical parts:

| Part of the work | Done by |
|---|---|
| List the sentences the matrix must account for | `units` |
| Suggest where to look for a sentence | `suggest` (a reading list, not an answer) |
| Prove every sentence is quoted verbatim by some entry | `check` (coverage) |
| Prove citations point at real lines that mention the named identifier | `check` (anchoring) |
| Prove cited tests exist, links resolve, the summary is current | `check` |
| Decide whether a cited range *does* what the sentence says, and the status | **A human or agent, then an independent review** |
| Find code, state and branches no clause leads to | `ancestry`, `uncited`, `index` |
| Confirm defects | `probe` (runs failing tests kept with the matrix) |
| Track changes since the analysis | `pin`, `check --pins`, `impact`, `why` |
| Produce views for humans | `summary`, `grid`, `index`, `html`, `export` |

### When to use it

- The project has a Markdown spec written as prose sentences, and someone needs to know how much of it the code honours.
- You are reviewing a change and want to know which spec promises it touches.
- You want a list of behaviour the spec is silent about (undo depth, memory policies, keyboard shortcuts).

### When not to use it

- There is no prose spec, or the spec is a list of tickets. There is nothing to quote.
- The code is not Rust and you need the code-side features. Coverage, links, tests-exist and pins work for any language. Anchoring, item spans, `suggest`, `index`, `uncited`, `ancestry` and `probe` assume Rust and Cargo.
- You need proof of correctness. A clean `check` proves consistency, not truth (see [Limitations](#limitations-and-accuracy)).
- You only need a call graph or a module map. Other tools in `system-analysis-tools/` do that better.

## Concepts and terminology

| Term | Meaning |
|---|---|
| **Spec** | The Markdown specification, `SPEC.md` by default (`--spec`). |
| **Unit** | One sentence of the spec, or one whole fenced code block. `units` lists them. Each unit has a line, an item number and a section. |
| **Item number** | The spec's own list numbering, such as `3.1.1` for `- 3.1.1. …`. Shown in `units` output; `-` when there is none. |
| **Section** | The headings above a unit, without the top-level title, joined by ` / ` (for example `Sessions: quick mode and projects / Project structure`). |
| **Matrix** | A directory of Markdown files (`*.md` at its top level) holding entries. |
| **Entry** | A `## ID · Title {#anchor}` heading and the text under it. |
| **Clause entry** | An entry with at least one quote. It accounts for spec sentences and must have a status. |
| **Note entry** | An entry with no quotes: a finding (`D-…`), an unspecified behaviour (`U1`), a review item. It is checked for citations, tests and links, but has no status. |
| **Quote** | A `>` line in a clause entry, before its first label, holding spec text verbatim. `…` elides text. |
| **Covered** | A unit is covered when quotes cover at least half of its characters. |
| **Status** | The first word of an entry's `**Status.**` section: `Implemented`, `Partial`, `Diverges`, `Missing`, `Deferred` or `Process`. The matrix README defines what each means. |
| **Area** | The `# …` heading of the matrix file an entry is in, such as `2 · Editing, views and export`. `summary` counts statuses per area. |
| **Section label** | A bold label at the start of a line, such as `**Implemented by.**`, `**Tests.**`, `**Notes.**`. Text up to the next label belongs to that section. |
| **Citation** | A Markdown link to code of the form `[graph.rs:243-264](../../src/graph.rs#L243)`. Its *key* is `path:start-end`, for example `src/graph.rs:243-264`. |
| **Anchoring** | The check that a backticked identifier written next to a citation occurs within 3 lines of the cited range, or names the enclosing item. An *unanchored* citation is a note to read. |
| **Test reference** | A backticked `tests/file.rs::function` or `src/file.rs::function`. `check` verifies that the function exists. |
| **Loose citation** | A citation outside any entry (for example in a README paragraph). It is checked and pinned like the others. |
| **Component** (or group) | A bucket of source paths used by `grid`: `graph`, `nodes`, `engine`, `formats`, `worker`, `expr`, `project`, `ui/state`, `ui/session`, `ui/views`, `build`, `test code`, `other`. The list is hard-coded for Delos. `--by file` uses file paths instead. |
| **Analysed commit** | The commit the matrix describes, written in the matrix README as ``commit `197e30c` ``. `impact`, `pin`, `html` and `export` read it. |
| **Summary block** | The text between `<!-- spectrace:summary -->` and `<!-- /spectrace:summary -->` in the matrix README. `summary --write` fills it; `check` fails when it is stale. |
| **Pins** | A JSON file recording the text behind every citation and the digest of every spec unit at the analysed commit. |
| **Drift** | What `check --pins` reports: citations that **moved** (same text, new lines), **changed** (text differs) or are **missing** (file gone), and spec units that **changed**, were **removed** or were **added**. |
| **Impact** | The entries whose cited lines or quoted sentences a `git diff` touches. |
| **Probe** | A Rust integration test in `<matrix>/probes/` that asserts what the spec says about a suspected defect. A **failing** probe **confirms** the finding. |
| **Ancestry** | Whether a function descends from a spec clause. **Seeds** are the lines clause entries cite under **Implemented by**. A function is **specified** (≥ 30% of its lines are seeds), **partly specified** (some), **support** (reached from seeds through references) or **unspecified** (no clause leads to it). |
| **Trigger** | An uncited line of a cited function that calls into unspecified code: how a user reaches that code. |
| **Unspecified state** | A struct field that code writes but no seed line uses. |
| **Uncited span** | A run of 3 or more uncited code lines inside a specified or partly specified function: a branch the clause skips over. |

## Requirements and installation

| Requirement | Needed for | Notes |
|---|---|---|
| Python ≥ 3.10 | everything | Standard library only. No packages to install. |
| [`uv`](https://docs.astral.sh/uv/) | running `./spectrace` directly | The launcher is a uv script (`#!/usr/bin/env -S uv run --quiet --script`). Without uv, run `python3 spectrace …`. |
| `git` | `impact` | Reads `git diff` and `git show` in `--root`. |
| `cargo` and a buildable crate | `probe` | Runs `cargo test` in `--root`. |
| Rust sources under `src/` and `tests/` | code-side features | The tool reads `*.rs` files under `<root>/src` and `<root>/tests`. |

There is nothing to install. Run the launcher from its directory or by path:

```sh
system-analysis-tools/spec-traceability/spectrace --help
# or, without uv:
python3 system-analysis-tools/spec-traceability/spectrace --help
```

The tool reads no environment variables and no configuration files.

## Quick start

All commands below run from the Delos project root
(`/Users/castilho/Code/Personal/delos`). The output shown is real, taken at
commit `197e30c`, trimmed where marked.

```sh
T=system-analysis-tools/spec-traceability/spectrace
M=system-models/spec-traceability
```

**Global options come before the subcommand.** `--root` (default `.`)
is the project root: the spec, `src/`, `tests/` and git are read from it.
`--spec` (default `SPEC.md`) is the spec path relative to the root. The
matrix directory argument is resolved from the **current directory**, not
from `--root`.

```sh
$T --root . --spec SPEC.md check $M      # same as: $T check $M
```

### 1. See what the spec says

```console
$ $T units | head -4
   3  -       Delos is to be an image editing tool focused on combining existing content with generative content.
   3  -       By generative content, we mean both AI-generated content and traditionally generative content, using classical procedural generation algorithms.
  10  -       Delos must be built using the Rust programming language.
  12  -       Delos must be possible to run as a desktop application, a web-based application and a mobile application.
$ $T units | wc -l
     151
```

Columns: spec line, item number (`-` if none), sentence.

### 2. Check the matrix

```console
$ $T check $M

90 clause entries (122 entries) · 151 spec units, 2 not quoted · 594 citations, 0 of 588 checkable unanchored · 143 test refs
0 errors, 0 warnings, 2 notes (-v to list notes)
$ $T check $M -v
note: SPEC.md:98: no entry quotes “<name>/ project.delos the node graph, as a Delos expression (see below) inputs/ ”
note: SPEC.md:129: no entry quotes “; Nodes used more than once are bound with def; everything else nests. (def phot”
...
```

The two notes are spec code blocks, which are illustrations. Exit code 0
means no errors.

### 3. Ask which promises depend on a line

```console
$ $T why $M src/ui/state.rs:470
src/ui/state.rs:470 in App::add_node (452-488)
  cited directly by D-ADD-AFTER-SPLIT · Add node after a split rewires the second output []
  cited directly by S5 · Specific outputs as inputs, relocation and export targets [Partial]
  cited directly by U3 · Adding and deleting nodes rewire the graph []
  same item cited by F1.2 · Operator parameters: WYSIWYG or form [Implemented]
  same item cited by N3 · Default canvas fits the window [Implemented]
```

Note entries (findings `D-…`, unspecified `U…`) show an empty status `[]`.
The exit code is 1 when no entry cites the line or its item, with or
without `--json`.

### 4. See status counts

```console
$ $T summary $M
| Area | Clauses | Implemented | Partial | Diverges | Missing | Deferred | Process |
|---|---|---|---|---|---|---|---|
| 1 · Product and platform requirements | 5 | 2 | 3 |  |  |  |  |
| 2 · Editing, views and export | 17 | 12 | 5 |  |  |  |  |
...
| **Total** | **90** | **69** | **16** | **0** | **0** | **2** | **3** |

Spec coverage: 149 of 149 spec sentences are quoted by a clause entry. 594 code citations into 33 files; 73 distinct tests cited.
```

### 5. Generate the views (into a scratch directory)

Commands with `-o` write a file and print `wrote <path>` on stderr.
Without `-o` they print to stdout.

```sh
OUT=/tmp/spectrace-out            # any directory; it is created if missing
$T grid $M -o $OUT/grid.md
$T grid $M --by file --format csv -o $OUT/grid-by-file.csv
$T index $M -o $OUT/code-to-spec.md
$T uncited $M -o $OUT/uncited.md
$T ancestry $M -o $OUT/ancestry.md
$T export $M -o $OUT/matrix.json
$T pin $M -o $OUT/pins.json
$T html $M -o $M/index.html       # see "html" below for why it goes in the matrix directory
```

The model keeps its generated views in `$M/generated/`, its pins in
`$M/pins.json` and its viewer in `$M/index.html`. Overwrite those only when
you mean to update the model.

## Typical workflows

### Build a matrix from a spec

The full procedure is in [AGENTS.md](AGENTS.md#procedure-a-build-a-matrix).
In short:

1. Create the matrix directory with a `README.md` that contains ``commit `<sha>` `` (any case: `Commit` works too) and an empty summary block:

   ```markdown
   The analysis is of commit `197e30c`.

   <!-- spectrace:summary -->
   <!-- /spectrace:summary -->
   ```

2. `$T units` lists the sentences. Read the whole spec and the code.
3. For each sentence, `$T suggest "<sentence>"` gives a reading list. Read the code it points at and around it.
4. Write entries in `NN-<area>.md` files (format in [REFERENCE.md](REFERENCE.md#matrix-format)).
5. `$T units $M --uncovered` lists what is left to quote.
6. `$T check $M` until it shows 0 errors; then `$T check $M -v` and read every note.
7. Write `findings.md` for every `Partial`, `Diverges` and `Missing` status. Write probes for defects; run `$T probe $M`.
8. `$T ancestry $M` for the reverse direction; record user-visible items in `unspecified.md`.
9. Have someone who did not write the entries review them against the code.
10. `$T summary $M --write`, generate the views, `$T pin $M -o $M/pins.json`.

### After the code changes

```sh
$T impact $M                         # entries whose cited lines the working-tree diff touches
$T check $M --pins $M/pins.json      # moved / changed citations, changed spec sentences
```

For every entry listed, re-read the code and decide whether the quoted
sentences still hold. Fix moved citations by changing the line numbers in
the link text and in `#L…`. When the matrix is current again, update the
README's ``commit `…` ``, run `summary --write`, and re-pin.

Example, after inserting one line at the top of `src/graph.rs`, appending a
comment to `src/project.rs:395` and to the `edition` line of `Cargo.toml`,
and editing two spec sentences (run on a scratch clone of the repository
with a copy of the matrix):

```console
$ $T check $M --pins $M/pins.json
error: 01-product-and-platform.md:25: R1 quotes text that is not in the spec: “Delos must be built using the Rust programming language.”
error: SPEC.md:10: no entry quotes “Delos must be built using the Rust programming language, edition 2024.”
error: SPEC.md:16: no entry quotes “Delos also prints a banner.”
error: README.md: the summary block is stale (run `spectrace summary --write`)
error: changed: Cargo.toml:1-4 (R1): re-read these entries
error: changed: src/project.rs:390-399 (P6.1): re-read these entries
error: spec changed: SPEC line 10 “Delos must be built using the Rust programming language.” (R1)
warning: moved: src/graph.rs:243-264 -> 244-265 (F1, F2)
warning: moved: src/graph.rs:66-72 -> 67-73 (F2)
...
warning: spec added: line 10 “Delos must be built using the Rust programming language, edi”
warning: spec added: line 16 “Delos also prints a banner.”

90 clause entries (122 entries) · 152 spec units, 4 not quoted · 594 citations, 0 of 588 checkable unanchored · 143 test refs
7 errors, 21 warnings, 2 notes (-v to list notes)
$ $T impact $M --rev HEAD
changes since HEAD:

P6.1 · Atomic saves [Implemented]
  code  src/project.rs:390-399 ∩ 395-395

R1 · Written in Rust [Implemented]
  spec  SPEC.md:10 “Delos must be built using the Rust programming language.”
  code  Cargo.toml:4-4 ∩ 4-4

R3 · GPU acceleration wherever possible [Partial]
  spec  SPEC.md:16 “Delos will extensively display and manipulate images.”
  spec  SPEC.md:16 “Using GPU acceleration wherever possible is a requirement fo”

R4 · Graceful CPU fallback [Implemented]
  spec  SPEC.md:16 “However, in case GPU acceleration is not available, Delos sh”
```

`impact` reports every sentence of an edited paragraph, because a unit
spans its whole paragraph's lines. The line inserted at the top of
`src/graph.rs` touches no entry, because no citation covers line 1 there;
`check --pins` reports the shifted citations as moved.

### After the spec changes

1. `$T impact $M` lists the entries that quote changed sentences.
2. `$T check $M` reports quotes no longer found in the spec (errors) and new sentences no entry quotes (errors).
3. Update the quotes, re-judge the statuses, add entries for new sentences (or mark them `Missing`).

### Before editing code

`$T why $M FILE:LINE` lists the entries that cite the line (directly) or
the function around it (same item). Those are the promises to keep.

### Reviewing a pull request

1. Check out the PR branch.
2. `$T impact $M` maps the PR's diff (working tree against the analysed commit) onto entries. If the PR is based on a later commit than the analysed one, the report also includes changes already on the base branch.
3. For each entry, read the quoted sentences next to the diff. Flag any `Implemented` clause the change weakens.
4. `changed files no entry cites:` lists changed files with no citation at all. Look there for new, unspecified behaviour.
5. `$T check $M --pins $M/pins.json` shows which citations must be re-read or renumbered.

### In CI

```sh
T=system-analysis-tools/spec-traceability/spectrace
M=system-models/spec-traceability
python3 $T check $M                 # exit 1 on any error: stale quotes, uncovered sentences, bad citations, stale summary
python3 $T check $M --strict        # also fails on warnings (missing **Tests.** sections, missing summary block)
python3 $T check $M --pins $M/pins.json   # fails when cited code changed, or a quoted spec sentence changed
```

The plain `check` is the right gate for a matrix that is maintained with
the code. `--pins` fails on every code change under a citation, so use it
as a reminder job, not a merge gate. `probe` needs a full Cargo build; run
it in a job that already builds the crate. None of this has been wired
into a CI system yet; the commands are the ones verified locally.

## Interpreting results

### `check`

The last two lines are statistics:

```
90 clause entries (122 entries) · 151 spec units, 2 not quoted · 594 citations, 0 of 588 checkable unanchored · 143 test refs
0 errors, 0 warnings, 2 notes (-v to list notes)
```

- *clause entries (entries)*: entries with quotes, and all entries.
- *spec units, not quoted*: units below the 50% coverage threshold, code blocks included.
- *checkable*: citations into `.rs` files with at least one backticked identifier nearby.
- *test refs*: test references, counting repeats.

**Errors** mean the matrix is wrong or stale. **Warnings** need a human
decision. **Notes** (shown with `-v`) are things to read: unanchored
citations, citations with no identifier, unquoted code blocks. The full
list of messages and their fixes is in
[REFERENCE.md](REFERENCE.md#check-messages).

An unanchored citation means the identifier in backticks does not appear
at the cited lines. Either the range is wrong or the wrong identifier is
named. An anchored citation is not necessarily correct: the identifier is
there, but the claim may not be.

### `units`

`line  item  text`, one unit per line. With `--uncovered`, only units below
the coverage threshold. Code blocks appear with their whitespace collapsed.

### `summary`

One row per area with status counts, a total row, and one sentence:
covered sentences (code blocks excluded), citation count, distinct cited
files and distinct cited tests. A clause whose status is not recognised
counts in the *Clauses* column but in no status column; `check` reports
it as an error anyway.

### `grid`

```
| Clause | Status | graph | nodes | engine | ... | build | test code | tests |
| D1 Generative image editing | Partial |  | ● | • | ... |  |  | 2 |
```

One row per clause entry, one column per component. `•` means fewer than 20
cited lines in that component, `●` 20 or more. `tests` is the number of
test references. CSV and JSON give exact line counts. A column with many
`●` is a component the spec leans on; a clause spread over many columns is
a cross-cutting promise.

### `index`

```
## `src/engine/cache.rs`

- `Entry` (10-10): D1, U7
- `Cache` (12-15): D1, N6, U7
```

Every cited item in `src/`, with every entry that cites it (clause and
note entries). `--json` also lists uncited items, with `"entries": []`.

### `uncited`

```
| File | Functions | Cited | Cited share of fn lines |
|---|---|---|---|
| `src/engine/geom.rs` | 27 | 0 | 0% |
...
- `src/engine/geom.rs`: `Size::new (16-18)`, `Size::rect (20-22)`, ...
```

Files sorted by the share of function lines any citation covers, then the
uncited functions per file. Citations from note entries count. A large
function cited for one line counts as cited; read the share, and prefer
`ancestry` for a precise answer.

### `why`

`cited directly by` lists entries whose citation includes the line.
`same item cited by` lists entries that cite other lines of the enclosing
item. `no entry cites this code` (exit 1) means no promise depends on it.

### `suggest`

```
 16.71  src/project.rs:392-399  fn write_atomic  [atomic, file, nev]
 15.07  src/worker.rs:84-100  struct Worker  [sav, atomic, project, written]
```

Score, location, kind and name, and the matched word stems. It is a
reading list. Item names score highest, then doc comments, string
literals and comments, then identifiers. It is weakest for UI behaviour,
where it tends to rank whole components above the handler inside them.

### `pin`

A JSON file; see [REFERENCE.md](REFERENCE.md#pins-file). It has no
human-readable output.

### `impact`

```
P6.1 · Atomic saves [Implemented]
  code  src/project.rs:390-399 ∩ 395-395
```

For each touched entry: `code  <citation> ∩ <changed old-side lines>` and
`spec  <spec path>:<line> “<sentence>”` (the path is `--spec`, relative to
the root; `SPEC.md` by default). Code citations into any file count,
including non-Rust files such as `Cargo.toml`. `(loose)` collects citations outside
entries; `(unclaimed)` collects changed sentences no entry quotes.
`changed files no entry cites:` lists files under `src/` or `tests/` that
changed but have no citation. `no entry is affected` means the diff
touches no cited line and no quoted sentence.

### `ancestry`

```
| Class | Items | Lines |
|---|---|---|
| specified | 218 | 4395 |
| partly specified | 21 | 947 |
| support | 275 | 1951 |
| unspecified | 162 | 713 |
```

Then three reports:

1. **Unspecified state.** Fields that code writes but no seed line uses. *Paths through writers* is the number of clauses whose reference paths reach a function that writes the field. A high number means hidden state woven into specified behaviour. Read this first.
   ```
   | `History.undo` `src/ui/state.rs:102` (declared on a cited line) | 44 | `App::push_history` (support), `App::undo` (unspecified), `App::redo` (unspecified) | U1 |
   ```
2. **Uncited branches of cited functions.** Runs of 3+ uncited code lines inside cited functions. `✎` marks spans that change state. Spans that change state or call uncited code come first.
3. **Unspecified code.** Groups of functions no clause leads to, with `triggered from` lines (how a user reaches them) and whether any note entry mentions them (`noted in …` or **not noted anywhere**).

*Noted in* lists note entries and clause Notes that cite those lines: the
analyst already knows about it. Unnoted items with triggers are the new
candidates for `unspecified.md` or for the spec.

### `probe`

```
confirms             failed_import_leaves_nothing_behind  [D-IMPORT-LEFTOVER]
    thread 'failed_import_leaves_nothing_behind' (4703747) panicked at tests/zz_spectrace_spec_probes.rs:46:5:
    a failed import left ["future"] in the projects folder
```

| Verdict | Meaning | Exit |
|---|---|---|
| `confirms` | The probe failed: the defect still reproduces. | 0, if a finding claims it |
| `no-longer-violated` | The probe passed: the defect looks fixed. Update the finding and status. | 1 |
| `ignored` | The test is `#[ignore]`d. | 1 |
| `did-not-run` | The test function was not in cargo's output. | 1 |
| `did not build or run` | Cargo failed before running tests; the tail of its output follows. | 1 |

The bracket shows the entry that claims the probe, or `no finding cites it`.

### `html`

A single self-contained page with four tabs:

- **Spec**: every unit, coloured by the worst status of the entries quoting it (Diverges, Missing, Partial, Deferred, Process, Implemented, in that order). Click a unit to see the first entry quoting it.
- **Clauses**: all entries (clause and note) by area, with a filter box.
- **Matrix**: clause × file table; shading grows with the number of cited lines.
- **Code**: every item in `src/` with the entries citing it and its ancestry tag (`spec`, `part`, `support`, `none`).

Code links in the page are relative to where the page is saved, exactly as
they are written in the matrix (for example `../../src/graph.rs#L243`).
Write the page into the matrix directory (`-o $M/index.html`) for them to
resolve.

### `export`

The whole matrix as JSON (entries with sections, citations, tests and
links; every unit with its coverage). Use it to feed other tools. Schema in
[REFERENCE.md](REFERENCE.md#export-json).

## Results on Delos

These results come from the study that produced the model, at commit
`197e30c`. The `check`, `suggest`, `uncited`, `ancestry` and `probe`
figures below were re-run while writing this document.

- **`check`**: 0 errors, 0 warnings. 149 of 149 sentences are quoted. The two code blocks (project layout, expression example) are notes. 594 citations into 33 files (after the citation-tightening experiment, [tightening.md](../../system-models/spec-traceability/tightening.md)); 0 of the 588 checkable citations are unanchored; 143 test references, all of which exist.
- **Anchoring is a consistency check, not a truth check.** After `check` was clean, an independent reviewer still found 33 problems in 90 entries ([review.md](../../system-models/spec-traceability/review.md)): 13 tests cited for more than they test, 5 line ranges that cover the wrong lines, 10 factual slips and 5 status disagreements. Every citation involved was anchored.
- **`suggest`** ranks `write_atomic` first for "Saves are atomic, so a project file is never left half-written." For the double-click sentence (F3.1.1) it ranks the whole `GraphView` component and a help text (`DocumentPanel`) first, above the handler. Use it to find files, then read.
- **`uncited`**: `geom.rs`, `pool.rs` and `profile.rs` have no cited function; `resident.rs` and `tile.rs` are under 10%. This is infrastructure under the clauses. A spec describes behaviour, not machinery.
- **`ancestry`**: 20 unspecified state fields. The top one is `App.selected_edge` (45 clauses' paths through its writers), then the undo history (`History.last_time`, `History.redo`, `History.undo`, 44 each), `ParamDef.visible_if` (42) and `Graph.next_id` (36). The undo history and `Graph.next_id` were also at the top before the citations were tightened; `Graph.next_id` is behind the confirmed stale-image defect (D-STALE). 36 of 141 unspecified groups have triggers; 27 of those are noted nowhere in the matrix. Examples: the Inspector shell, Delete-key deletion (`App::delete_selection`), the export button handler (`App::export_pressed`).
- **Calibration**: measure it by *enrichment*, not overlap. `spectrace ancestry system-models/spec-traceability --recall U` ([recall mode](REFERENCE.md#recall-mode)) scores the 16 hand-written items in `unspecified.md`:

  ```
  overlap: 16 of 16 (lax; compare with random placement)
  enrichment: 51% of cited code lines flagged vs 32% of the same files = x1.57
  ```

  *Overlap* (an item counts when any of its cited lines touches flagged code) is near chance: on the matrix as first written, citations moved to random places in the same files scored 12.8 of 16 against a real 10 of 16, because flagged code is dense in UI files. *Enrichment* (the flagged share of the items' cited code lines over the flagged share of all code lines in the same files; 1.0 is chance) was ×1.36 before the citations were tightened and ×1.57 after. The lines of the D-ADD-AFTER-SPLIT defect went from 0 of 6 flagged to 6 of 6: before, F1.2 cited all of `App::add_node` for "selects the new node", which hid the rewiring rule. Cite the narrowest range. Details: [tightening.md](../../system-models/spec-traceability/tightening.md).
- **`probe`**: both probes fail with the messages `findings.md` describes, confirming D-IMPORT-LEFTOVER (P15) and D-OPEN-WRITES (P6.2). A run takes about 20 seconds with a warm Cargo cache. `tests/` is unchanged afterwards.

## Limitations and accuracy

**What `check` proves.** Every spec sentence is quoted verbatim; every
quote exists in the spec; every citation is inside its file, its link and
text agree, and (for Rust) a named identifier is within 3 lines; every
cited test function exists; every in-matrix link resolves; the summary
is current. It does **not** prove that the cited code does what the
sentence says, that a test asserts what it is cited for, or that the
status is right.

**Spec splitting.** Sentences end at `.`, `!` or `?` followed by a space
and a capital letter, `(`, `` ` ``, `*` or a quote mark. Splits never
happen inside a code span or after `e.g.`, `i.e.`, `etc.`, `vs.`, `cf.`.
Other abbreviations (`Dr. Smith`) split. Tables and HTML are treated as
text. Each paragraph or list item is one block; its units all share the
block's first and last line.

**The Rust reader.** It masks comments and strings and matches braces. It
has no type information and does not expand macros. Item spans are right
for ordinary code. `cfg`-gated duplicates appear twice.

**Anchoring.** Identifiers are split into tokens (`App::add_node` →
`App`, `add_node`); one-character tokens are ignored. Any token matching
as a whole word counts, so a common word such as `self` or `new` anchors
easily.

**`suggest`.** Lexical and Rust-only. It finds identifiers and doc words,
not behaviour.

**`uncited`.** An item counts as cited if any citation overlaps it, so a
large item cited for one line counts fully. Read the share.

**`ancestry`.** The reference graph is lexical. Very common method names
(`new`, `get`, `write`, `read`, …) are ignored, so a function such as
`Doc::write` looks unspecified even though `to_text` calls it. A bare
name resolves to every function with that name, so trait methods
over-connect. *Partly specified* versus *specified* is a 30% threshold;
spans are reported for both, so nothing depends on it. UI components are
mostly markup, so their uncited spans are long; read spans marked `✎` or
with calls first.

**Pins.** Spec units are matched by digest of their text. A sentence that
moves to a new line is not reported. Two identical sentences count once.
A moved citation is found only if the exact same lines (trailing spaces
ignored) appear elsewhere in the same file; the first match from the top
wins.

**`impact`.**

- It diffs `src/`, `tests/`, the spec and every other file the matrix cites (on Delos, `Cargo.toml`). A change to a file outside `src/` and `tests/` that no entry cites is not listed under `changed files no entry cites`.
- Citations describe the README commit. With `--rev` at another commit, each citation is first carried through the diff between the two commits, so its line numbers match the diff it is compared with. A citation of code that changed in between widens to the changed lines, which can add hits on lines the matrix never cited. Renames are not followed: a citation of a file that is missing at `--rev` is listed as unmapped, not compared. Keep the README's commit current: a newer matrix with an old commit line would be carried from the wrong place.
- A pure insertion is mapped to the line before it, or to line 1 when it is at the top of the file.
- Untracked files are not in `git diff`, so a new, uncommitted file is invisible.

**`grid` components** are hard-coded for Delos's layout
(`report.py`, `DEFAULT_GROUPS`). On another project use `--by file`.

**`probe`** copies each probe file to `tests/zz_spectrace_<name>.rs` for
the run and deletes it afterwards, even if cargo fails. If the process is
killed, the copy stays; the next run refuses to start until it is deleted.

## Troubleshooting

Real messages, with fixes. For every `check` message, see
[REFERENCE.md](REFERENCE.md#check-messages).

| Message | Cause | Fix |
|---|---|---|
| `system-models/spectrace: No such file or directory` | The matrix path is wrong. It is resolved from the current directory, not from `--root`. | Run from the project root, or pass a path valid from where you are. |
| `/…/delos/NOPE.md: No such file or directory` | `--spec` does not exist under `--root`. | Pass `--spec` relative to the root, before the subcommand. |
| `spectrace: error: argument cmd: invalid choice: …` (exit 2) | Unknown subcommand, or a global option placed after it. | `$T --root R --spec S <command> …`. |
| `--uncovered needs a matrix directory` | `units --uncovered` without a matrix. | `$T units $M --uncovered`. |
| `README.md has no <!-- spectrace:summary --> ... <!-- /spectrace:summary --> block` | `summary --write` found no block. | Add both markers to the matrix README, each on its own line. |
| `<matrix>/README.md: No such file or directory` | `summary --write` and the matrix has no README. | Create it with a summary block. |
| `error: README.md: the summary block is stale (run `spectrace summary --write`)` | Statuses or coverage changed since the block was written. | `$T summary $M --write`. |
| `no --rev and no commit `…` in the matrix README` | `impact` needs a base revision. | Write ``commit `<sha>` `` (any case) in the matrix README, or pass `--rev`. |
| `git diff failed: fatal: bad revision 'nosuch'` | The revision is not in this repository (shallow clone, wrong `--root`). | Fetch the commit, or pass `--root` / `--rev` correctly. |
| `why needs FILE:LINE, e.g. src/graph.rs:243 (got 'src/graph.rs')` | `why` without `:LINE`. | `$T why $M src/graph.rs:243`. |
| `src/x.rs:3 in None (None)` / `no entry cites this code` | Nothing cites the line, or the path is not relative to the root (absolute paths never match). | Use a root-relative path such as `src/graph.rs:243`. |
| `unknown entry F3.1.1` | `suggest --clause` without `--matrix`, or an unknown ID. | `$T suggest --matrix $M --clause F3.1.1`. |
| `tests/ already holds ['zz_spectrace_x.rs']: a previous run was killed. Delete them and retry.` | A killed `probe` left its copy behind. | Delete `tests/zz_spectrace_*.rs` and retry. |
| `<stem>: did not build or run` followed by cargo output | The probe does not compile, or cargo failed. | Read the cargo output; probes may only use the crate's public API. |
| `probe <file>.rs: cargo test timed out after 1800 s (raise --timeout)` | One probe file's `cargo test` ran longer than `--timeout`. Its copy in `tests/` is deleted; later probes are not run. | Pass a larger `--timeout`, or warm the Cargo cache first. |
| An entry is missing from the counts | Its heading does not match `## ID · Title {#anchor}`: wrong separator (`-` instead of `·`), lowercase ID, or no anchor. Such a heading is silently treated as a plain heading and ends the previous entry. | Fix the heading. See [REFERENCE.md](REFERENCE.md#entry-headings). |
| A `>` line is not counted as a quote | It comes after the entry's first `**Label.**`. | Put all quotes directly under the heading. |
| HTML viewer code links are broken | The page was written outside the matrix directory. | `$T html $M -o $M/index.html`. |

## Development

### Module map

```
spectrace                launcher (uv script; adds its directory to sys.path, calls cli.main)
spectrace_lib/
  cli.py                 argparse, one cmd_* function per subcommand; -o handling
  spec.py                spec → units (sentence splitting, numbering, sections); quote matching with …
  matrix.py              matrix Markdown → entries, quotes, sections, citations, tests, links; statuses
  rust.py                Rust reader: comment/string masking, items and spans by brace matching; source tree
  check.py               coverage, entry, citation, anchoring, test, link and summary checks
  pins.py                pin / verify (drift); analysed commit; git diff → impact
  report.py              summary, grid, code index, uncited, why, HTML viewer (template inline)
  ancestry.py            spec ancestry: classes, reachability, unspecified state, spans, clusters
  suggest.py             lexical candidate ranking (tf-idf-like)
  probe.py               copy probes into tests/, run cargo, parse results
tests/test_spectrace.py  unit tests on a tiny generated spec, crate and matrix
```

### Design decisions

- **Markdown is the source of truth.** The matrix is hand-written Markdown that reads well on GitHub. The tool parses conventions (headings, labels, link forms) instead of a schema, so entries stay readable prose.
- **Quotes, not IDs, link the matrix to the spec.** Verbatim quotes make spec drift visible without numbering every sentence.
- **Citations name the identifier.** A backticked identifier next to every link lets `check` catch ranges that went stale.
- **No dependencies.** Standard-library Python and a regex-based Rust reader. It runs anywhere Python runs, at the cost of type-level precision.
- **Data on stdout, progress on stderr.** Output can be piped or redirected.
- **Probes stay outside `tests/`.** They fail on purpose, so they must not break the project's test suite. `probe` copies them in only for the run.
- **Only Implemented-by citations are seeds.** Citations in Notes, findings and `unspecified.md` describe context and defects; counting them as spec would hide unspecified behaviour.

### Running the tests

From the project root:

```console
$ python3 -m unittest discover -s system-analysis-tools/spec-traceability/tests
.............................
----------------------------------------------------------------------
Ran 29 tests in 2.172s

OK
```

The tests build a tiny spec, Rust crate and matrix in a temporary
directory. They need `git` (for `impact`) but not `cargo`; `probe` is
tested only for its timeout, with a fake `cargo` on `PATH`. The
`RegressionTests` class holds one test per fixed bug.

### Extending

- **New subcommand.** Add a `cmd_<name>(a)` in `cli.py` and register it in `main()`. Use `_load(a)` for the spec, tree and matrix, and `_out(a, text)` for `-o` support. Return an exit code.
- **New check.** Add it to `check.run`, appending to `r.errors`, `r.warnings` or `r.notes`. Document the message in REFERENCE.md.
- **New status.** Add it to `matrix.STATUSES`. The summary table, HTML legend and checks pick it up. Add a colour to the HTML template's `--<status>` variables.
- **Different components.** Edit `report.DEFAULT_GROUPS` (name, path prefix; a prefix ending in `/` matches a directory).
- **Another language.** Replace `rust.py`'s `parse` with one that returns `Item`s with spans. Anchoring, `suggest`, `index`, `uncited` and `ancestry` then work unchanged. `probe` is Cargo-specific.
- Add a test in `tests/test_spectrace.py` for any behaviour change, and keep REFERENCE.md in step with `cli.py`.
