# spectrace reference

The exact behaviour of every command, option and file format. For concepts,
workflows and examples, see [README.md](README.md). For the agent
procedure, see [AGENTS.md](AGENTS.md).

Everything here was checked against `spectrace_lib/` and, where possible,
by running the tool on the Delos repository.

## Contents

- [Invocation](#invocation)
- [Global options](#global-options)
- [Path resolution](#path-resolution)
- [Output conventions and exit codes](#output-conventions-and-exit-codes)
- [Commands](#commands): [units](#units) · [check](#check) · [summary](#summary) · [grid](#grid) · [index](#index) · [uncited](#uncited) · [why](#why) · [suggest](#suggest) · [pin](#pin) · [impact](#impact) · [ancestry](#ancestry) · [probe](#probe) · [html](#html) · [export](#export)
- [Check messages](#check-messages)
- [Spec format](#spec-format)
- [Matrix format](#matrix-format)
- [Output formats](#output-formats)
- [Constants](#constants)
- [Environment and configuration](#environment-and-configuration)

## Invocation

```
spectrace [-h] [--root ROOT] [--spec SPEC] COMMAND [command options]
```

`spectrace` is a uv script (`#!/usr/bin/env -S uv run --quiet --script`,
`requires-python = ">=3.10"`, no dependencies). `python3 spectrace …` is
equivalent. `-h`/`--help` is accepted at the top level and by every
command.

## Global options

Global options must come **before** the command name.

| Option | Type | Default | Meaning |
|---|---|---|---|
| `--root ROOT` | path | `.` | Project root. The spec, `src/`, `tests/`, git and cargo are used from here. Made absolute before use. |
| `--spec SPEC` | path | `SPEC.md` | Spec file, relative to `--root`. |

## Path resolution

| Path | Resolved against |
|---|---|
| `--spec` | `--root` |
| The `matrix` argument of every command | the **current directory** |
| `-o`/`--output` | the current directory. Parent directories are created. |
| `--pins` | the current directory |
| `why` location `FILE:LINE` | `--root` (normalised with `os.path.normpath`; an absolute path never matches) |
| A citation's link target | the directory of the Markdown file that contains it; then made relative to `--root` |
| A test reference `tests/x.rs::f` | `--root` |

Source files are read from `<root>/src/**/*.rs` and `<root>/tests/**/*.rs`.
A citation may point at any file under the root (for example
`Cargo.toml`); non-Rust files are read as plain lines.

## Output conventions and exit codes

- Data goes to **stdout**. With `-o FILE`, data goes to the file and `wrote FILE` goes to **stderr**.
- `summary --write` prints `updated <matrix>/README.md` on stderr.
- Errors print one line on stderr and exit **1**, with no traceback. This covers the tool's own usage errors (`sys.exit(message)`), a missing file or directory (`<path>: No such file or directory`: matrix directory, spec, `--pins` file, matrix README for `summary --write`, `<root>/tests` for `probe`), a failing `git` (`git diff failed: <git's first error line>`), a malformed `why` location and a `probe` timeout.
- argparse errors (unknown command or option, missing argument, bad choice, non-integer for an `int` option) print usage on stderr and exit **2**.
- When the reader of stdout closes the pipe early (`spectrace … | head`), the command stops quietly, with nothing on stderr, and exits **141** (128 + SIGPIPE, as a process killed by the signal would).

| Command | 0 | 1 |
|---|---|---|
| `units` | always, unless… | `--uncovered` without a matrix |
| `check` | no errors (and, with `--strict`, no warnings) | any error; any warning with `--strict` |
| `summary` | always, unless… | `--write` and the README is missing or has no summary block |
| `grid`, `index`, `uncited`, `ancestry`, `pin`, `html`, `export` | always | only on the errors above (missing files) |
| `why` | some entry cites the line or its item (text or `--json`) | no entry cites it (text or `--json`); location not `FILE:LINE` |
| `suggest` | always, unless… | `--clause` names no entry (or `--matrix` is missing) |
| `impact` | always, unless… | no `--rev` and no commit in the README; `git` fails |
| `probe` | every probe test `confirms` and is claimed by an entry; or there are no probes | a probe did not build, a test passed, was ignored or did not run, a confirming test is not claimed, `tests/` holds a stale copy, or a probe file timed out |

## Commands

In the synopses, `M` is the matrix directory.

### units

Split the spec into units: the sentences (and code blocks) a matrix must quote.

```
spectrace units [--uncovered] [--json] [-o OUTPUT] [M]
```

| Argument / option | Type | Default | Meaning |
|---|---|---|---|
| `M` | path, optional | none | Matrix directory. Required by `--uncovered`; otherwise loaded but unused. |
| `--uncovered` | flag | off | Only units whose coverage is below 0.5. |
| `--json` | flag | off | JSON array of [unit objects](#unit-object). |
| `-o`, `--output` | path | stdout | Write to a file. |

Text output: one line per unit, `f"{line:4}  {item or '-':7} {text}"`.

```
  10  -       Delos must be built using the Rust programming language.
  20  1       Delos must be able to load and display existing images.
```

### check

Verify quotes, coverage, entries, citations, anchoring, tests, links and the README summary.

```
spectrace check [--pins PINS] [-v] [--strict] [--json] M
```

| Argument / option | Type | Default | Meaning |
|---|---|---|---|
| `M` | path | required | Matrix directory. |
| `--pins PINS` | path | none | A [pins file](#pins-file). Adds drift messages: citations `moved` (warning), `changed`/`missing` (error); spec units `changed`/`removed` (error), `added` (warning). |
| `-v`, `--verbose` | flag | off | Also print notes. |
| `--strict` | flag | off | Exit 1 when there are warnings. |
| `--json` | flag | off | Print a [check JSON object](#check-json) instead of text (still exits 1 on errors). |

Text output, all on stdout: every error as `error: …`, every warning as
`warning: …`, with `-v` every note as `note: …`, then a blank line and:

```
90 clause entries (122 entries) · 151 spec units, 2 not quoted · 594 citations, 0 of 588 checkable unanchored · 143 test refs
0 errors, 0 warnings, 2 notes (-v to list notes)
```

` (-v to list notes)` is appended only without `-v`. Every message is
listed under [Check messages](#check-messages).

### summary

Status counts per area, as the Markdown block kept in the matrix README.

```
spectrace summary [--write] M
```

| Argument / option | Type | Default | Meaning |
|---|---|---|---|
| `M` | path | required | Matrix directory. |
| `--write` | flag | off | Replace the [summary block](#readmemd) in `M/README.md` instead of printing. |

Output: a table `| Area | Clauses | Implemented | Partial | Diverges | Missing | Deferred | Process |`,
one row per area in order of first appearance (an entry with no area is
counted under `(none)`), a bold `**Total**` row, a blank line, and:

```
Spec coverage: 149 of 149 spec sentences are quoted by a clause entry. 594 code citations into 33 files; 73 distinct tests cited.
```

Only clause entries are counted. Empty status cells are blank in area
rows and `**0**` in the total row. Sentence coverage counts `text` units
only. Citations count all citations (note entries and loose citations
included); tests are distinct `(file, function)` pairs across all entries.

`--write` exits 1 with `README.md has no <!-- spectrace:summary --> ... <!-- /spectrace:summary --> block`
if the block is absent, or with `<M>/README.md: No such file or directory`
if the README is missing. **It writes into the matrix directory.**

### grid

Clause × component matrix.

```
spectrace grid [--by {group,file}] [--format {md,csv,json}] [-o OUTPUT] M
```

| Argument / option | Type | Default | Meaning |
|---|---|---|---|
| `M` | path | required | Matrix directory. |
| `--by` | `group` or `file` | `group` | Columns are [components](#components) or cited file paths (sorted). |
| `--format` | `md`, `csv` or `json` | `md` | Output format; see [grid formats](#grid-formats). |
| `-o`, `--output` | path | stdout | Write to a file. |

Rows are clause entries only, in matrix order. A cell is the number of
cited lines (sum of `end - start + 1` over the entry's citations, with
overlaps counted twice). Only columns with at least one citation appear.

### index

Code → spec: each cited item in `src/` with the entries citing it.

```
spectrace index [--json] [-o OUTPUT] M
```

| Argument / option | Type | Default | Meaning |
|---|---|---|---|
| `M` | path | required | Matrix directory. |
| `--json` | flag | off | [Index JSON](#index-json), including uncited items. |
| `-o`, `--output` | path | stdout | Write to a file. |

Items are those of `src/**/*.rs`, excluding test items, `impl` blocks and
`mod`s. An item is cited by an entry when any citation of that entry
overlaps its line span. All entries count (clause, note, loose citations
excepted). Text output lists only cited items:

```
## `src/engine/cache.rs`

- `Entry` (10-10): D1, U7
```

### uncited

Functions no entry cites, per file.

```
spectrace uncited [--json] [-o OUTPUT] M
```

| Argument / option | Type | Default | Meaning |
|---|---|---|---|
| `M` | path | required | Matrix directory. |
| `--json` | flag | off | [Uncited JSON](#uncited-json). |
| `-o`, `--output` | path | stdout | Write to a file. |

Only `fn` items of `src/` (not tests) are counted. A function is cited when
an entry's citation overlaps it (as in `index`). *Cited share of fn lines*
is the share of the lines of all functions in the file that any citation
(including loose ones) covers. Text output: a table sorted by share
ascending, then a bullet list per file (sorted by path) of
`` `Name (start-end)` `` for every uncited function.

### why

Which entries depend on a line.

```
spectrace why [--json] M FILE:LINE
```

| Argument / option | Type | Default | Meaning |
|---|---|---|---|
| `M` | path | required | Matrix directory. |
| `FILE:LINE` | string | required | Root-relative path and 1-based line, split at the last `:`. Example: `src/graph.rs:243`. Anything else (no `:`, a non-numeric line) exits 1 with `why needs FILE:LINE, e.g. src/graph.rs:243 (got '…')`. |
| `--json` | flag | off | [Why JSON](#why-json). The exit code is the same as in text mode. |

*Enclosing item*: the item with the smallest span that contains the line
(any kind, `impl` included). *Direct*: entries with a citation that
includes the line. *Same item*: entries with a citation overlapping the
enclosing item, minus the direct ones. All entries count, clause and note.

```
src/graph.rs:243 in Graph::connect (243-264)
  cited directly by F1 · Load and display images as graph nodes [Implemented]
  cited directly by F2 · The document is a DAG [Implemented]
  same item cited by S5 · Specific outputs as inputs, relocation and export targets [Partial]
```

When there is no item, the first line reads `… in None (None)`. When
nothing cites the code: `  no entry cites this code`, exit 1 (with
`--json`, the JSON with empty `direct` and `same_item`, also exit 1).

### suggest

Rank code items that may implement some spec text. Does not need a matrix
unless `--clause` is used.

```
spectrace suggest [--matrix MATRIX] [--clause CLAUSE] [--top TOP] [--json] [TEXT ...]
```

| Argument / option | Type | Default | Meaning |
|---|---|---|---|
| `TEXT ...` | strings, zero or more | empty | Spec text; the words are joined with spaces. |
| `--matrix MATRIX` | path | none | Matrix directory, needed for `--clause`. |
| `--clause CLAUSE` | string | none | Entry ID (`F3.1.1`) or anchor (`f3-1-1`). Its quotes joined, or its title if it has no quotes, replace `TEXT`. |
| `--top TOP` | int | `15` | Maximum rows. |
| `--json` | flag | off | [Suggest JSON](#suggest-json). |

Candidates: items of `src/**/*.rs`, excluding test items and any `impl`,
`mod`, `struct`, `enum` or `type` longer than 200 lines. Words of 3+
letters, minus stop words, are lower-cased and stemmed (so `saves` →
`sav`, `never` → `nev`); identifiers are split at `_` and camel case.
Each item's term weights: name ×4, doc comment ×3, string literals ×2,
`//` comments ×2, identifiers ×1. Score: Σ over query words present of
`ln((n+1)/(df+0.5)) × ln(1+weight)`, divided by `1 + ln(1 + lines/40)`.
Rows with score 0 are dropped. Empty text prints nothing and exits 0.

Text output: `f"{score:6.2f}  {file}:{start}-{end}  {kind} {item}  [{matched stems}]"`.

```
 16.71  src/project.rs:392-399  fn write_atomic  [atomic, file, nev]
```

### pin

Record the code behind every citation and the digest of every spec unit,
for later drift checks.

```
spectrace pin [-o OUTPUT] M
```

| Argument / option | Type | Default | Meaning |
|---|---|---|---|
| `M` | path | required | Matrix directory. |
| `-o`, `--output` | path | stdout | Write to a file (conventionally `M/pins.json`). |

Output: a [pins file](#pins-file). Citations whose file is missing or whose
range ends past the file are skipped. Pinning the current tree reflects the
working tree, not a commit; pin when the tree is at the analysed commit.

### impact

Entries whose cited lines or quoted spec sentences a diff touches.

```
spectrace impact [--rev REV] [--slack SLACK] [--json] M
```

| Argument / option | Type | Default | Meaning |
|---|---|---|---|
| `M` | path | required | Matrix directory. |
| `--rev REV` | git revision | the [README commit](#readmemd) | Compare the working tree with this revision. Any revision works: citations are carried from the README commit to `REV` first (step 3). |
| `--slack SLACK` | int | `0` | Lines of slack around each citation. |
| `--json` | flag | off | [Impact JSON](#impact-json). |

Procedure:

1. `git -C ROOT diff -U0 REV -- src tests <spec> <every path the matrix cites>`: working tree against `REV`, tracked files only. Cited paths outside the root (`../…`) are left out. A failing `git` (unknown revision, not a repository) exits 1 with `git diff failed: <git's first error line>`.
2. For each hunk, the old-side range `a..a+n-1`. A pure insertion (`n = 0`) after line `a` maps to `a..a`; an insertion before line 1 (`a = 0`) maps to `1..1`.
3. **Carrying citations to `REV`.** Citations give line numbers at the README commit (`BASE`). If `REV` resolves to a different commit (`git rev-parse --verify REV^{commit}`), each citation is moved into `REV`'s numbering through `git -C ROOT diff -U0 --no-renames BASE REV -- <every cited path>`:
   - a line before any changed hunk keeps its number, and a later line shifts by the hunks before it;
   - a line inside a changed hunk widens to that hunk's new lines (so a citation of code edited between `BASE` and `REV` covers the edited version);
   - a deleted line becomes the line before the gap (line 1 at the top), the same convention as step 2.

   A citation's range is the carried first line's start to the carried last line's end. Citations of a file that `REV` no longer has (deleted, or renamed, since renames are not followed) go to `unmapped` instead. Without a README commit, or when `REV` is `BASE`, nothing is carried. `BASE` may be newer than `REV`: the diff runs backwards just the same, and code added after `REV` collapses to the point where it was inserted.
4. **Code**: a citation is hit when a hunk on its file overlaps its (carried) range widened by `slack`. Reported as `path:start-end ∩ a-b` under the entry ID (`(loose)` for loose citations), or `path:start-end → c-d ∩ a-b` when carrying moved it to `c-d`. `path:start-end` is always the matrix's own citation.
5. **Spec**: if the spec changed, the spec at `REV` is read with `git show REV:<spec>` and split into units. Each unit whose line span overlaps a hunk is reported, under every entry that quotes it in that old spec, or `(unclaimed)`. Format: `<spec>:<line> “<first 60 characters>”`, where `<spec>` is the spec path relative to the root (`SPEC.md` by default, `docs/spec.md` with `--spec docs/spec.md`).
6. **Files without citations**: changed files (other than the spec) that no citation points at. Since only `src/`, `tests/`, the spec and cited files are diffed, these are always under `src/` or `tests/`.

The spec side needs no carrying: it matches quoted sentences by text.

Text output: `changes since REV:`; `  (citations carried from BASE to REV)`
when step 3 ran; then per entry (sorted by ID) `\n<ID> · <title> [<status>]`
followed by `  spec  …` and `  code  …` lines; then per entry with unmapped
citations `\n<ID>: cited file gone at REV: <citations>`; then
`\nchanged files no entry cites: a, b` if any; then `  no entry is affected`
if no entry was hit and none is unmapped.

On Delos, against a revision five commits before the analysed one:

```
$ spectrace impact system-models/spec-traceability --rev HEAD~5
changes since HEAD~5:
  (citations carried from 197e30c to HEAD~5)

F1 · Load and display images as graph nodes [Implemented]
  code  src/worker.rs:270-277 → 269-274 ∩ 269-269
  code  src/engine/source.rs:43-65 → 39-60 ∩ 59-59
  code  src/nodes/basic.rs:77-91 ∩ 90-90
…
R3 · GPU acceleration wherever possible [Partial]
  code  src/engine/mod.rs:283-309 → 215-241 ∩ 238-238
  code  src/engine/mod.rs:257-271 → 204-204 ∩ 204-204
```

`mod.rs:257-271` (`source_on_gpu`) did not exist at `HEAD~5`, so it
collapses to the point where it is later inserted.

### ancestry

Code with no spec ancestry.

```
spectrace ancestry [--top TOP] [--recall PREFIX] [--json] [-o OUTPUT] M
```

| Argument / option | Type | Default | Meaning |
|---|---|---|---|
| `M` | path | required | Matrix directory. |
| `--top TOP` | int | `25` | Markdown only: maximum rows of *Uncited branches* and maximum groups of *Unspecified code*. *Unspecified state* is never truncated. |
| `--recall PREFIX` | str | off | Calibration mode instead of the report: score the analysis against hand-written `note` entries whose id starts with `PREFIX` (e.g. `U` for `unspecified.md`). See [Recall mode](#recall-mode). |

The report has three sections, in this order: *Unspecified state* (JSON
`state`), *Uncited branches of cited functions* (`spans`) and
*Unspecified code* (`clusters`).
| `--json` | flag | off | Full [ancestry JSON](#ancestry-json) (not truncated). With `--recall`: the recall JSON below. |
| `-o`, `--output` | path | stdout | Write to a file. |

#### Recall mode

With `--recall PREFIX`, `ancestry` prints a calibration table instead of
its report. It answers: does the automatic analysis flag the code behind
the gaps a human wrote down by hand? Only entries of kind `note` whose id
starts with `PREFIX` are scored. *Flagged* code is the lines of
`unspecified` functions plus the lines of uncited spans. Two measures:

- **Overlap** (lax). An entry counts as found when one of its citations overlaps:

  | `found_by` | Overlapping with |
  |---|---|
  | `code` | a function classed `unspecified` |
  | `state` | a function that writes unspecified state |
  | `span` | an uncited span |

  Flagged code is dense in UI files, so almost any range touches it. On Delos, citations moved to random places in the same files scored 12.8 of 16 against a real 10 of 16 (matrix as first written). Do not read overlap alone.
- **Enrichment**. The flagged share of the entries' cited code lines (*flagged share*), divided by the flagged share of all code lines in the files those entries cite (*chance share*). 1.0 is chance; higher means the analysis points at the hand-found gaps more than at random code. Code lines are non-blank masked lines other than lone brackets and punctuation. Calibrate on this.

Text output: one row per entry (`id`, `found_by` joined with `+` or
`MISSED`, flagged lines over code lines of its citations, title), then the
overlap line, the enrichment line and a metrics line. Exit code 0. On
Delos (matrix after the citation-tightening experiment):

```
U13   span+state           12/15   Choosing the backend at run time
U14   span+state          108/108  Navigating the views
U15   span                 39/44   Layer stacks grow from the inspector
U16   span+state           19/29   Session housekeeping

overlap: 16 of 16 (lax; compare with random placement)
enrichment: 51% of cited code lines flagged vs 32% of the same files = x1.57
state fields 20 · spans 98 (1123 code lines) · groups 141 (36 triggered) · support 275 fn/1951 lines · unspecified 162 fn/713 lines · specified 218 fn/4395 lines · partly specified 21 fn/947 lines
```

Before tightening the same set scored ×1.36 (33% vs 24%); see
[tightening.md](../../system-models/spec-traceability/tightening.md).

JSON (`--recall PREFIX --json`):

| Key | Type | Meaning |
|---|---|---|
| `found` | int | Entries with at least one `found_by` (overlap). |
| `total` | int | Entries scored. |
| `rows[]` | list | `{id, title, found_by: [str], flagged_lines: int, code_lines: int}`, with `found_by` drawn from `code`, `span`, `state` (sorted; empty = missed). |
| `flagged_share` | float (3 decimals) | Flagged lines over code lines, summed over all rows' citations. |
| `chance_share` | float (3 decimals) | Flagged code lines over all code lines of the files the rows cite. |
| `enrichment` | float (2 decimals) or `null` | `flagged_share / chance_share` (computed before rounding); `null` when nothing in those files is flagged. |
| `metrics.classes` | object | Per class (`specified`, `partly specified`, `unspecified`, `support`): `{items, lines}`. |
| `metrics.state_fields` | int | Unspecified state fields. |
| `metrics.spans`, `metrics.span_lines` | int | Uncited spans and their code lines. |
| `metrics.groups`, `metrics.triggered_groups` | int | Unspecified-code groups, and those with a trigger. |

Algorithm:

1. **Items**: `fn`, `const` and `static` items of `src/**/*.rs`, excluding test items.
2. **Seeds**: lines cited by *clause* entries in a section whose label starts with `Implemented by` (case-insensitive). Other citations (Notes, note entries) are *note lines*.
3. **Class**: an item with seed lines covering ≥ 30% of its lines is `specified`; with some seed lines, `partly specified`; otherwise `unspecified` for now.
4. **Reachability**, per clause: start from the items referred to on the clause's seed lines. From each reached item, follow the references on its seed lines if it has any, otherwise on all its lines. Unspecified items reached this way become `support`. Each item records the clauses that reach it.
5. **References** are lexical, per masked source line: `name(` calls, `Type::name`/`module::name`/`Self::name` paths, `.name(` method calls, `Name {` (a Dioxus component in `rsx!`), and bare names of `const`/`static` items. `.name` without `(` is a field access and ignored. Common names (`new`, `get`, `set`, `len`, `push`, `write`, `read`, `load`, `save`, `name`, `id`, … see `ancestry.STOP`) are ignored unless qualified.
6. **Unspecified state**: for each field line `name: Type` of each `struct` in `src/`, the field is reported when no seed line (other than its own declaration) contains the word, and some item writes it (`.field =`, `.field += …`, `.field.push`, `.set`, `.write`, `.insert`, `.remove`, `.clear`, `.pop`, `.take` or `.store`; or `field.set(`/`field.write(`). Sorted by number of clauses through writers, descending.
7. **Uncited spans**: inside every specified or partly specified item, runs of non-seed lines with at least 3 non-trivial lines (lines of only brackets, `} else {`, `_ => {}` and similar are trivial). Sorted with spans that write state or call support/unspecified items first, then by size.
8. **Unspecified code**: unspecified items grouped into connected components of the reference graph. *Triggers* are non-seed lines of specified or partly specified items that refer to a member. Groups with triggers come first, then by size.

Markdown output: a class table, then `## Unspecified state`,
`## Uncited branches of cited functions` and `## Unspecified code`.
Spans show `✎` when they change state. Groups are shown when they have
triggers or at least 10 lines, up to `--top`; a closing line counts the
rest. Each group shows its line count, files, `changes state` when a
member matches the mutation pattern, and `noted in <IDs>` or
`**not noted anywhere**`.

### probe

Run the probe tests kept with the matrix.

```
spectrace probe [--cargo-args CARGO_ARGS] [--timeout TIMEOUT] M
```

| Argument / option | Type | Default | Meaning |
|---|---|---|---|
| `M` | path | required | Matrix directory. Probes are `M/probes/*.rs` (not recursive). |
| `--cargo-args` | string | `""` | Extra arguments for `cargo test`, split on whitespace. Example: `--cargo-args '--release'`. |
| `--timeout` | int (seconds) | `1800` | Timeout per probe file. |

Procedure, per probe file `<stem>.rs`, in name order:

1. Refuse to start if `<root>/tests/` already contains a file whose name starts with `zz_spectrace_` (exit 1: `tests/ already holds [...]: a previous run was killed. Delete them and retry.`).
2. Copy it to `<root>/tests/zz_spectrace_<stem>.rs`.
3. Run `cargo test <cargo-args> --test zz_spectrace_<stem> -- --test-threads=1` in the root, capturing output.
4. Delete the copy (in a `finally`, so also when cargo fails or times out). If `cargo test` runs longer than `--timeout`, stop: exit 1 with `probe <stem>.rs: cargo test timed out after <TIMEOUT> s (raise --timeout)`; later probe files are not run.
5. If cargo printed no `test NAME ... ok|FAILED|ignored` line and exited non-zero: print `<stem>: did not build or run` and the last 3000 characters of its output.
6. Otherwise, for each test function in the probe file (`#[test]` or `#[tokio::test…]`, other attributes and `async` allowed): verdict `confirms` (FAILED), `no-longer-violated` (ok), `ignored`, or `did-not-run` (not in the output).

Output per test: `f"{verdict:20} {name}  [{entry or 'no finding cites it'}]"`;
for a failure, the panic message (the `---- NAME stdout ----` block up to
the first blank line, last 600 characters), indented by four spaces.

**Claims.** An entry claims a probe test when a section whose label
starts with `Kind` or `Confirmed` (case-insensitive) contains
``Probe `test_name` `` or ``probe `test_name` ``. On Delos:
``**Kind.** Defect. … **Confirmed.** Probe `failed_import_leaves_nothing_behind` …``.

No probes directory, or no `.rs` files in it: no output, exit 0.

**Side effects:** creates and deletes one file in `<root>/tests/` per probe
file; Cargo writes to `target/`.

### html

Self-contained HTML viewer.

```
spectrace html [-o OUTPUT] M
```

| Argument / option | Type | Default | Meaning |
|---|---|---|---|
| `M` | path | required | Matrix directory. |
| `-o`, `--output` | path | stdout | Write to a file. Use `M/index.html` so that code links resolve. |

See [HTML output](#html-output). Runs `ancestry` internally (about 1.3 s on Delos).

### export

The whole matrix as JSON.

```
spectrace export [-o OUTPUT] M
```

| Argument / option | Type | Default | Meaning |
|---|---|---|---|
| `M` | path | required | Matrix directory. |
| `-o`, `--output` | path | stdout | Write to a file. |

See [Export JSON](#export-json).

## Check messages

`FILE` is the basename of a matrix file, `LINE` a line in it. `ID` is an
entry ID (empty for a loose citation). Quoted text is truncated as shown.

### Errors

| Message | Meaning and fix |
|---|---|
| `FILE:LINE: duplicate id ID (first in FILE:LINE)` | Two entries share an ID. Rename one. |
| `FILE:LINE: ID has anchor #A, expected #B` | The anchor must be the ID in lowercase with `.` replaced by `-`. |
| `FILE:LINE: ID needs **Status.** starting with one of Implemented, Partial, Diverges, Missing, Deferred, Process` | A clause entry has no status section, or it starts with another word. |
| `FILE:LINE: ID is STATUS but cites no code` | `Implemented`, `Partial` or `Diverges` with no citation anywhere in the entry. |
| `FILE:LINE: ID quotes text that is not in the spec: “…70 chars”` | The quote is not verbatim, or the spec changed. Copy the text again from the spec. |
| `SPEC:LINE: no entry quotes “…80 chars”` | A text unit is less than half covered by quotes. Quote it in an entry. (For a code block this is a note.) |
| `FILE:LINE: ID cites missing file PATH` | The link target does not exist under the root. Check the relative path from the Markdown file. |
| `FILE:LINE: ID cites PATH:A-B, outside the file (N lines)` | The range is out of bounds, or `A > B`. |
| `FILE:LINE: link text says line A but the link points at #LB` | The text's first line and the `#L` anchor differ. |
| `FILE:LINE: ID cites test file F, which does not exist` | A test reference names a missing file. |
| `FILE:LINE: ID cites F::N, which does not exist` | No `fn N` in that file. |
| `FILE:LINE: ID links to X.md, which is not in the matrix` | The link names a Markdown file not at the matrix's top level. |
| `FILE:LINE: ID links to X.md#A, which does not exist` | No `{#A}` in that file. |
| `README.md: the summary block is stale (run `spectrace summary --write`)` | The block differs from `summary` output (whitespace at the ends ignored). |
| `changed: KEY (IDS): re-read these entries` | `--pins`: the cited lines' text changed and is not found elsewhere in the file. |
| `missing: KEY (IDS): re-read these entries` | `--pins`: the cited file no longer exists. |
| `spec changed: SPEC line N “…60 chars” (IDS or unclaimed)` | `--pins`: a pinned sentence is gone and some unit now covers its old line. |
| `spec removed: SPEC line N “…” (IDS or unclaimed)` | `--pins`: a pinned sentence is gone and no unit covers its old line. |

### Warnings

| Message | Meaning and fix |
|---|---|
| `FILE:LINE: ID has no **Tests.** section (write 'None.' if there are none)` | An `Implemented`, `Partial`, `Diverges` or `Process` entry has neither a test reference nor a `Tests` section. |
| `README.md has no <!-- spectrace:summary --> block` | The matrix README exists but has no summary block. (No README: no message.) |
| `moved: KEY -> A-B (IDS)` | `--pins`: the same text is now at lines A-B. Update the citation. |
| `spec added: line N “…60 chars”` | `--pins`: a spec unit that was not pinned. |

### Notes (printed with `-v`)

| Message | Meaning |
|---|---|
| `SPEC:LINE: no entry quotes “…”` | An unquoted code block. Quote it only if it is normative. |
| `FILE:LINE: ID KEY unanchored: none of [...] occurs near the cited lines` | No backticked identifier of the citation's segment or line occurs within 3 lines of the range or names the enclosing item. Read it. |
| `FILE:LINE: ID KEY has no backticked identifier to anchor on` | Neither the segment nor the line has a backticked identifier. |

### Anchoring in detail

Applies to citations of `.rs` files only.

1. The *segment* of a citation is the text of its Markdown line from the end of the previous citation to the start of the next one.
2. The *identifiers* are the backticked spans in the segment, excluding those starting with `tests/` or `src/`. They are split into tokens at `::`, `.`, brackets, spaces, `,`, `<`, `>`, `&`, `!`, `=`, `:` and quotes; tokens of one character are dropped.
3. The citation is anchored if any token occurs as a whole word in lines `start-3 … end+3` (original text, comments included), or is a token of the name of the item enclosing `start`.
4. If the segment has no tokens or none matched, the same test is applied to all backticked identifiers of the whole line (citations removed). If neither has tokens, the citation is not checkable.

## Spec format

Any Markdown file. The parser reads it line by line:

| Construct | Treatment |
|---|---|
| A line whose stripped text starts with ```` ``` ```` | Opens or closes a fenced block. The whole block is one unit of kind `code`, with no item number. |
| `#`, `##`, … headings | End the current block and set the section. A unit's section is the heading stack without its first entry, joined by ` / `; if the stack has one heading, that heading. |
| Blank line | Ends the current block. |
| List item: `- `, `* ` or `+ `, optionally followed by a number `N.` or `N.N.N.`; or a bare `N.`/`N.N.` | Starts a new block. The number is the unit's item (`3.1.1`). The marker is removed. |
| Any other line | Continues the current block (or starts one). |

Each text block is normalised (`**` removed, whitespace collapsed to one
space) and split into sentences: a split happens after `.`, `!` or `?`
followed by whitespace and then a capital letter, `(`, `` ` ``, `*`, `"` or
`“`. Text inside `` `code spans` `` never splits; `e.g.`, `i.e.`, `etc.`,
`vs.` and `cf.` never end a sentence. All units of a block have the
block's first and last line.

The **flat spec** is all units joined with single spaces. Quotes are
matched against it, so a quote may span adjacent units.

## Matrix format

A matrix is a directory. Every `*.md` file at its top level is parsed, in
name order. Subdirectories (`generated/`, `probes/`) are not parsed.

```
<matrix>/
  README.md        overview; the analysed commit; the summary block
  NN-<area>.md     "# N · Area", then clause entries
  findings.md      note entries "## D-NAME · Title {#d-name}"
  unspecified.md   note entries "## U1 · Title {#u1}"
  review.md        optional; any other .md file is parsed the same way
  probes/*.rs      probe tests (see probe)
  pins.json        pins file (conventional name)
  generated/       views written by the tool; not parsed
  index.html       viewer (conventional name)
```

### Parsing rules

- Lines inside fenced code blocks (```` ``` ````) are ignored entirely.
- Every `{#anchor}` (`[a-z0-9-]+`) on any line is recorded as a link target of that file.
- A line starting with `# ` sets the **area** for the following entries of that file.
- A line matching an [entry heading](#entry-headings) starts an entry.
- Any other line starting with `## ` ends the current entry. A line starting with `### ` resets the current section but keeps the entry.
- Inside an entry, a line matching a [section label](#section-labels) starts a section.
- Inside an entry and before its first label, a line starting with `>` is a **quote** (text after `>`, stripped; empty quotes ignored).
- Every other line of an entry is appended to the current section (if any), and is scanned for citations, test references and links. Citations outside any entry are **loose citations**.

### Entry headings

```
## ID · Title {#anchor}
```

Regex: `^##\s+(ID)\s+·\s+(.+?)\s*\{#([a-z0-9-]+)\}\s*$`, where the separator
is the middle dot `·` (U+00B7) and `ID` is either:

- a capital letter, optionally more letters, then a dotted number and an optional lowercase letter: `F3.1.1`, `O10`, `U1`, `F1a`; or
- a capital letter, optionally more letters, then `-` and capitals, digits or dashes: `D-STALE`, `D-ADD-AFTER-SPLIT`.

The anchor must equal the ID lowercased with `.` → `-` (`F3.1.1` →
`f3-1-1`, `D-STALE` → `d-stale`); `check` reports a mismatch. An entry
with quotes is a **clause** entry; without, a **note** entry. An entry may
be looked up by ID or by anchor (`suggest --clause`).

### Section labels

```
**Status.** Implemented.
**Implemented by.**
**Tests.** None.
**Notes.** …
```

Regex: `^\*\*([^*]+?)\.?\*\*\.?\s*(.*)$`. The label is the bold text
without a trailing `.`; the rest of the line starts the section's text.
Any label is accepted. Labels with a meaning to the tool:

| Label | Used by |
|---|---|
| `Status` | The status: the first of `Implemented`, `Partial`, `Diverges`, `Missing`, `Deferred`, `Process` that the section text starts with, case-insensitive. |
| `Implemented by` (prefix, case-insensitive) | Citations here are [ancestry](#ancestry) seeds (clause entries only). |
| `Tests` | Its presence silences the "no **Tests.** section" warning. |
| `Kind`, `Confirmed` (prefix, case-insensitive) | Searched for probe claims. |

A label is recognised only at the start of a line. A repeated label
replaces the earlier section text.

### Status rules

| Status | Needs a citation | Needs `Tests` or a test reference (warning) |
|---|---|---|
| `Implemented`, `Partial`, `Diverges` | yes (error) | yes |
| `Process` | no | yes |
| `Missing`, `Deferred` | no | no |

### Quotes

- One quote per `>` line, verbatim from the spec. `**` is ignored and whitespace collapsed on both sides; everything else (backticks, single `*`, punctuation) must match.
- `…` (U+2026) elides text: the pieces must occur in order; the quote covers from the first piece's start to the last piece's end.
- The first occurrence in the flat spec is used.
- A unit is covered when the union of quoted spans covers at least 50% of its characters.

### Citations

```
[graph.rs:243-264](../../src/graph.rs#L243)
[mod.rs:395](../../src/ui/mod.rs#L395)
```

Regex: `\[([^\]\[]+?):(\d+)(?:-(\d+))?\]\(([^)#\s]+)#L(\d+)\)`.

- The link target (before `#L`) is the file, relative to the Markdown file. It is normalised and made relative to the root; that is the citation's `path`.
- The text's numbers are the range `start-end` (`end = start` if omitted). The text's file name is not checked; by convention it is the path's last components.
- The `#L` number must equal `start`.
- The *section* of a citation is the label it appears under (`""` before any label).
- Put the identifier the sentence is about in backticks in the same bullet, next to the link, for [anchoring](#anchoring-in-detail).

### Test references

```
`tests/engine.rs::cycles_are_rejected`
`src/expr.rs::stacks_are_lists_of_layers`
```

Regex: `` `((?:tests|src)/[^`:\s]+\.rs)::([A-Za-z_][A-Za-z0-9_]*)` ``. The
file is root-relative. `check` verifies that the file has a `fn` with that
name (any `fn`, test or not). Only references inside entries count.

### Links between entries

```
[F3.2](#f3-2)
[P7](04-sessions-and-projects.md#p7)
```

Regex: `\[([^\]]+)\]\(([^)#\s/]*\.md)?#([a-z0-9-]+)\)`. The file part may
not contain `/`. Links into other directories (`../invariants/findings.md#…`)
and links without an anchor are not checked.

### README.md

| Content | Used by |
|---|---|
| ``commit `<6–40 lowercase hex digits>` `` (first match; `commit` in any case, so ``Commit `abc1234` `` works) | `impact` default `--rev`; `commit` field of `pin`, `export`, `html`. Without it they get `null`. |
| `<!-- spectrace:summary -->` + newline … `<!-- /spectrace:summary -->` | `summary --write` replaces the content; `check` compares it. The opening marker must be followed directly by a newline. |

The README is also parsed as a matrix file, so entries or citations in
it count.

### Components

`grid --by group` assigns each cited path to the first matching component
(`report.DEFAULT_GROUPS`); a prefix ending in `/` matches a directory,
others match the exact path:

| Component | Path |
|---|---|
| `graph` | `src/graph.rs` |
| `nodes` | `src/nodes/` |
| `engine` | `src/engine/` |
| `formats` | `src/formats.rs` |
| `worker` | `src/worker.rs` |
| `expr` | `src/expr.rs` |
| `project` | `src/project.rs` |
| `ui/state` | `src/ui/state.rs` |
| `ui/session` | `src/ui/session.rs` |
| `ui/views` | `src/ui/` |
| `build` | `Cargo.toml` |
| `test code` | `tests/` |
| `other` | anything else |

### Probe files

Rust integration test files in `<matrix>/probes/`, written against the
crate's public API. Each test asserts what the spec says, so it fails
while the defect exists. The probe is claimed by a finding as described in
[probe](#probe).

## Output formats

All JSON is UTF-8, indented by one space. `check`, `why`, `impact` and
`suggest` print JSON to stdout; the others honour `-o`.

### Unit object

| Field | Type | Meaning |
|---|---|---|
| `index` | int | 0-based position. |
| `line`, `end_line` | int | First and last line of the unit's block, 1-based. |
| `section` | string | Heading path. |
| `item` | string | Spec item number, or `""`. |
| `kind` | `"text"` or `"code"` | |
| `text` | string | Normalised text. |
| `digest` | string | First 12 hex digits of SHA-1 of `text`. |

`export` adds `covered` (float 0–1, two decimals).

### Check JSON

```json
{"errors": [...], "warnings": [...], "notes": [...],
 "stats": {"entries": 122, "clauses": 90, "spec_units": 151, "uncovered_units": 2,
           "citations": 594, "anchor_checked": 588, "unanchored": 0, "tests": 143}}
```

Messages are the strings listed under [Check messages](#check-messages).
`notes` is always included.

### Grid formats

- **md**: `| Clause | Status | <cols> | tests |`; cell `•` for 1–19 lines, `●` for 20 or more, blank for 0; `tests` blank for 0; a legend line at the end.
- **csv**: header `id,title,status,<cols>,tests`; title double-quoted; cells are line counts (0 when none).
- **json**: `{"cols": [names], "rows": [{"id", "title", "status", "cells": {col: lines}, "tests": int}]}`.

### Index JSON

`{"src/file.rs": [{"kind", "name", "file", "start", "end", "test", "entries": [IDs]}]}`
for every `.rs` file under `src/` (files without items map to `[]`).
`name` is qualified by the impl's type (`Graph::connect`).

### Uncited JSON

`[{"file", "functions": int, "cited": int, "line_share": float (3 decimals), "uncited": ["Name (start-end)"]}]`,
one object per file with at least one function, in path order.

### Why JSON

`{"path", "line", "item": name or null, "item_lines": "start-end" or null, "direct": [IDs], "same_item": [IDs]}`.
Exit 1 when `direct` and `same_item` are both empty.

### Suggest JSON

`[{"score": float, "item", "kind", "file", "start", "end", "matched": [stems]}]`.

### Pins file

```json
{
 "commit": "197e30c",
 "spec": "SPEC.md",
 "citations": {
  "src/nodes/basic.rs:62-95": {
   "path": "src/nodes/basic.rs", "start": 62, "end": 95,
   "digest": "c06268f515c99a36",
   "text": ["pub struct StaticImage;", "", "..."],
   "entries": ["D1", "F1"]
  }
 },
 "spec_units": {
  "0": {"line": 3, "digest": "ebd035b74674",
        "text": "Delos is to be an image editing tool focused on combining existing content with generative content.",
        "entries": ["D1"]}
 }
}
```

| Field | Meaning |
|---|---|
| `commit` | README commit, or `null`. Informational; `check --pins` ignores it. |
| `spec` | Spec file basename. Informational. |
| `citations` | Keyed by `path:start-end`, one per distinct key. `digest`: first 16 hex digits of SHA-1 of the lines joined by `\n`, each right-stripped. `entries`: IDs of all entries with that key (`""` for loose). |
| `spec_units` | Keyed by unit index as a string. `entries`: clause IDs quoting it. |

Verification (`check --pins`): a citation is `same` if its current lines
have the digest; `moved` to the first window of the same length (from the
top of the file) that has it; otherwise `changed`; `missing` if the file is
gone. Spec units are compared by digest: a pinned digest no longer present
is `changed` if some current unit covers its old line, else `removed`; a
current digest not pinned is `added`.

### Impact JSON

```json
{"rev": "HEAD",
 "base": "197e30c",
 "carried": false,
 "code": {"R1": ["Cargo.toml:4-4 ∩ 4-4"], "P6.1": ["src/project.rs:390-399 ∩ 395-395"]},
 "spec": {"R1": ["SPEC.md:10 “Delos must be built using the Rust programming language.”"]},
 "unmapped": {},
 "files_without_citations": []}
```

| Key | Type | Meaning |
|---|---|---|
| `rev` | str | The revision compared with the working tree, as given. |
| `base` | str | The commit the citations describe (README commit), or `rev` when the README has none. |
| `carried` | bool | Whether citations were carried from `base` to `rev` (they are different commits). |
| `code` | object | Entry ID → hit strings (`path:start-end[ → c-d] ∩ a-b`). |
| `spec` | object | Entry ID (or `(unclaimed)`) → `<spec>:<line> “<text>”`. |
| `unmapped` | object | Entry ID → citations whose file `rev` does not have. |
| `files_without_citations` | list | Changed files no citation points at. |

### Ancestry JSON

| Field | Content |
|---|---|
| `counts` | `{class: {"items": int, "lines": int}}` for `specified`, `partly specified`, `support`, `unspecified` (classes with no items are absent). |
| `items` | Keyed `"<file>::<name>@<start>"`: `{"name", "file", "start", "end", "class", "cited_share": float, "clauses": [IDs reaching it]}`. |
| `clusters` | `[{"lines", "files", "members": ["Name (file:start-end)"], "mutates": bool, "triggers": [{"from": item name, "file", "line"}], "noted_in": [IDs]}]`, in report order. |
| `state` | `[{"field": "Struct.field", "file", "line", "declared_on_cited_line": bool, "writers": [{"item", "file", "start", "class"}], "clauses_through_writers": [IDs], "noted_in": [IDs]}]`. |
| `spans` | `[{"item", "file", "start", "end", "code_lines", "first": first code line (≤100 chars), "calls": [≤8 names], "writes": int, "noted_in": [IDs]}]`. |

### Export JSON

```json
{"commit": "197e30c",
 "entries": [Entry, ...],
 "spec_units": [Unit + "covered", ...]}
```

Entry object:

| Field | Type | Meaning |
|---|---|---|
| `id`, `title`, `anchor` | string | From the heading. |
| `file` | string | Matrix file basename. |
| `line` | int | Heading line. |
| `area` | string | The file's `# ` heading, or `""`. |
| `kind` | `"clause"` or `"note"` | |
| `status` | string | Recognised status, or `""`. |
| `quotes` | [string] | Raw quote text. |
| `sections` | {label: text} | Section text as written (Markdown). |
| `citations` | [{`entry`, `section`, `path`, `start`, `end`, `md`: `"file.md:line"`}] | |
| `tests` | [string] | `"tests/x.rs::name"`. |
| `links` | [string] | `"file.md#anchor"` (same-file links use the entry's own file). |

### HTML output

One file, no external resources. Title `Spec Traceability`; header
`Spec ↔ code traceability` with `N clauses · M spec sentences · commit X`.
The data is embedded as `const D = {...}`:

| Key | Content |
|---|---|
| `commit` | README commit or `null`. |
| `entries` | Export entries plus `sections_html` and `quotes_html`. |
| `units` | Unit objects plus `entries` (quoting IDs) and `covered`. |
| `grid` | `grid --by file --format json`. |
| `code` | Index JSON, each item with `ancestry` (its class, or `""`). |
| `statuses` | The six statuses. |

Tabs: **Spec**, **Clauses**, **Matrix**, **Code**. The last tab is
remembered in `localStorage` under `spectrace-tab`. Links to other entries
open them in the side panel; other links keep their href as written in the
matrix (relative to the page), so code links resolve only when the page
sits in the matrix directory. Light and dark colour schemes follow the
system setting.

## Constants

| Constant | Value | Where | Meaning |
|---|---|---|---|
| `COVERED` | 0.5 | `check.py` | Fraction of a unit that quotes must cover. |
| `NEAR` | 3 | `check.py` | Lines of slack for anchoring. |
| `SPECIFIED` | 0.3 | `ancestry.py` | Seed share for `specified`. |
| `SPAN_MIN` | 3 | `ancestry.py` | Minimum non-trivial lines in an uncited span. |
| `min_lines` | 10 | `ancestry.report_md` | Untriggered groups smaller than this are hidden in Markdown. |
| grid threshold | 20 | `report.grid_md` | `•` below, `●` at or above. |
| `PREFIX` | `zz_spectrace_` | `probe.py` | Name prefix of probe copies in `tests/`. |

None of these can be changed from the command line.

## Environment and configuration

`spectrace` reads no environment variables and no configuration files.
Behaviour depends only on the arguments, the files under `--root`, the
matrix directory, and (for `impact`) the git repository at `--root`. `probe`
runs `cargo`, which honours Cargo's own environment variables
(`CARGO_TARGET_DIR`, `RUSTFLAGS`, …).
