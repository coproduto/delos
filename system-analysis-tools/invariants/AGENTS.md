# invkit: instructions for agents

An **invariant catalogue** models a system through the facts its code
assumes are always true: what is assumed, who relies on it, what keeps it
true, and what breaks if it stops being true. Your job is to make the
catalogue accurate enough that a human can use it to decide things. Which
invariant does a change touch? Which assumptions sit on the extension
points? Where does the code rely on a convention nobody checks? The human
makes the decisions.

This file is the procedure. [README.md](README.md) explains the concepts
and how to read results. [REFERENCE.md](REFERENCE.md) has every option,
exit code and format rule; consult it when in doubt.

## Setup

Run everything from the project root:

```sh
T=system-analysis-tools/invariants/invkit    # needs uv; or: python3 system-analysis-tools/invariants/invkit
C=system-models/invariants                   # the catalogue directory
```

- Data goes to stdout, or to a file with `-o` (`writes` and `refs` have no
  `-o`). Progress goes to stderr.
- Exit codes: `check` returns 1 on any `error` issue or, with `--pins`, any
  pin that is not `same`. `probe` returns 1 when a probe disagrees with the
  catalogue. `writes`/`refs` return 1 when nothing matches. `impact`,
  `scan`, `catalog`, `pin`, `relate` and `recall` return 0 on success.
  Any command returns 1 with a one-line `invkit: error: …` on stderr when a
  file is missing or unreadable, a JSON file is invalid, git fails (e.g. a
  bad `--rev`) or a probe run times out. Usage errors return 2.
- Never modify `src/` or `tests/`. Only `probe` touches `tests/`, and it
  cleans up after itself.

## Procedure: build a catalogue

1. **Scan.** `$T scan --md /tmp/inv/scan.md -o /tmp/inv/scan.json`. Read
   the whole Markdown report. These parts matter most:
   - **Hotspots**: functions holding several kinds of evidence. Read the top
     40 or so in full. On Delos, 92% of the top 25 are items the finished
     catalogue cites.
   - **Identity**: every id counter with its scope, its issuers, the places
     its owner is rebuilt, and every hash or cache key. Answer the printed
     question for each per-value counter. This report leads to Delos's
     finding 1 (`Graph.next_id` is per value; the structural hash keys on
     its ids).
   - **Public fields written outside their type**: each one is an API bypass.
     A field with a rule (acyclic edges, sizes that match) puts that rule on
     every writer listed.
   - **Contracts on implementors**: a trait with many impls is an extension
     point. List what every new implementation must uphold.
   - **Conversions that fill in a constant**, **code embedded in another
     language** (parity pairs), **named limits**, **sentinel values**,
     **file-system effects**: each is usually a hidden assumption. On Delos,
     `impl From<NodeId> for Link` filling `output: 0` leads to finding 2.
2. **Read all the source.** The scan is a reading order, not a substitute
   for reading. Every invariant needs an assumption you can state in one
   sentence and code you can cite.
3. **For each invariant, fill in its fields with the query tools:**
   - *Maintained by*: `$T writes FIELD` lists every write. If the claim is
     "only X writes this", the list must show only X, or the exceptions go
     into the text. On Delos, `$T writes inputs` lists the five edge
     writers (`Builder::fill`, `connect`, `disconnect`, `insert_resize`,
     `set_stack_layers`) behind fragile spot 2. Add `--init` to include
     struct literals.
   - *Depended on by*: `$T refs NAME` lists every use. `refs Type::name`
     drops the uses that certainly belong to another type (`Other::name`,
     and `self.name` / `Self::name` inside `impl Other`), but keeps
     `x.name` on any receiver, since the type of `x` is unknown; check the
     owner of those hits.
   - *Tests*: search `tests/` and the inline `mod tests`. Cite tests as
     `` `tests/x.rs::name` ``. Write "no test" when there is none; that is
     a finding.
   - *Status*: `Holds.`, a conditional ("Holds for PNG only."),
     `**Status: VIOLATED.**`, or "Inherits the [X](…) violation."
4. **Suspect a violation? Write a probe.** A probe is a Rust integration
   test in `$C/probes/` that asserts the invariant. Keep it minimal and use
   only the public API. Then:
   1. `git status --short tests/` and note the result.
   2. `$T probe $C`. It copies each probe to `tests/zz_invkit_<name>.rs`,
      runs `cargo test --test zz_invkit_<name>`, and deletes the copy. A
      cold build can take minutes. If another process holds cargo's lock,
      wait.
   3. `git status --short tests/` again. It must match step 1.
   4. A probe that **fails** (`confirmed`) confirms the violation. Write the
      finding: `**Invariant.**` with a link to the invariant,
      `**Mechanism.**`, `**Confirmed.** Probe `` `test_name` ``, the
      consequences (marked as derived from the code if you did not observe
      them), and repair options.
   - To pass cargo flags, give them as one quoted string in the `=` form:
     `--cargo-args='--features x'` or `--cargo-args=--release`.
5. **Write the overview** (`$C/README.md`). `check` looks for these exact
   phrasings:
   - "N invariants across M areas" (M as a digit or a word).
   - "K invariants turned out to be **violated**" (K as a digit or a word,
     "2" or "Two").
   - The analysed commit as ``commit `abc1234` ``.
   - A table of how each invariant is enforced (type system / API checks /
     by construction / runtime fallback / convention + tests / convention
     only). Every ID in a table row must exist.
   - The roots, in a Mermaid `subgraph Roots … end`, and what the human
     should watch.
6. **Choose roots yourself.** `$T relate $C` shows the explicit link graph,
   but on Delos it does not recover the roots a careful reading picked (it
   ranks I1, G5 and B5 highest; only G5 is a README root). A root is a
   decision that many other invariants follow from (stateless kinds, a
   coordinate convention, determinism). Argue for each root in the README.
7. **Check until clean.** `$T check $C` must report 0 errors. Then run
   `$T check $C -v` and read every `unanchored` note. Each one is a citation
   whose clause shares no identifier or distinctive words with the cited
   lines. Fix the line, or name the identifier in backticks next to the
   link. Paraphrase that is correct is fine. Run `$T catalog $C` if you need
   to see how the parser read a section.
8. **Pin.** With the analysed commit checked out:
   `$T pin $C -o $C/pins.json`. Later checks are then exact.

## The format, briefly

```markdown
# 2 · Geometry and rendering

## R5 · Definite sizes are whole pixels {#r5}

**Statement.** Every `Some(size)` from `NodeKind::size` has integer `w` and `h`.

**Maintained by.** Each implementation rounds its own result. Clip rounds
([basic.rs:335](../../src/nodes/basic.rs#L335)), …

**Depended on by.** Export: `GraphBands` renders … ([formats.rs:230-251](../../src/formats.rs#L230)).

**Tests.** `tests/export.rs::export_matches_sequential_render`.

**If broken.** Exports are resampled slightly, with no error.

**Status.** Holds. Every `size()` implementation follows the convention; nothing checks it.
```

- Area files are named `NN-<area>.md` (digits, then `-`). Other `.md` files
  hold no invariants.
- The heading is `## ID · Title {#id}` with the middle dot `·`. IDs are
  capital letters plus a number, unique across the catalogue. The anchor is
  the lowercase ID.
- Section labels are bold at the start of an unindented line that is not
  a bullet. A bullet that starts with bold text (`- **Export.** …`) stays
  in the section above it. **Do not start an unindented non-bullet line
  with bold text inside a section**: the parser treats it as a new note
  section, and its citations leave the section you meant.
- A link's text names the range (`file.rs:a-b`) and its href points at
  `#La`. Put the identifier the sentence is about in backticks **in the same
  clause** as the link. That is what `check` anchors on.
- Link to another invariant as `[R7](#r7)` in the same file, or as
  `[I1](04-structural-identity.md#i1)` in another one. Links in **Depended
  on by** point the other way in `relate`.
- Inside **Tests.**, a bare `` `snake_case_name` `` after a file reference
  belongs to that file.
- Fenced code blocks are ignored by the parser.

Full rules: [REFERENCE.md › The catalogue format](REFERENCE.md#the-catalogue-format).

## Procedure: keep a catalogue current

- **After code changes:** `$T check $C --pins $C/pins.json`. *Moved*
  citations come with their new line numbers. *Changed* ones mean the
  invariant must be re-read against the new code. Update the text, the
  commit in README, and re-pin. If no pins file exists, check out the
  analysed commit and create one first.
- **Reviewing a diff:** `$T impact $C` compares the working tree with the
  analysed commit (it reads the commit from README). Do not pass a
  different `--rev` unless the catalogue's line numbers refer to it. For
  each invariant listed, ask: *does this change alter the statement's
  premise, or the mechanism that maintains it?* Both violations in the
  Delos catalogue were premise drift. Code that was correct when written
  became wrong once a premise changed: graphs started outliving their ids,
  nodes got several outputs.
  - `impact` sees only changed **cited** lines. For new code, also run
    `$T writes FIELD` on fields with rules (for example `inputs`) to catch
    new writers.
  - Tell the human which invariants a change touches **before** proceeding
    when the change weakens a root or a convention-only invariant.
- **After a fix:** `$T probe $C` reports `no-longer-violated`. Ask the human
  whether to move the probe into `tests/` as a regression test, then update
  the status and the finding.

## Pitfalls

- Line coverage of the scan means nothing on its own. Candidates are dense,
  so even random placement covers many citations. Use the ranking and the
  reports.
- `scan.WEIGHTS` were calibrated on Delos. On another code base, read past
  the top 40, and recalibrate with `$T recall /tmp/inv/scan.json $C` once
  you have a catalogue. Like every catalogue command, `recall` reads the
  source from `--root` (default `.`), not from the directory `scan` ran in.
- The lexer has no types. `writes inputs` matches every field called
  `inputs`, whatever its type. Check the owner in each hit.
- Macros, `cfg`-gated code and generated code are invisible. Say so when an
  invariant depends on them.
- A citation that anchors is not necessarily right, and one that does not
  anchor is not necessarily wrong. Read the code. Pins are the exact check.
- Don't claim UI-level consequences you did not run. Mark them "derived from
  the code", as `findings.md` does.
- `probe` refuses to run if `tests/zz_invkit_<name>.rs` already exists,
  which means a run was killed or used `--keep`. Delete the file and retry.
- Run from the project root. From elsewhere, `check` reports every test
  file as missing.
