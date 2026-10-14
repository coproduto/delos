# invkit: tools for invariant catalogues

`invkit` helps people build, check and maintain an **invariant catalogue**.
An invariant catalogue models a system through the facts its code assumes
are always true: what is assumed, which code relies on it, what keeps it
true, and what happens if it breaks. The model it supports for Delos lives
in [`system-models/invariants/`](../../system-models/invariants/).

- Exact options, exit codes and file formats: [REFERENCE.md](REFERENCE.md).
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

### What question it answers

The catalogue answers: *which assumptions does this system rest on, and
which of them does a given change touch?* invkit supports that model in
three ways:

| Need | Commands |
|---|---|
| **Build** a catalogue: find where to read, and gather evidence for each invariant | `scan`, `writes`, `refs` |
| **Check** a catalogue: links point at the right code, tests exist, counts agree, violations still reproduce | `check`, `probe`, `catalog`, `relate` |
| **Maintain** a catalogue and **review changes** against it | `pin`, `check --pins`, `impact`, `recall` |

### When to use it

- You are writing an invariant catalogue for a Rust code base and want a
  reading order and evidence (who writes a field, who uses a function).
- You have a catalogue and want to know it is still accurate after the code
  moved on.
- You are reviewing a diff and want to know which invariants' evidence it
  touches, so a human can judge whether the change alters a premise.
- You want CI to fail when a catalogue's links, tests or counts go stale.

### When not to use it

invkit does the mechanical part. It cannot tell you:

- **What the invariants are.** `scan` points at places worth reading. A
  person (or agent) still reads the code and states each invariant.
- **Whether an invariant holds.** `check` verifies that citations, tests and
  counts are consistent. It does not prove anything about the code. Only
  `probe` executes code, and only for the probes you wrote.
- **Whether a change breaks an invariant.** `impact` says which invariants
  cite lines the diff touches. Whether the change alters the premise is a
  human judgment. A change to uncited code can still break an invariant.
- **Which invariants are the roots.** `relate` ranks invariants by explicit
  links. On Delos it does not recover the roots the README argues for
  (see [Limitations](#limitations-and-accuracy)).
- **Anything about non-Rust code.** The source analysis (`scan`, `writes`,
  `refs`, test detection) understands Rust only.

## Concepts and terminology

### The catalogue

| Term | Meaning |
|---|---|
| **Catalogue** | A directory of Markdown files: `README.md`, one `NN-<area>.md` per area, `findings.md`, and `probes/*.rs`. Format: [REFERENCE.md › The catalogue format](REFERENCE.md#the-catalogue-format). |
| **Invariant** | One `## ID · Title {#id}` entry, e.g. `G2 · The graph is acyclic`. IDs are capital letters plus a number (`G2`, `R11`), unique across the catalogue. |
| **Area** | The topic of one `NN-<area>.md` file, taken from its `# ` heading. |
| **Section** | A part of an invariant started by a bold label. Canonical names: `statement`, `maintained` (Maintained by), `depended` (Depended on by), `depends` (Depends on), `tests`, `broken` (If broken), `status`. Any other bold label becomes a note, `note:<slug>`. |
| **Citation** | A Markdown link from the catalogue to code, e.g. `[graph.rs:103-107](../../src/graph.rs#L103)`. A *line citation* has a range; a *whole-file citation* does not. |
| **Clause** | The text around a citation, from the previous link to the next one. `check` uses it for anchoring. |
| **Test reference** | A backticked test, e.g. `` `tests/engine.rs::cycles_are_rejected` ``. |
| **Cross-reference (xref)** | A link to another invariant or catalogue page, e.g. `[I1](04-structural-identity.md#i1)`. |
| **Finding** | A `## Finding N · …` entry in `findings.md` describing a violation, with the invariant it violates and the probe that confirms it. |
| **Probe** | A Rust integration test in `probes/` that asserts a violated invariant. It **fails** while the violation exists. |
| **Fragile spot** | A row of the numbered table in `findings.md`: an invariant that holds today but is easy to break. |
| **Root** | An invariant many others follow from: an architectural decision. The README names them in a Mermaid `subgraph Roots`. |
| **Analysed commit** | The commit named in the catalogue README as ``commit `197e30c` ``. Line numbers refer to it. |

### Status classes

invkit classifies each invariant's **Status** section:

| Class | When | Example from Delos |
|---|---|---|
| `holds` | The status is a plain "Holds." with no condition. | G1: "Holds. Enforced by API checks in more than one place." |
| `qualified` | Holds, but with a stated condition (contains only, until, must, not checked, is not, or anything other than a plain "Holds."). This is the catalogue's notion of fragile. | G2: "Holds, but only by the discipline of the code that writes edges." |
| `violated` | The label or text contains `VIOLATED`. | I1, U6 |
| `inherited` | "Inherits the … violation". | I4 |

### `check` output

| Term | Meaning |
|---|---|
| `error` / `warn` / `info` | Issue levels. Only errors (and pin drift) fail `check`. `info` lines (notes) appear only with `-v`. |
| **Anchored** | A citation whose clause matches the cited code. `name`: an identifier from the clause appears within ±3 lines of the range or names the enclosing function. `words`: no identifier, but distinctive words or numbers do. |
| **Unanchored** | Nothing from the clause was found near the cited lines. A prompt to re-read, not a verdict. |
| **Unchecked** | Nothing to compare: a whole-file link, or a clause with no identifiers or distinctive words. |
| **Drift** | Cited files that differ between the analysed commit and the working tree. |
| **Sparse** | An invariant missing one of Maintained by, Depended on by, Tests, If broken. |

### Pins

| Term | Meaning |
|---|---|
| **Pin** | The exact text of one cited line range, recorded by `pin`. |
| `same` | The pinned lines are still at the cited line numbers. |
| `moved` | The same lines are elsewhere in the file. invkit suggests the new range. |
| `changed` | The pinned lines no longer exist. Re-read the invariant against the new code. |
| `missing` | The cited file is gone. |

### Probe verdicts

| Verdict | Meaning |
|---|---|
| `confirmed` | A probe named by a finding failed: the violation still reproduces. |
| `no-longer-violated` | A probe named by a finding passed: the violation was fixed, or the probe is wrong. |
| `holds` | A probe no finding names passed. |
| `unexpected-failure` | A probe no finding names failed. |
| `error` | The probe did not build, or a test did not run. |

### Scan

| Term | Meaning |
|---|---|
| **Candidate** | One line flagged by a detector. |
| **Kind** | The detector: `claim`, `panic`, `guard`, `normalise`, `field-write`, `identity`, `key`, `limit`, `dual`, `order`, `durable`, `snapshot`, `borrow`, `sentinel`, `contract`, `default`. Each is defined in [REFERENCE.md › scan](REFERENCE.md#scan). |
| **Item** | The enclosing function as `Type::method`, or the enclosing type or module. `(top level)` when there is none. |
| **Hotspot** | An item ranked by how many *kinds* of evidence it holds, weighted by `scan.WEIGHTS`. |
| **Identity report** | Every id counter with its scope (per value or per process), the functions that issue ids, where the counter's owner is rebuilt, and every hash or cache key. |
| **Per value** counter | A counter in a `Clone` struct: cloning copies it and rebuilding the owner restarts it, so ids can repeat across values. |

## Requirements and installation

| Requirement | Needed for | Notes |
|---|---|---|
| Python 3.10 or later | everything | Standard library only. No `pip install`. |
| `uv` | running `invkit` directly | The launcher is a `uv` script. Without `uv`, run `python3 system-analysis-tools/invariants/invkit …`. |
| `git` | `impact`; drift in `check`; the commit in `pin` | Without git, `check` reports a `drift` note and `pin` records `commit: null`. `impact` fails. |
| `cargo` | `probe` | A full build of the project. On Delos a cold build plus probe run took about 3 minutes. |
| Rust sources | `scan`, `writes`, `refs`, test checks | The lexer handles Rust only. `catalog`, `check` (minus test lookups), `pin`, `impact` and `relate` work on any code base whose citations use the same Markdown form. |

There is nothing to install. Run the tool in place from the project root:

```sh
T=system-analysis-tools/invariants/invkit
$T --help
```

## Quick start

From the project root (`/Users/castilho/Code/Personal/delos`). The output
below is real, from commit `197e30c`, trimmed.

```sh
T=system-analysis-tools/invariants/invkit
C=system-models/invariants
```

**Check the catalogue against the code.**

```console
$ $T check $C
51 invariants in 6 areas; status {'holds': 26, 'qualified': 22, 'violated': 2, 'inherited': 1}
citations: 183 anchored (141 by identifier, 42 by wording), 83 unanchored, 26 unchecked (69% of checkable)
test references: 34
since 197e30c: 0 changed file(s) in src/ and tests/, 0 of them cited
0 error(s), 1 warning(s), 133 note(s)
  warn  missing-section  05-concurrency-and-ui.md:214: J0 has no statement section
```

**Ask who writes a field and who uses a function.**

```console
$ $T writes inputs
writes to .inputs: 7 site(s) in 5 item(s)
  Builder::fill  [src/expr.rs]
      624  self.g.node_mut(id).unwrap().inputs = vec![layers.first().copied().flatten(), …
      636  self.g.node_mut(id).unwrap().inputs[port] = input;
  Graph::connect  [src/graph.rs]
      262  n.inputs[port] = Some(from);
  Graph::disconnect  [src/graph.rs]
      269  n.inputs[port] = None;
  Graph::insert_resize  [src/graph.rs]
      372  self.nodes.get_mut(&r).unwrap().inputs[0] = Some(src);
      373  self.nodes.get_mut(&node).unwrap().inputs[port] = Some(r.into());
  Graph::set_stack_layers  [src/graph.rs]
      489  n.inputs = vec![front, back];

$ $T refs Graph::depends_on
uses of Graph::depends_on: 3 site(s) in 3 item(s)
  Graph::connect  [src/graph.rs]
      255  if self.depends_on(from.node, to) {
  connect_port  [src/ui/graph_view.rs]
      121  if g.depends_on(from.node, head) {
  NodePanel  [src/ui/inspector.rs]
       79  .filter(|n| n.id != id && n.kind != ids::OUTPUT && !graph.depends_on(n.id, id) && …
```

**Produce a reading list for a new catalogue.**

```console
$ mkdir -p /tmp/inv
$ $T scan --md /tmp/inv/scan.md -o /tmp/inv/scan.json
984 candidates in 31 files: {'claim': 76, 'limit': 28, 'guard': 232, …}
wrote /tmp/inv/scan.md
wrote /tmp/inv/scan.json
$ head -30 /tmp/inv/scan.md
…
| # | item | file | evidence | lines |
|---|---|---|---|---|
| 1 | `ImageView` | src/ui/image_view.rs | field-write×12, normalise×11, guard×6, key×5, snapshot×3, order, borrow, panic | 45, 58, … |
| 2 | `Worker::export` | src/worker.rs | guard×4, order×3, snapshot×2, durable×2, panic, borrow | 281, 282, … |
```

**Confirm the violations still reproduce** (builds the project; see the
[probe workflow](#confirm-violations-with-probe) first).

```console
$ $T probe $C
$ cargo test --test zz_invkit_invariant_probes -- --test-threads=1
confirmed            structural_hash_distinguishes_images_after_undo  (FAILED, expected FAILED)
    assertion `left != right` failed: a red and a blue image have the same structural hash
      left: 2102092581140937527
     right: 2102092581140937527
confirmed            add_node_after_a_split_keeps_each_consumer_on_its_output  (FAILED, expected FAILED)
    the right-hand consumer now reads the split's LEFT output
all probes behave as the catalogue says
```

## Typical workflows

### Build a catalogue

1. **Scan** and read the whole Markdown report:
   `$T scan --md /tmp/inv/scan.md -o /tmp/inv/scan.json`.
   Read the top hotspots in full, then the identity report, the public-field
   writes, the trait contracts and the "conversions that fill in a constant"
   section. On Delos these sections point at both findings: `Graph.next_id`
   is listed as a per-value counter, and `impl From<NodeId> for Link` is
   listed as filling `output: 0`.
2. **Read the source.** The scan is a reading order, not a substitute.
3. **For each invariant**, gather evidence:
   - Maintained by: `$T writes FIELD` (add `--init` for struct literals).
   - Depended on by: `$T refs NAME`.
   - Tests: search `tests/` and inline `mod tests`.
4. **Write a probe** for each suspected violation, in `probes/`, and run
   `$T probe $C`. A failing probe confirms it. Write the finding.
5. **Write the README**, including the counts `check` verifies and the
   analysed commit.
6. **Check until clean**: `$T check $C` has 0 errors. Then read every
   `unanchored` note in `$T check $C -v`.
7. **Pin** at the analysed commit: `$T pin $C -o $C/pins.json`.

[AGENTS.md](AGENTS.md) has this procedure in full detail.

### Maintain a catalogue after code changes

The Delos catalogue does not ship a pins file yet. Create one at the
analysed commit first (`$T pin $C -o $C/pins.json` while that commit is
checked out). Then, after the code changes:

```sh
$T check $C --pins $C/pins.json
```

Example, after inserting one line at the top of `src/graph.rs` and editing
line 255 (run on a scratch clone):

```text
0 error(s), 2 warning(s), 133 note(s)
  warn  missing-section  05-concurrency-and-ui.md:214: J0 has no statement section
  warn  drift            README.md: 1 cited file(s) changed since 197e30c: src/graph.rs; re-check their citations
pins: 238 same, 29 moved, 1 changed, 0 missing
  moved    G1   01-document-model.md:24 graph.rs:103-107 → now at 104-108
  …
  changed  G2   01-document-model.md:46 graph.rs:255
```

- For each `moved` pin, update the line numbers in the catalogue.
- For each `changed` pin, re-read the invariant against the new code and
  update its text or status.
- Update the commit in the catalogue README, then re-pin.

The command exits 1 while any pin is not `same`.

### Review a diff with `impact`

`impact` compares the analysed commit with the **working tree** and lists
invariants whose cited lines the diff touches. Run it before committing, or
on a checked-out branch:

```console
$ $T impact $C
comparing with 197e30c (from README.md)
1 invariant(s) cite lines changed since 197e30c:
  G2   [qualified] The graph is acyclic
       via graph.rs:255 (sections: maintained)
Re-read each one: does the change alter its statement's premise, or the mechanism that maintains it?
```

For each invariant listed, ask whether the change alters the statement's
premise or the mechanism that maintains it. Both Delos violations were
premise drift: code that was right when written became wrong once a premise
changed. `--slack N` widens the match to hunks within N lines of a citation.

Use `--rev` only with the revision the catalogue's line numbers refer to.
Passing an older or newer revision compares the wrong lines.

### Confirm violations with `probe`

`probe` copies each `probes/<name>.rs` to `tests/zz_invkit_<name>.rs`, runs
`cargo test --test zz_invkit_<name>`, and deletes the copy. The project's
`tests/` directory is left as it was, unless the run is killed or you pass
`--keep`. Check before and after:

```sh
git status --short tests/
$T probe $C
git status --short tests/
```

`probe` holds cargo's build lock. If another cargo command runs in the same
project, one of them waits.

### Use it in CI

```sh
T=system-analysis-tools/invariants/invkit
C=system-models/invariants
python3 $T check $C                          # exit 1 on catalogue errors
python3 $T check $C --pins $C/pins.json      # exit 1 when cited code moved or changed
python3 $T probe $C                          # exit 1 when a probe disagrees with findings.md
python3 $T impact $C --json -o impact.json   # always exit 0; publish for reviewers
```

`check` without `--pins` fails only on broken references, missing tests and
wrong counts. Adding `--pins` fails on any edit to cited code, including a
line shift, so it fits a scheduled job better than a per-commit gate.
Checkouts must include history back to the analysed commit for drift and
`impact` (for example `fetch-depth: 0` on GitHub Actions).

### Calibrate the scanner

Once a catalogue exists, measure how well `scan` would have pointed at it:

```console
$ $T recall /tmp/inv/scan.json $C
citations near a candidate: 187/256
invariants with at least one: 51/51
hotspot precision (share of top-N items the catalogue cites): @25 92%, @50 74%, @100 63%
161 cited items; 984 candidates
```

`recall` re-reads the cited source files from `--root` (default `.`), like
every catalogue command, so it works from any directory given the right
`--root`. The scan file's own `root` field is not used.

## Interpreting results

### `check`

- **Errors** are mechanical faults: a link to a missing file or line, link
  text that disagrees with its href, a test that does not exist, a broken
  cross-reference, README counts that disagree with the files, a violated
  invariant with no finding, a finding naming a probe that does not exist.
  Fix all of them.
- **Warnings** are likely problems: drift since the analysed commit, missing
  Statement or Status, an invariant with no citation, a finding without a
  probe. On Delos the one warning (J0 has no Statement) is deliberate.
- **Notes** (`-v`) are prompts. `sparse` says an invariant lacks a section.
  `unanchored` says a citation's clause shares no identifier and no
  distinctive word with the cited lines. Read the code: fix the line, or
  name the identifier in backticks next to the link. Correct paraphrase is
  fine and will stay unanchored.
- The anchoring percentage measures how much of the catalogue's wording
  echoes the code. It is not a correctness rate.

### `scan`

- Read hotspots as a reading order. Most items near the top maintain or
  rely on some invariant; the ranking says nothing about which one.
- The dedicated reports (identity, public-field writes, contracts,
  conversions with constants) matter more than raw candidate counts. Some
  detectors are weak as hotspot evidence but their reports are where
  findings come from.
- Line coverage (`recall`'s "citations near a candidate") is not a quality
  measure. Candidates are dense, so random placement also covers many
  citations. Use hotspot precision instead.

### `impact`

- Listed invariants are those whose *evidence* changed. Re-read each one.
- An empty list means no cited line changed. It does not mean the change is
  safe: new code that writes a field directly (a new edge writer, say) is
  invisible to `impact`. Use `writes`/`refs` to check new writers and users.
- The `sections:` column tells you what changed: a `maintained` citation
  means the mechanism changed; a `depended` citation means a consumer
  changed.

### `relate`

- `N explicit relations` counts distinct (dependent, premise) pairs: a
  pair linked from two sections counts, and is drawn in Mermaid, once.
- "Most relied on" ranks by transitive dependents. Treat it as a hint when
  choosing roots. On Delos it ranks I1, G5 and B5 highest, while the README
  argues for G8, R1/R2, R11, G2, G5 and B1.

### `probe`

- `confirmed` is the expected state while a violation exists.
- `no-longer-violated` after a fix: update the status and the finding, and
  consider moving the probe into `tests/` as a regression test.
- `error` with `build-failed`: the probe does not compile against the
  current code. The last 25 lines of cargo output are shown in JSON.

## Limitations and accuracy

- **Regex-based Rust reading.** The lexer masks comments and strings and
  matches braces. It has no types, no macro expansion and no `cfg`
  evaluation. Code generated by macros or build scripts is invisible.
- **`writes` and `refs` match names, not types.** `writes inputs` finds
  every field called `inputs`. `refs Type::name` drops only the uses that
  certainly belong to another type (`Other::name`, and `self.name` or
  `Self::name` inside `impl Other`); a method call or field access on any
  other receiver (`x.name`) is kept, because the receiver's type is
  unknown. Check the owner of each hit.
- **Anchoring is a heuristic.** It can confirm a citation that points at the
  wrong place if the words happen to match, and it cannot confirm correct
  paraphrase. On Delos 69% of checkable citations anchor; the rest were
  read by hand during the original study and were correct. Pins are the
  exact tool.
- **Scan weights are calibrated in-sample.** `scan.WEIGHTS` were set from
  each detector's lift against the Delos catalogue itself, so the Delos
  precision figures (92% of the top 25 hotspots are cited items) are
  optimistic for other code bases. Recalibrate with `recall`.
- **Bold lines become notes.** An unindented line that is not a bullet and
  starts with bold text starts a section; any label other than the
  canonical ones is a note (`note:<slug>`). Bullets that start with bold
  text (`- **Export.** …`) stay in the section above them.
- **`impact` sees only `src/` and `tests/`,** only tracked files, and only
  lines that existed at the analysed commit. Changes to uncited code are
  not reported.
- **`check` drift** compares whole files, so any edit to a cited file
  raises the warning, even far from the cited lines.
- **Pins** compare whitespace-stripped lines. A block that appears twice in
  a file is reported as `moved` to the nearest occurrence, which may be the
  wrong one.

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| Many `missing-test-file` errors, e.g. `J6: tests/project_e2e.rs does not exist` | Run from the wrong directory: test paths resolve against `--root`. | Run from the project root, or pass `--root`. |
| `missing-file … does not exist (outside --root: is the root right?)` | The catalogue's `../../src/…` links resolve outside `--root`. | Pass the right `--root`, or move the catalogue so the relative links resolve. |
| `invkit: error: catalogue directory not found: …` or `invkit: error: PATH: No such file or directory` (exit 1) | The catalogue directory, pins file or scan file path is wrong. | Check the path. |
| `invkit: error: pins file PATH is not valid JSON: …` (or `scan file`) | The file is not JSON written by `pin` or `scan`. | Regenerate it. |
| `invkit: error: unrecognized arguments: --release` | argparse reads a lone dash-prefixed value after `--cargo-args ` as an invkit option. | Use the `=` form with one quoted string: `--cargo-args=--release`, `--cargo-args='--features x'`. |
| `invkit: error: … has no Cargo.toml: probes are Rust integration tests` | `--root` is not a Cargo project. | Run from the project root. |
| `invkit: error: …/tests/zz_invkit_<name>.rs already exists; remove it (left from an interrupted run?) and retry` | A previous `probe` was killed or run with `--keep`. | Delete that file, then rerun. |
| ``invkit: error: `cargo test … --test zz_invkit_<name> …` timed out after N s`` (exit 1) | A probe run took longer than `--timeout` (default 1800 s); a cold build can. The copied file has been removed. | Raise `--timeout`, or build once with `cargo test --no-run` first. |
| `probe` reports `error` and `build-failed` | The probe does not compile against current code. | Run with `--json` to see the cargo output, or `--keep` and run `cargo test --test zz_invkit_<name>` yourself. Delete the file afterwards. |
| ``invkit: error: `git -C ROOT diff -U0 REV -- src tests` failed: fatal: bad revision 'REV'`` (exit 1) | `impact` was given an unknown revision, or the root is not a git work tree, or the analysed commit is not in a shallow clone. | Pass a valid `--rev`, or fetch full history. |
| `info drift README.md: could not compare with commit C` | Same causes, in `check`. | As above. |
| `impact` says `no cited line changed` right after you edited code | The edit touched uncited lines, the file is untracked, or it is outside `src/` and `tests/`. | Expected. Use `writes`/`refs` for new code. |
| `count` error `says Two violated, catalogue has 3: […]` | An invariant's status contains `VIOLATED`, or README's count is out of date. | Fix README or the status. Inherited violations should say "Inherits the … violation". |
| `writes`/`refs` exit 1 with `0 site(s)` | No match in `--dirs` (default `src`). | Check the spelling, or widen `--dirs`. |

## Development

### Module map

| File | Role |
|---|---|
| `invkit` | Launcher: a `uv` script that puts this directory on `sys.path` and calls `invkit_lib.cli.main`. |
| `invkit_lib/cli.py` | Argument parsing, the ten commands, text formatting for `relate`, `impact`, `recall`. |
| `invkit_lib/rustsrc.py` | Rust lexer: masked code, comments, string literals, items (`fn`, `impl`, `trait`, `mod`, `struct`, `enum`) with spans, `#[test]` functions. |
| `invkit_lib/catalog.py` | Catalogue parser: invariants, sections, status classes, citations, test references, cross-references, findings, fragile table, probes. |
| `invkit_lib/check.py` | Consistency checks and citation anchoring. |
| `invkit_lib/pins.py` | `pin`, pin verification, and git-diff impact. |
| `invkit_lib/scan.py` | Detectors, hotspot ranking, identity report, recall scoring. |
| `invkit_lib/report.py` | The Markdown scan report. |
| `invkit_lib/query.py` | `writes` and `refs`. |
| `invkit_lib/probe.py` | Copying, running and judging probe tests. |
| `invkit_lib/relate.py` | Edges between invariants, ranking, Mermaid output, README roots. |
| `tests/test_invkit.py` | Unit tests. They build a small Rust project and catalogue in a temporary directory; no cargo or git needed. |

### Running the tests

From the project root:

```sh
python3 -m unittest discover -s system-analysis-tools/invariants/tests
```

### Extending

- **A new detector**: add a regex and an `add("kind", …)` call in
  `scan.detect`, append the kind to `scan.KINDS`, give it a weight in
  `scan.WEIGHTS`, and, if it deserves its own section, add it to the list in
  `report.scan_markdown`. Measure its value with `recall` against a finished
  catalogue.
- **A new section label**: add it to `catalog.SECTIONS`. Add it to
  `catalog.REQUIRED` or `catalog.EXPECTED` if `check` should ask for it.
- **A new check**: add it to `check.run` with `rep.add(level, kind, where,
  message)`. Only `error` affects the exit code.
- **A new command**: write `cmd_<name>(args)` in `cli.py`, register a
  subparser in `main`, and return a non-zero int for failure.
- Add a test in `tests/test_invkit.py` for each change; the fixtures are
  small strings at the top of the file.

### Design decisions

- **No dependencies, no parser.** A regex lexer that keeps line and column
  positions is enough to attribute lines to `Type::method` and is easy to
  run anywhere. The cost is the blind spots listed above.
- **Markdown is the source of truth.** The catalogue is written for people.
  invkit parses the conventions it already uses rather than requiring a
  separate data file. `catalog` shows what was understood.
- **Heuristic checks are notes; exact checks are errors.** Anchoring only
  produces `info` notes. Errors come from facts: a file, a line, a test or an
  anchor exists or it does not. Pins give the exact version of anchoring.
- **Probes fail by design.** A probe asserts the invariant, so a failing
  probe is evidence of the violation and a passing one signals a fix. Probes
  live in the catalogue, not in `tests/`, so the analysed code stays
  unchanged.
- **Roots are a judgment.** `relate` reports the explicit link graph and
  says it is an aid. An earlier code-usage ranking was tried and not shipped
  because it did not recover the roots either.
