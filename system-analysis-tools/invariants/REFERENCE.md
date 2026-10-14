# invkit reference

The exact behaviour of every command, option, exit code and file format.
For concepts and workflows, see [README.md](README.md). For the procedure an
agent follows, see [AGENTS.md](AGENTS.md).

Everything here was checked against `invkit_lib/` and, where possible, by
running the command on the Delos repository at commit `197e30c`.

## Contents

- [Invocation](#invocation)
- [Conventions shared by all commands](#conventions-shared-by-all-commands)
- [Exit codes](#exit-codes)
- [Commands](#commands): [scan](#scan) · [catalog](#catalog) · [check](#check) ·
  [pin](#pin) · [probe](#probe) · [relate](#relate) · [impact](#impact) ·
  [writes](#writes) · [refs](#refs) · [recall](#recall)
- [The catalogue format](#the-catalogue-format)
- [Output formats](#output-formats)
- [Environment variables and configuration](#environment-variables-and-configuration)

## Invocation

```sh
system-analysis-tools/invariants/invkit <command> [options]
python3 system-analysis-tools/invariants/invkit <command> [options]
```

The launcher is a `uv` script with no dependencies. Running it directly needs
`uv` on `PATH`. Running it with `python3` (3.10 or later) works the same way.
`invkit --help` and `invkit <command> --help` print the argparse help.

## Conventions shared by all commands

- **`--root ROOT`** (string, default `.`). Every command takes it. It is the
  project root. Source directories (`--dirs`), test references
  (`tests/x.rs`) and git commands are resolved against it. Run from the
  project root and you never need it.
- **Catalogue citations** are resolved relative to the Markdown file that
  contains them (`../../src/graph.rs` from `system-models/invariants/`), then
  expressed relative to `--root`.
- **stdout** carries the result (text or JSON). **stderr** carries progress
  lines (`wrote PATH`, counts, `comparing with …`, `$ cargo test …`).
- **`-o FILE` / `--output FILE`** (where offered) writes the result to FILE
  instead of stdout. Parent directories are created. A trailing newline is
  added. `wrote FILE` goes to stderr.
- **`--json`** (where offered) replaces the text result with JSON (indent 1,
  UTF-8, not ASCII-escaped). Sets are written as lists.
- Options are not repeatable unless the table says so. Options with
  `nargs` (`--dirs`, `--exclude`) take several values after one flag;
  giving the flag twice keeps only the last list. `--cargo-args` takes one
  shell-quoted string instead (see [probe](#probe)).

## Exit codes

| Command | 0 | 1 | 2 |
|---|---|---|---|
| `scan` | always on success | runtime error | usage error |
| `catalog` | always on success | runtime error (e.g. catalogue directory missing) | usage error |
| `check` | no `error`-level issue, and (with `--pins`) every pin is `same` | at least one `error` issue, or any pin `moved`/`changed`/`missing`; also a runtime error (e.g. pins file missing or not JSON) | usage error |
| `pin` | always on success | runtime error | usage error |
| `probe` | every probe test's verdict is `confirmed` or `holds`, or there are no probes | any other verdict; `--root` has no `Cargo.toml`; a `tests/zz_invkit_*.rs` file already exists; cargo timed out | usage error (including a `--cargo-args` value with unbalanced quotes) |
| `relate` | always on success | runtime error | usage error |
| `impact` | always on success, whether or not invariants are listed | `git diff` failed (bad revision, not a git repository); runtime error | usage error |
| `writes` | at least one site found | no site found; runtime error | usage error |
| `refs` | at least one site found | no site found; runtime error | usage error |
| `recall` | always on success | runtime error (e.g. scan file missing or not JSON) | usage error |

Warnings (`warn`) and notes (`info`) from `check` never change its exit code.

A *runtime error* prints one line, `invkit: error: <message>`, on stderr
(after any progress lines) and exits 1. Covered: a missing catalogue
directory; a missing or unreadable file (`PATH: No such file or
directory`); a pins or scan file that is not valid JSON; a failing git
command (`` `git -C ROOT diff -U0 REV -- src tests` failed: <git's first
stderr line> ``); a probe run past `--timeout` (`` `cargo test …` timed out
after N s ``); and the `probe` refusals listed under [probe](#probe). Other
unexpected failures (bugs) still print a Python traceback.

## Commands

### scan

Find candidate invariant sites in Rust source, rank the items that hold
them, and build the identity report.

```
invkit scan [--root ROOT] [--dirs DIR [DIR ...]] [--exclude [PREFIX ...]]
            [-o OUTPUT] [--md MD] [--top TOP]
```

| Option | Type | Default | What it does |
|---|---|---|---|
| `--root ROOT` | path | `.` | Project root. |
| `--dirs DIR …` | one or more paths | `src` | Directories (relative to root) scanned recursively for `*.rs`. Hidden directories and `target` are skipped. |
| `--exclude [PREFIX …]` | zero or more strings | none | Skip files whose root-relative path starts with any PREFIX (plain string prefix, e.g. `src/web`). |
| `-o, --output OUTPUT` | path | none | Write the scan JSON here. |
| `--md MD` | path | none | Write the Markdown reading list here. |
| `--top TOP` | int | `40` | Number of hotspots in the Markdown report. Does not affect the JSON, which lists all hotspots. |

**Output.** If `--md` is given, the Markdown report goes to MD. The JSON
goes to `-o` if given; if neither `-o` nor `--md` is given, the JSON goes to
stdout. With `--md` and no `-o`, no JSON is written. stderr gets one line:
`N candidates in M files: {kind: count, …}`.

**Reads.** `*.rs` under `--dirs` for candidates. The test list always reads
`src/` and `tests/` under the root, whatever `--dirs` says. Candidates in
files under `tests/` and inside `mod tests` blocks are dropped.

Detector kinds (`kind` field of a candidate):

| Kind | Fires on |
|---|---|
| `claim` | A comment containing must, always, never, only, invariant, guarantee(s/d), assume(s/d)/assuming, ensure(s), at most, at least, exactly, by construction, deliberately, so that, otherwise, cannot, can't, safe(ly), unique, same, equal, before, after, until. `note` lists the words. |
| `panic` | `.unwrap()`, `.expect(`, `panic!`, `unreachable!`, `assert!`/`assert_eq!`/`assert_ne!`, `map[&key]`, `.copy_from_slice(`, `[0]` indexing. |
| `guard` | `return Err/None/false/Ok(())`, `bail!`, `ensure!`, `else { return/continue`, `{ return }`, a bare `return;`, `.ok_or`, a line ending in `?;`. |
| `normalise` | `.clamp/.max/.min(<number>`, `.round/.floor/.ceil/.trunc()`, `clamp_*(`, `.saturating_*`, `.div_ceil(`. |
| `field-write` | A write to a `pub` struct field (`=`, `+=`, `-=`, or `push/insert/remove/retain/clear/extend/truncate/drain/pop/swap/get_mut`) outside an inherent `impl` of a type that declares it. Fields named `id`, `name`, `x`, `y`, `w`, `h` are ignored. `note` is `Owner.field`. |
| `identity` | A struct field named `next_*`, `*counter*`, `*_seq` or `seq` (`note`: `per value: cloned with its owner, restarts when the owner is rebuilt` if the struct derives `Clone`, else `per instance`); a `static` atomic integer (`static atomic: unique for the life of the process`); `next_*`/`*counter* += 1` (`issues ids`). |
| `key` | `.hash(&mut`, `Hasher`, `DefaultHasher`, `hash_*(`, `cache.get(`/`cache.insert(`, `impl Hash`. |
| `limit` | A `const`/`static` with an UPPER_CASE name whose value contains a digit. `note` is the name. |
| `dual` | A string literal spanning 3+ lines that contains `fn name(`, `return`, `@group`, `@compute` or `SELECT`. |
| `order` | Channels, `Mutex`, `RwLock`, `Atomic*`, `spawn*(`, `.lock()`, `Condvar`, `.store(`, `.load(`, `.fetch_*(`; or seq/latest/generation/epoch/version/token/cancel* next to a comparison or `+=`. |
| `durable` | `fs::rename/remove_file/remove_dir_all/create_dir*/write/copy`, `File::create`, `.sync_all(`, `.tmp`, `fsync`, `temp_dir(`. |
| `snapshot` | `.clone()` on a line that also mentions history, undo, redo, snapshot, job, graph, state, prev or push. |
| `borrow` | `drop(x)` in code; `borrow` or `AlreadyBorrowed` in code or comments. |
| `sentinel` | Numbers like `1_000_000`, `== 3u`, `u32::MAX` and other `::MAX`. |
| `contract` | Each method declared in a project `trait`. `note`: `Trait: N impl(s); <doc>`. |
| `default` | An `impl From`/`impl Default` under 30 lines that sets a field to a literal (`0`, `None`, `true`, `false`, a number, `Default::default()`). `note`: `fills field: value, …`. |

Hotspot score for an item = Σ over kinds of `WEIGHTS[kind] × (1 + 0.25 × (min(n, 5) − 1))`,
where n is the number of candidates of that kind in the item. `WEIGHTS` is
in `invkit_lib/scan.py`.

### catalog

Parse a catalogue into JSON. Use it to see exactly what the parser
understood.

```
invkit catalog [--root ROOT] [-o OUTPUT] catalogue
```

| Argument / option | Type | Default | What it does |
|---|---|---|---|
| `catalogue` | path (required) | — | Catalogue directory. |
| `--root ROOT` | path | `.` | Project root. |
| `-o, --output OUTPUT` | path | stdout | Where to write the JSON. |

stderr gets the `counts` object. Output schema:
[catalogue JSON](#catalogue-json-invkitcatalogue1).

### check

Verify a catalogue against the code.

```
invkit check [--root ROOT] [--pins PINS] [--strict] [-v] [--json] [-o OUTPUT] catalogue
```

| Argument / option | Type | Default | What it does |
|---|---|---|---|
| `catalogue` | path (required) | — | Catalogue directory. |
| `--root ROOT` | path | `.` | Project root. |
| `--pins PINS` | path | none | Also compare a pins file (from `pin`) with the current code. Any pin that is not `same` makes the exit code 1. |
| `--strict` | flag | off | Report `sparse` (missing expected sections) as `warn` instead of `info`. Does not change the exit code. |
| `-v, --verbose` | flag | off | Also print `info` issues (notes) in the text output. |
| `--json` | flag | off | Print JSON instead of text. Includes all issues whatever `-v` says. |
| `-o, --output OUTPUT` | path | stdout | Where to write the result. |

**Reads.** Every `*.md` in the catalogue directory and `probes/*.rs`; every
cited file; every cited test file; all `*.rs` under `src/` and `tests/`
(to compute word frequencies for anchoring); git (`rev-parse`, `diff
--name-only`) if README names a commit.

**Text output.** A summary, then one line per issue
(`  <level> <kind> <where>: <message>`), errors first, then warnings, then
notes (only with `-v`). With `--pins`, a `pins:` block follows (see
[pins verification](#pins-verification-output)).

```
51 invariants in 6 areas; status {'holds': 26, 'qualified': 22, 'violated': 2, 'inherited': 1}
citations: 183 anchored (141 by identifier, 42 by wording), 83 unanchored, 26 unchecked (69% of checkable)
test references: 34
since 197e30c: 0 changed file(s) in src/ and tests/, 0 of them cited
0 error(s), 1 warning(s), 133 note(s)
  warn  missing-section  05-concurrency-and-ui.md:214: J0 has no statement section
```

The `since …` line appears only when README names a commit and git works.
"% of checkable" is anchored ÷ (anchored + unanchored).

**Issue kinds.** `where` is `file.md:line` or a file name.

| Kind | Level | Raised when | Message |
|---|---|---|---|
| `duplicate-id` | error | Two invariants share an ID. | `ID defined twice` |
| `anchor` | warn | An invariant's `{#anchor}` is not its ID in lower case. | `ID has anchor #x` |
| `missing-section` | warn | No `statement` or no `status` section. | `ID has no statement section` |
| `sparse` | info (warn with `--strict`) | Any of `maintained`, `depended`, `tests`, `broken` is missing. | `ID lacks: …` |
| `uncited` | warn | The invariant has no code citation. | `ID cites no code` |
| `missing-file` | error | A cited file does not exist. | `TEXT → FILE does not exist` (plus ` (outside --root: is the root right?)` when the path starts with `..`) |
| `bad-line` | error | The cited range is not within the file. | `TEXT: FILE has N lines` |
| `href-mismatch` | error | `#Lnn` differs from the range in the link text, or the file named in the text is not the linked file. | `text says TEXT but link goes to #Lnn` / `text names X but link goes to FILE` |
| `unanchored` | info | Anchoring found nothing (see [anchoring](#anchoring)). | `ID TEXT: nothing from "<clause>" within ±3 lines` |
| `missing-test-file` | error | A cited test file does not exist. | `ID: FILE does not exist` |
| `missing-test` | error | No `#[test]` function of that name in the file. | `ID: no #[test] fn NAME in FILE` |
| `bad-xref` | error | A cross-reference points at a file not in the catalogue, an anchor not defined in that file, or an invariant ID at the wrong place. | `[X] → F is not in the catalogue` / `[X] → #a is not defined in F` / `[X] points at F#a, but X is G#b` |
| `count` | error | README's "N invariants across M areas" or "K invariants turned out to be **violated**" disagrees with the catalogue. | `says N invariants, catalogue has M` etc. |
| `count` | info | README has no "N invariants across M areas" claim. | `no 'N invariants across M areas' claim found` |
| `unknown-id` | error | A README table row, a finding or a fragile row names an undefined ID. | `table mentions X, which is not defined` / `finding N names unknown X` / `fragile row N names unknown X` |
| `unknown-id` | warn | A README line mentions an undefined ID before a `"` (Mermaid node labels). | `diagram mentions X, which is not defined` |
| `unreported` | error | A `violated` invariant is named by no finding. | `ID is VIOLATED but no finding names it` |
| `status` | warn | A finding names an invariant whose status class is not `violated`. | `finding N names ID, whose status is CLASS` |
| `status` | info | A fragile row lists an invariant whose status is plain `holds`. | `fragile row N lists ID, whose status reads as an unqualified 'Holds'` |
| `unconfirmed` | warn | A finding names no probe. | `finding N names no probe` |
| `missing-probe` | error | A finding names a probe test that is not in `probes/*.rs`. | `finding N: probe NAME is not in probes/` |
| `orphan-probe` | warn | A probe test is named by no finding. | `probe NAME is not named by any finding` |
| `drift` | warn | Cited files changed (working tree vs. the README commit, under `src/` and `tests/`). | `N cited file(s) changed since C: …; re-check their citations` |
| `drift` | info | git could not compare with the README commit. | `could not compare with commit C` |

#### Anchoring

For each line citation, `check` takes the *clause* around the link: the text
from the previous link (or sentence break) to the next link. Then:

1. **Names.** Identifiers in backticks in the clause are split on `::`.
   Fragments shorter than 3 characters, pure digits and a stop list (`Some`,
   `self`, `Vec`, `u32`, …) are dropped. A fragment *hits* if it appears as a
   whole word (case-insensitive) within ±3 lines of the cited range, or is
   part of the qualified name (or trait) of an item enclosing the range. The
   state is `name` if a hit is *strong* (in the enclosing name, or the
   identifier occurs in at most 20% of the project's `.rs` files) or if there
   are two or more hits.
2. **Words.** Otherwise, words of 4+ letters (compared by their first five
   letters) and numbers of 2+ digits are taken from the clause, minus a prose
   stop list and minus words found in more than 40% of files. Each word
   present in the window scores 1.0 (in ≤ 15% of files) or 0.5 (≤ 40%); each
   number scores 1.5; each clause word that matches a part of the enclosing
   item's name scores 1.5. A score of 2 or more gives `words`.
3. Otherwise the state is `unanchored`, or `unchecked` if the clause had
   neither names nor usable words. Whole-file links and unreadable files are
   also `unchecked`.

### pin

Record the exact text of every cited line range.

```
invkit pin [--root ROOT] [-o OUTPUT] catalogue
```

| Argument / option | Type | Default | What it does |
|---|---|---|---|
| `catalogue` | path (required) | — | Catalogue directory. |
| `--root ROOT` | path | `.` | Project root. |
| `-o, --output OUTPUT` | path | stdout | Where to write the pins JSON. |

Pins every line citation in every catalogue file, including README.md and
findings.md. Whole-file links and citations of missing files are skipped.
The commit is `git rev-parse --short HEAD` (null if git fails); it is the
checked-out commit, not the README commit. stderr: `pinned N citations at C`.
Format: [pins file](#pins-file-invkitpins1).

### probe

Run the catalogue's probe tests with cargo.

```
invkit probe [--root ROOT] [--cargo-args ARGS] [--timeout TIMEOUT] [--keep]
             [--json] [-o OUTPUT] catalogue
```

| Argument / option | Type | Default | What it does |
|---|---|---|---|
| `catalogue` | path (required) | — | Catalogue directory. |
| `--root ROOT` | path | `.` | Project root. Must contain `Cargo.toml`. |
| `--cargo-args ARGS` | one string, split like a shell command line (`shlex.split`) | none | Extra arguments inserted after `cargo test`. Write it in the `=` form with quotes: `--cargo-args='--features x'`, `--cargo-args=--release`. A value containing a space may also follow the flag (`--cargo-args '--features x'`), but a lone dash-prefixed value there (`--cargo-args --release`) is read by argparse as an option and fails with `unrecognized arguments`. Unbalanced quotes are a usage error (exit 2). |
| `--timeout TIMEOUT` | int (seconds) | `1800` | Timeout for each `cargo test` run. On expiry, the copied file is removed and invkit exits 1 with ``invkit: error: `cargo test …` timed out after N s``. |
| `--keep` | flag | off | Leave the copied probe file in `tests/` (for debugging). Delete it yourself before the next run. |
| `--json` | flag | off | Print JSON instead of text. |
| `-o, --output OUTPUT` | path | stdout | Where to write the result. |

**What it does, per file `probes/<stem>.rs`:**

1. Refuses to start if `ROOT/tests/zz_invkit_<stem>.rs` exists (non-word
   characters in the stem become `_`).
2. Copies the probe to that path.
3. Runs `cargo test [ARGS…] --test zz_invkit_<stem> -- --test-threads=1` in
   ROOT, capturing output. stderr gets `$ <command>`.
4. Removes the copy (unless `--keep`), even if cargo failed or timed out.
5. Parses `test NAME ... ok|FAILED|ignored` lines and panic messages.

A probe test is *expected to fail* if some finding's **Confirmed.** section
names it; every other test in `probes/` is expected to pass.

| Verdict | Outcome | Meaning |
|---|---|---|
| `confirmed` | `FAILED`, expected `FAILED` | The violation still reproduces. |
| `no-longer-violated` | `ok` or `ignored`, expected `FAILED` | The violation is gone, or the probe is wrong. Update the catalogue. |
| `holds` | `ok`, expected `ok` | A probe no finding names passed. |
| `unexpected-failure` | `FAILED` or `ignored`, expected `ok` | A probe no finding names failed. |
| `error` | `missing` | The test did not appear in cargo's output. |
| `error` | `build-failed` | No test result lines at all (usually a compile error). `detail` has the last 25 lines of output. |

Text output: one line per test, `<verdict> <name> (<outcome>, expected <X>)`,
followed by up to 6 lines of `detail` (the panic message) unless the verdict
is `holds`. Last line: `all probes behave as the catalogue says` or
`some probes disagree with the catalogue: update findings.md or the probe`.
With no probes, text output is an empty line. Error messages (exit 1):

```
invkit: error: <root> has no Cargo.toml: probes are Rust integration tests
invkit: error: <root>/tests/zz_invkit_<stem>.rs already exists; remove it (left from an interrupted run?) and retry
invkit: error: `cargo test … --test zz_invkit_<stem> -- --test-threads=1` timed out after <N> s
```

### relate

Relations between invariants, from the catalogue's cross-references.

```
invkit relate [--root ROOT] [--mermaid] [--top TOP] [--json] [-o OUTPUT] catalogue
```

| Argument / option | Type | Default | What it does |
|---|---|---|---|
| `catalogue` | path (required) | — | Catalogue directory. |
| `--root ROOT` | path | `.` | Project root. |
| `--mermaid` | flag | off | Print a Mermaid `graph LR` instead. Takes precedence over `--json`. |
| `--top TOP` | int | `12` | Rows in the "Most relied on" list (text output only). |
| `--json` | flag | off | Print JSON. |
| `-o, --output OUTPUT` | path | stdout | Where to write the result. |

**Edges.** Each link from invariant A to invariant B (by anchor) is an edge.
Its direction comes from the section the link sits in: in **Depended on
by**, B rests on A; in any other section (statement, maintained, depends,
tests, if broken, status, notes) A rests on B. Self-links are ignored. Edges
are `(dependent, premise, section)` triples, so the same pair can appear
once per section (JSON `edges` lists them all); the "N explicit relations"
count counts distinct (dependent, premise) pairs.

**Text output.** The edge count; the top `--top` invariants by number of
transitive dependents (stopping at the first with none); the IDs in README's
Mermaid `subgraph Roots … end`, with any undefined ones; invariants with no
edge. In a cycle an invariant is not counted among its own transitive
dependents.

**Mermaid output.** One `premise --> dependent` line per distinct pair, node labels `ID Title` (title cut to 40 characters), and a
`broken` class on violated and inherited invariants.

### impact

Invariants whose cited lines a diff touches.

```
invkit impact [--root ROOT] [--rev REV] [--slack SLACK] [--json] [-o OUTPUT] catalogue
```

| Argument / option | Type | Default | What it does |
|---|---|---|---|
| `catalogue` | path (required) | — | Catalogue directory. |
| `--root ROOT` | path | `.` | Project root (a git work tree). |
| `--rev REV` | git revision | the commit README names, else `HEAD` | The revision the catalogue's line numbers refer to. It is compared with the **working tree**. |
| `--slack SLACK` | int | `0` | Also count a hunk that ends up to SLACK lines before, or starts up to SLACK lines after, a cited range. |
| `--json` | flag | off | Print JSON. |
| `-o, --output OUTPUT` | path | stdout | Where to write the result. |

**How.** Runs `git diff -U0 REV -- src tests` in ROOT and reads the old-side
line ranges of each hunk (a pure insertion after line a counts as a–a+1).
New files are ignored (they have no old side). A line citation is hit if a
hunk overlaps its range (± slack); a whole-file citation is hit if its file
has any hunk. Only citations inside invariants count, not those in README
or findings. Untracked files are not part of `git diff` and are not seen.

When `--rev` is not given, stderr gets
`comparing with C (from README.md)` or `comparing with HEAD (no commit named in README.md)`.

Text output: `no cited line changed since REV`, or

```
1 invariant(s) cite lines changed since 197e30c:
  G2   [qualified] The graph is acyclic
       via graph.rs:255 (sections: maintained)
Re-read each one: does the change alter its statement's premise, or the mechanism that maintains it?
```

At most 6 citations are listed per invariant in text; JSON has all.

### writes

Every write to a field, by name.

```
invkit writes [--root ROOT] [--dirs DIR [DIR ...]] [--init] [--json] field
```

| Argument / option | Type | Default | What it does |
|---|---|---|---|
| `field` | string (required) | — | Field name, without the dot. |
| `--root ROOT` | path | `.` | Project root. |
| `--dirs DIR …` | one or more paths | `src` | Directories searched. |
| `--init` | flag | off | Also list struct-literal initialisations (`field: value`), marked `(init)`. Lines with `let` or `fn`, the field's own declaration, and function parameters named `field` (anywhere in a signature, including multi-line ones) are skipped. |
| `--json` | flag | off | Print JSON. |

A *write* is `.field =` (not `==`), a compound assignment (`+= -= *= /= |= &=`),
any of those after indexing (`.field[i] =`), a mutating method call
(`push`, `push_back`, `push_front`, `insert`, `remove`, `retain`, `clear`,
`extend`, `truncate`, `drain`, `pop*`, `swap`, `get_mut`, `entry`, `append`,
`sort*`, `dedup*`, `set`), or `&mut x.field`. Matching is on masked code
(comments and strings blanked), by name only: the owning type is not known.
Output goes to stdout only (no `-o`). Text output groups sites by enclosing
item:

```
writes to .inputs: 7 site(s) in 5 item(s)
  Builder::fill  [src/expr.rs]
      624  self.g.node_mut(id).unwrap().inputs = vec![…]
```

### refs

Every use of a name outside its definition.

```
invkit refs [--root ROOT] [--dirs DIR [DIR ...]] [--json] name
```

| Argument / option | Type | Default | What it does |
|---|---|---|---|
| `name` | string (required) | — | `name` or `Type::name`. |
| `--root ROOT` | path | `.` | Project root. |
| `--dirs DIR …` | one or more paths | `src` | Directories searched. |
| `--json` | flag | off | Print JSON. |

Matches the last `::` segment as a whole word in masked code. Lines that
define it (`fn|struct|enum|trait|const|static|type|mod NAME`) are skipped.
With a `Type::` prefix, a line is dropped when every use on it certainly
belongs to another type: a path `Other::name` (`Other` not `Type` or
`Self`), or `self.name` / `Self::name` inside an `impl Other` block (a
trait's default methods are kept). Uses whose owner the lexer cannot know
are kept: `x.name` on any other receiver, `Type::name` and bare `name`. So
`refs Graph::new` lists 7 sites on Delos where `refs new` lists 233, while
`refs Graph::depends_on` and `refs depends_on` return the same 3 sites
because every call is a method call. Output goes to stdout only (no `-o`).

### recall

Score a scan against a finished catalogue (calibration).

```
invkit recall [--root ROOT] [--json] [-o OUTPUT] scan catalogue
```

| Argument / option | Type | Default | What it does |
|---|---|---|---|
| `scan` | path (required) | — | Scan JSON from `invkit scan -o`. |
| `catalogue` | path (required) | — | Catalogue directory. |
| `--root ROOT` | path | `.` | Project root for the catalogue. Cited source files are re-read from here, so `recall` works from any directory given the right `--root`. The scan's own `root` field is not used (it records the `--root` given to `scan`, possibly relative). |
| `--json` | flag | off | Print JSON. |
| `-o, --output OUTPUT` | path | stdout | Where to write the result. |

Only invariant citations of `.rs` files under `src/` with a line number
count. A citation is *covered* when a candidate lies within its range ± 2
lines, or a multi-line candidate (`dual`, `default`) spans its start. An
invariant is covered when any of its citations is. *Hotspot precision @N*
is the share of the top N hotspots whose (file, item) contains at least one
cited line.

```
citations near a candidate: 187/256
invariants with at least one: 51/51
hotspot precision (share of top-N items the catalogue cites): @25 92%, @50 74%, @100 63%
161 cited items; 984 candidates

Missed citations:
  G1   graph.rs:103-107             - `Graph::new` creates it ( ).
```

## The catalogue format

A catalogue is one directory. The parser reads only its top level and
`probes/`.

```
<catalogue>/
  README.md        overview and the claims check verifies
  NN-<area>.md     invariants; the file name must start with digits and "-"
  findings.md      findings and the fragile-spots table
  probes/*.rs      Rust integration tests for the violations
  (any other .md)  parsed for links, anchors and findings, but holds no invariants
```

Fenced code blocks (lines starting with ```` ``` ````) are skipped entirely
in every file.

### Area files (`NN-<area>.md`)

| Element | Syntax | Rule |
|---|---|---|
| Area name | `# 1 · The document model` | The first `# ` heading. A leading `N · ` is removed. |
| Invariant heading | `## G1 · Title {#g1}` | Regex `^##\s+([A-Z]+\d+)\s+·\s+(.*?)\s*\{#([\w-]+)\}\s*$`. ID is capital letters then digits. The separator is the middle dot `·` (U+00B7) with spaces. The anchor should be the ID in lower case (else `anchor` warning). |
| End of an invariant | any other `## ` heading | `---` lines do **not** end it. `###` headings are body text. |
| Section label | `**Label.** text` at the start of a line, optionally after `- ` or `* ` | Must not be indented. Matched case-insensitively by prefix, after removing a trailing `.`: see the table below. Text after the label and following lines belong to the section until the next label. |

| Section (canonical) | Labels that start it |
|---|---|
| `statement` | `Statement` |
| `maintained` | `Maintained by`, `Sub-invariants maintained by` |
| `depended` | `Depended on by` |
| `depends` | `Depends on` |
| `tests` | `Tests` |
| `broken` | `If broken` |
| `status` | `Status` (also `Status: VIOLATED`) |
| `note:<slug>` | any other bold label, e.g. `**Caveat.**` → `note:caveat` |

A label matches if it equals the prefix or continues with a space, `:` or
`.`. An unindented line starting with bold text is a label unless it is a
bullet (`- ` or `* `). A bullet such as `- **Export.** …` under **Depended
on by** stays in `depended`, with its citations and links.

**Status class** (from the status label and text):

| Class | Rule (applied in order) |
|---|---|
| `violated` | `VIOLATED` appears in the label or the text. |
| `inherited` | The text starts with `Inherits` and later contains `violation`. |
| `holds` | The first sentence is exactly `Holds.` (or `Holds`), and the whole text contains none of: only, until, must, not checked, is not, violat… |
| `qualified` | Anything else (a status with a condition). |

### Code citations

```markdown
([graph.rs:103-107](../../src/graph.rs#L103))
```

- A link is a code citation if its href starts with `../` and the path ends
  in `.rs`, `.md`, `.toml` or `.wgsl`. Links starting with `http` are
  ignored.
- The line range comes from the link text (`:a` or `:a-b` at the end),
  otherwise from the href (`#La`, `#La-Lb`, `#La-b`). With neither, the
  citation is a whole-file citation.
- `check` requires the `#L` line to equal the text's first line, and the
  file named in the text (`graph.rs`) to be a suffix of the linked path.
- Put the identifier the sentence is about in backticks in the same clause
  as the link; anchoring uses it.
- Citations outside any invariant (README, findings, intro text) are
  checked and pinned but not attributed to an invariant.

### Cross-references

A link is a cross-reference if its href starts with `#` or names a `.md`
file without starting with `../`: `[R7](#r7)`, `[I1](04-structural-identity.md#i1)`,
`[findings.md](findings.md)`. The target file must be in the catalogue, the
anchor must exist in it (explicit `{#id}` anchors, or the GitHub-style slug
of any `#`/`##` heading), and a link whose text is an invariant ID must point
at that invariant's file and anchor.

### Test references

- In backticks: `` `tests/engine.rs::cycles_are_rejected` `` (regex
  `^((?:tests|src)/[\w/]+\.rs)(?:::(\w+))?$`).
- A reference with `::name` counts in any section of an invariant. A bare
  file (`` `tests/ui_state.rs` ``) counts only in the **Tests** section and
  only under `tests/`.
- In the **Tests** section, a bare `` `snake_case_name` `` containing `_`
  after a file reference refers to that file. The remembered file resets at
  the next label.
- `check` requires the file to exist and to contain a function with a
  `#[test]` (or `#[path::test]`) attribute of that name.

### README.md

| Claim | Pattern `check` looks for | If absent |
|---|---|---|
| Invariant and area counts | `N invariants across M areas` (M as a digit or a word one–ten) | `info` note |
| Violation count | `K invariants turned out to be **violated**` (K as a digit, or a word one–ten) | not checked |
| Analysed commit | ``commit `abc1234` `` (7–40 hex digits) | no drift check; `impact` uses `HEAD` |
| Roots (for `relate`) | a Mermaid `subgraph Roots … end` | no "README roots" line |
| Tables | every `[A-Z]\d+` in a table row, with ranges like `J1–J5` expanded, must be a defined ID | — |
| Diagram labels | IDs before a `"` on the same line should be defined | — |

### findings.md

| Element | Syntax |
|---|---|
| Finding heading | `## Finding 1 · Title {#finding-1}` |
| Invariant(s) | `**Invariant.** [I1](04-structural-identity.md#i1) …`: link texts of the form `[A-Z]+\d+` in this section. |
| Probe(s) | `**Confirmed.** Probe `` `test_name` `` …`: each `Probe`/`Probes` followed by a backticked name. |
| Other sections | Any other bold label (`**Mechanism.**`, `**Repair options**`); kept as slugs. |
| Fragile-spots table | Rows `\| N \| first cell \| … \|`. Invariants in the first cell are link texts and backticked `UPPER_CASE` names (3+ characters). |

### probes/

Each `probes/*.rs` file is a Rust integration test. Test functions are those
with a `#[test]`-like attribute. A probe asserts that a violated invariant
holds, so it fails while the violation exists.

## Output formats

### Catalogue JSON (`invkit.catalogue/1`)

| Field | Type | Contents |
|---|---|---|
| `format` | string | `"invkit.catalogue/1"` |
| `dir` | string | Absolute catalogue path. |
| `counts` | object | `invariants`, `areas`, `by_status` (class → count), `citations` (invariant citations only). |
| `invariants[]` | array | `id`, `title`, `anchor`, `area`, `md`, `line`, `sections` (canonical name → text), `status`, `status_label`, `status_class`, `citations[]`, `tests[]`, `xrefs[]`. |
| `invariants[].citations[]` | array | `file` (root-relative), `start`, `end` (null for whole-file), `text`, `href`, `md`, `md_line`, `section`, `context` (the bullet or paragraph), `names` (backticked identifiers in the clause), `local` (the clause), `block_names` (identifiers in the whole bullet). |
| `invariants[].tests[]` | array | `file`, `name` (null for a bare file), `md`, `md_line`. |
| `invariants[].xrefs[]` | array | `target` (anchor), `md_target` (file, `""` = same file), `text`, `md`, `md_line`, `href`. |
| `findings[]` | array | `number`, `title`, `anchor`, `line`, `invariants`, `probes`, `sections`. |
| `fragile[]` | array | `n`, `invariants`, `cells`. |
| `probes` | object | `probes/file.rs` → list of test names. |

### Pins file (`invkit.pins/1`)

```json
{"format": "invkit.pins/1", "commit": "197e30c", "pins": [
  {"invariant": "G1", "md": "01-document-model.md", "md_line": 24,
   "cite": "graph.rs:103-107", "file": "src/graph.rs", "start": 103, "end": 107,
   "lines": ["    pub fn new() -> Self {", "…"], "sha": "1be3eb399ea4"}]}
```

| Field | Meaning |
|---|---|
| `commit` | `git rev-parse --short HEAD` when pinned, or null. Informational. |
| `invariant` | Owning ID, or `""` for citations in README or findings. |
| `md`, `md_line`, `cite` | Where the citation is and its link text. |
| `file`, `start`, `end` | The cited range. |
| `lines` | The cited lines, verbatim. |
| `sha` | First 12 hex digits of SHA-1 over the whitespace-stripped lines. Not used by `check`. |

#### Pins verification output

`check --pins` compares each pin with the current file, ignoring leading and
trailing whitespace on each line. It uses only the pins file, not the
current catalogue text.

| State | Meaning | Extra fields (JSON) |
|---|---|---|
| `same` | The lines at `start` are unchanged. | — |
| `moved` | The same block exists elsewhere in the file. `now` is the occurrence nearest the old start. | `now` (`"a"` or `"a-b"`), `candidates` (first 5 start lines) |
| `changed` | The block is not found anywhere in the file. | — |
| `missing` | The file cannot be read. | — |

Text: `pins: N same, N moved, N changed, N missing`, then one line per
non-`same` pin: `  <state> <ID or -> <md>:<line> <cite>[ → now at a-b]`.

### check JSON

```json
{"stats": {...}, "issues": [...], "anchoring": [...], "pins": null}
```

| Field | Contents |
|---|---|
| `stats.citations` | `{name, words, unanchored, unchecked}` counts over all citations (invariants, README, findings). |
| `stats.test_refs` | Number of test references. |
| `stats.commit` | README commit, if named. |
| `stats.drift` | `{head, changed, cited_changed}`, if git compared. |
| `stats.invariants`, `stats.areas`, `stats.status` | Counts; `status` maps class → count. |
| `issues[]` | `{level, kind, where, message}`; `level` is `error`, `warn` or `info`. |
| `anchoring[]` | `{invariant, where, cite, file, start, end, state, names, matched}` per citation (`invariant` is `""` outside invariants). |
| `pins` | null, or the list of pin rows: `{invariant, md, md_line, cite, file, start, end, state[, now, candidates]}`. |

### Scan JSON (`invkit.scan/1`)

| Field | Contents |
|---|---|
| `format` | `"invkit.scan/1"` |
| `root` | The `--root` value as given (usually `"."`). |
| `files` | Scanned files. |
| `counts` | kind → number of candidates. |
| `candidates[]` | `{kind, file, line, item, text, note, end}`. `item` is the enclosing `Type::fn` (or type/mod), `""` at top level; `end` is set for `dual` and `default`. |
| `hotspots[]` | `{file, item, kinds, score, lines}`, sorted by score; `item` is `(top level)` when empty. |
| `identity` | `sources[]` (identity candidates), `issuers` (issuing fn → `"file:line item"` call sites), `rebuilt` (per-value counter owner → `"file:line item"` sites calling `Owner::new(`/`Owner::default(`), `keys[]` (key candidates), `question` (text). |
| `tests[]` | `{file, name, line}` for every `#[test]` in `src/` and `tests/`. |

The Markdown report (`--md`) has: a count table, `Hotspots (top N)`,
`Identity`, `Public fields written outside their type`, `Contracts on
implementors`, then (when non-empty) `Conversions that fill in a constant`,
`Code embedded in another language`, `Named limits`, `Sentinel values`,
`File-system effects`, `Snapshots`, `Borrow discipline`, `Rules stated in
comments`, and `Tests` (count per file). Guard, panic, normalise and order
candidates appear only in hotspots and JSON.

### probe JSON

```json
{"probes": [{"file": "probes/invariant_probes.rs", "name": "...", "outcome": "FAILED",
             "expected": "FAILED", "verdict": "confirmed", "detail": "..."}], "ok": true}
```

`build-failed` rows have `name: null` and no `expected`. With no probes:
`{"probes": [], "summary": "no probes"}`.

### relate JSON

| Field | Contents |
|---|---|
| `edges` | `[dependent, premise, section]` triples. |
| `ranking[]` | `{id, title, status, direct, transitive}`, sorted by number of transitive then direct dependents. |
| `readme_roots` | IDs in README's `subgraph Roots`. |

### impact JSON

A list (empty if nothing is hit) of
`{id, title, status, md, hits[]}`, where each hit is
`{cite, section, hunk: "a-b"}` or `{cite, section, whole_file: true}`.
Only the first hunk that touches a citation is recorded.

### writes / refs JSON

A list of `{file, line, item, kind, text}` (`writes`; `kind` is `write` or
`init`) or `{file, line, item, text}` (`refs`). `item` is `""` at top level.

### recall JSON

| Field | Contents |
|---|---|
| `citations` | `{covered, total}` |
| `invariants` | `{covered, total}`; total counts invariants with at least one `src/` citation. |
| `per_invariant` | ID → `[covered, total]` |
| `missed[]` | `{invariant, cite, file, start, end, context}` |
| `kinds` | kind → number of covered citations it was near. |
| `candidates` | Number of candidates considered. |
| `hotspot_precision` | `{"25": p, "50": p, "100": p}` as fractions. |

## Environment variables and configuration

invkit reads no environment variables and no configuration file. Behaviour
is set only by command-line options. The constants that tune heuristics
live in the source: `scan.WEIGHTS`, `check.SLACK` (3), `check.STOP`,
`check.PROSE_STOP`, `catalog.SECTIONS`, `catalog.REQUIRED`,
`catalog.EXPECTED`, `probe.PREFIX` (`zz_invkit_`).

`probe` runs cargo with the inherited environment, so cargo's own variables
(`CARGO_TARGET_DIR`, `RUSTFLAGS`, …) apply. `impact`, `check` and `pin` run
`git` with the inherited environment.
