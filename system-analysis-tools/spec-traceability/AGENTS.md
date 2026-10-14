# spectrace: instructions for agents

A **traceability matrix** models a system as the correspondence between
what its spec promises and the code that keeps each promise. Your job is
to make the matrix accurate enough that a human can make decisions from
it. Which promises does this change touch? Which promises are only partly
kept? What does the code do that nobody asked for? The human makes the
decisions; the matrix makes them visible.

This file is the procedure. Exact options, exit codes and formats are in
[REFERENCE.md](REFERENCE.md). Concepts and examples are in
[README.md](README.md).

## Rules

1. **Never edit `src/`, `tests/` or the spec** while building or checking a matrix. You describe the code; you do not change it. Probes live in `<matrix>/probes/`, not in `tests/`.
2. **Do not overwrite the model's files by accident.** `summary --write` rewrites `<matrix>/README.md`. `-o` overwrites its target. To experiment, write to a scratch directory.
3. **`probe` touches `tests/`.** It copies each probe to `tests/zz_spectrace_<name>.rs` and deletes it afterwards. Run `git status --short tests/` before and after; it must be unchanged. If a run was killed, delete `tests/zz_spectrace_*.rs`.
4. **Report, don't decide.** Statuses and findings state facts with citations. Decisions (fix the code, change the spec, accept the gap) belong to the human. Ask before proceeding when a change weakens an `Implemented` clause.
5. **A clean `check` is necessary, not sufficient.** It proves consistency, not truth.

## Setup

Run everything from the project root.

```sh
T=system-analysis-tools/spec-traceability/spectrace   # or: python3 system-analysis-tools/spec-traceability/spectrace
M=system-models/spec-traceability                     # the matrix directory
```

- Global options go **before** the command: `$T --root R --spec S check $M`. `--root` defaults to `.`, `--spec` to `SPEC.md` (relative to the root).
- The matrix path is relative to the current directory, not to `--root`.
- Data goes to stdout (or to `-o FILE`); `wrote FILE` and progress go to stderr.
- Exit codes: `check` 1 on errors (or warnings with `--strict`); `why` 1 when nothing cites the line (also with `--json`); `probe` 1 unless every probe confirms a claimed finding; argparse usage errors 2; every other error (missing file, bad `--rev`, malformed `why` location, probe timeout) is one line on stderr and exit 1. Piped into `head`, a command stops quietly with exit 141.
- Requirements: Python ≥ 3.10; `git` for `impact`; `cargo` for `probe`.

## Command card

| Goal | Command |
|---|---|
| Sentences the matrix must quote | `$T units` |
| Sentences still unquoted | `$T units $M --uncovered` |
| Where to look for a sentence | `$T suggest "<sentence>"` or `$T suggest --matrix $M --clause ID` |
| Check the matrix | `$T check $M`, then `$T check $M -v` |
| Status table into the README | `$T summary $M --write` |
| Promises depending on a line | `$T why $M src/file.rs:LINE` |
| Entries a diff touches | `$T impact $M` (or `--rev REV`, `--slack N`) |
| Drift since pinning | `$T check $M --pins $M/pins.json` |
| Pin the current state | `$T pin $M -o $M/pins.json` |
| Run probes | `$T probe $M` |
| Code no clause leads to | `$T ancestry $M -o $M/generated/ancestry.md` |
| Views | `grid`, `index`, `uncited`, `export`, `html` (see Procedure F) |

## Procedure A: build a matrix

1. **Set up the directory.** Create `$M/README.md` with the analysed commit and an empty summary block:

   ```markdown
   The analysis is of commit `abc1234`.

   <!-- spectrace:summary -->
   <!-- /spectrace:summary -->
   ```

   Write `commit` (any case) followed by the hash in backticks; `impact`, `pin`, `html` and `export` read it. Also write the status definitions here (see the Delos model's README for a template).

2. **List the units.** `$T units`. Every line is a sentence an entry must quote. Read the whole spec first anyway: entries group sentences by meaning, not by line.

3. **Read all the code.** `suggest` is a reading list, not an answer: `$T suggest "<sentence>"` ranks items by shared words, and is weakest for UI behaviour (it ranks whole components above the handler inside them). You cannot set a status without reading the code that implements the clause *and* the code paths around it.

4. **Write one entry per clause** in files named `NN-<area>.md`, each starting with `# N · Area`. Usually one entry per spec item or short paragraph; split when sentences have different statuses. Template:

   ```markdown
   ## F1.1 · Import creates a Static Image connected to the output {#f1-1}

   > Importing an image to Delos specifically creates a "Static Image" node connected directly to the output.

   **Status.** Implemented.

   **Implemented by.**
   - `finish_import` adds the `ids::IMAGE` node and connects it with `connect` ([mod.rs:251-284](../../src/ui/mod.rs#L251)).

   **Tests.** `tests/project_e2e.rs::first_import_asks_and_creates_a_project` (checks the saved expression).

   **Notes.** Importing a second image detaches the first ([D-IMPORT](findings.md#d-import)).
   ```

   - **Heading.** `## ID · Title {#anchor}` with the middle dot `·`. The anchor is the ID lowercased with `.` → `-`. Follow the spec's numbering where it has one (F3.1.1 is item 3.1.1); otherwise a letter per section plus a number.
   - **Quotes.** Every sentence the entry covers, verbatim, one `>` line each, all **before** the first label. `…` elides text inside a quote. Do not paraphrase: quotes are how drift is detected.
   - **Status.** Start with one of `Implemented`, `Partial`, `Diverges`, `Missing`, `Deferred`, `Process`, then one or two sentences saying *which part* holds when it is not `Implemented`. `Partial` covers a missing part, a platform or path where it does not hold, or a known defect contradicting it.
   - **Implemented by.** One bullet per mechanism. Name the identifier in backticks in the same bullet as its link. Cite the narrowest range that shows the claim. Link form: `[file.rs:START-END](relative/path/file.rs#LSTART)`, path relative to the Markdown file.
   - **Tests.** Only tests that *assert* the claim, as `` `tests/file.rs::name` ``. Open each one. If a test only exercises the code, or only one of several cases, say so ("replace mode only"). Write `None.` when there is none; that is information, not a failure.
   - **Notes.** Edge cases, deliberate deviations, links to findings. Cite defects and edge cases here, not under Implemented by.

5. **Look for the second path.** Most mistakes in the Delos matrix were edge cases of a correct claim: a second failure path (`load` failing after `unpack` succeeded), a second platform (`confirm` is a no-op off desktop), a second caller (an automatic fit that writes to the graph). For every "never", "always", "every" and "nothing" in the spec, find every writer or caller with grep before writing `Implemented`. Use `$T why $M FILE:LINE` to see which other entries already cite a function.

6. **Check until clean.** `$T units $M --uncovered` shows what is left. `$T check $M` must show 0 errors. Then `$T check $M -v` and read every note. An unanchored citation means the backticked identifier is not near the cited lines: fix the range or the identifier. Unquoted spec code blocks are notes; quote them only if they are normative. The message list with fixes is in [REFERENCE.md](REFERENCE.md#check-messages).

7. **Findings.** Every `Partial`, `Diverges` or `Missing` gets an entry in `findings.md` (`## D-NAME · Title {#d-name}`, no quotes) with: **Kind.** (defect, gap, decision, spec issue); **Clauses.** (links); **Observed.** (with citations); **Consequence.** (mark *derived from the code* if you did not observe it); and **Decision.** (the choice it asks of a human).

8. **Confirm defects with probes.** For a defect, write a minimal integration test in `$M/probes/<name>.rs` that asserts what the spec says, using only the crate's public API. Run `$T probe $M` (check `git status --short tests/` before and after). A failing probe confirms the finding. Record it in the finding's **Kind.** line or a **Confirmed.** label as ``**Confirmed.** Probe `test_name` ``; otherwise `probe` reports `no finding cites it` and exits 1.

9. **The reverse direction.** `$T ancestry $M -o $M/generated/ancestry.md`. Read *Unspecified state* first: a field written on many specified paths but used on none is where the spec has a hole the code filled (on Delos: the undo history and the id counter, where the defects were). Then *Unspecified code* groups marked **not noted anywhere** that have triggers. Put user-visible behaviour and resource policies into `unspecified.md` (`## U1 · Title {#u1}`, with **What.**, **Where.**, **Why it matters.**). Leave plumbing out. `$T uncited $M` is the coarser, function-level view. Once `unspecified.md` exists, calibrate with `$T ancestry $M --recall U`. Read the `enrichment` line, not the `overlap` line: overlap is near chance (on Delos, randomly placed citations scored 12.8 of 16 against a real 10 of 16), while enrichment compares the flagged share of the hand-written items' lines with the flagged share of the same files (on Delos ×1.36 before tightening citations, ×1.57 after). Rows with a low `flagged/code` ratio, or `MISSED`, usually sit under an over-broad **Implemented by** citation. Narrow it to the lines that implement the quoted sentences and move the rest to **Notes**, then review the narrowed citation like a new one: a blind tightening pass on Delos wrongly cut about 40 specified ranges, a third of what it newly exposed ([tightening.md](../../system-models/spec-traceability/tightening.md)).

10. **Independent review.** Have someone who did not write the entries (a fresh agent) re-read every citation, status, test and note against the code, and report problems only. Check each reported item against the code yourself before applying it, and keep the list in `review.md`. On Delos this found 33 problems after `check` was clean.

11. **Finish.** `$T summary $M --write`; generate the views (Procedure F); `$T check $M` (0 errors); `$T pin $M -o $M/pins.json`. Pin only when the working tree matches the analysed commit for `src/`, `tests/` and the spec.

## Procedure B: after the code changes

1. `$T impact $M`. It lists entries whose cited lines the working-tree diff (since the README commit) touches, and changed files that no entry cites.
2. For each entry listed, read its quoted sentences and the new code. Does the change still do what they say? If an `Implemented` clause is weakened, tell the human **before** proceeding.
3. `$T check $M --pins $M/pins.json`.
   - `moved: KEY -> A-B` (warning): same text at new lines. Update the link text and `#L` to the new lines.
   - `changed: KEY` (error): the cited text changed. Re-read the entries listed and update range, status and notes.
   - `missing: KEY` (error): the file is gone. Find where the code went.
4. Look at `changed files no entry cites` from step 1: new behaviour may need an entry in `unspecified.md`, or a spec sentence may now be implemented.
5. When the matrix is current: update the README's ``commit `…` ``, `summary --write`, regenerate views, re-pin.

`impact` diffs `src/`, `tests/`, the spec and every other file the matrix cites (for example `Cargo.toml`). A changed file outside `src/` and `tests/` that no entry cites is not listed.

## Procedure C: after the spec changes

1. `$T impact $M` reports entries whose quoted sentences are in changed spec lines (`spec  SPEC.md:LINE “…”`, or the `--spec` path). `(unclaimed)` means a changed sentence no entry quoted.
2. `$T check $M` reports quotes no longer in the spec (`quotes text that is not in the spec`) and new sentences no entry quotes (`no entry quotes`).
3. Update the quotes verbatim. Re-judge each affected status against the code. A new sentence is new work: find the code, or add an entry marked `Missing`.
4. Finish as in Procedure B step 5.

## Procedure D: reviewing a pull request

1. On the PR branch, run `$T impact $M`.
2. For each entry: quote the spec sentences, show the hunk, and say whether the promise still holds, citing lines.
3. List `changed files no entry cites` and say what new behaviour they add.
4. Report to the human: promises touched, promises weakened (with evidence), new unspecified behaviour. Do not edit the matrix as part of a review unless asked.

If the PR's base is later than the README commit, `impact` with no `--rev` also shows changes already on the base branch. To see only the PR, pass `--rev <merge-base>`: citations are carried from the README commit to that revision first (the output says so), and hits read `path:start-end → c-d ∩ a-b`. Treat `→` ranges that widened a lot as a sign the cited code changed in between, and re-check those citations. Citations listed as `cited file gone` point at a deleted or renamed file; fix them in the matrix.

## Procedure E: before editing code, and after a fix

- **Before editing:** `$T why $M FILE:LINE` lists the promises that depend on the line (directly) or on its function (same item). Keep them, or tell the human which ones the change affects.
- **After a fix:** `$T probe $M` should report `no-longer-violated` for the fixed defect (and exit 1, because not every probe confirms). Update the entry's status and the finding. Moving the probe into `tests/` as a regression test is the human's decision.

## Procedure F: regenerate the views

```sh
$T summary $M --write
$T grid $M -o $M/generated/grid.md
$T grid $M --by file --format csv -o $M/generated/grid-by-file.csv
$T index $M -o $M/generated/code-to-spec.md
$T uncited $M -o $M/generated/uncited.md
$T ancestry $M -o $M/generated/ancestry.md
$T export $M -o $M/generated/matrix.json
$T html $M -o $M/index.html        # must be in the matrix directory for code links to resolve
```

`generated/` is not parsed by the tool, so views never affect `check`.

## Pitfalls

- Anchoring proves the identifier is at the cited lines, not that the claim is true. Don't stop at a clean `check`.
- Cite tests for what they assert. "Covered by X" when X only exercises a function was the most common error on Delos (13 of 33).
- Don't infer a status from names or comments. A doc comment saying "never writes" is a claim to verify.
- Don't paraphrase the spec in quotes. Quotes are how drift is detected.
- Don't claim UI-level consequences you did not run. Mark them *derived from the code*, or write a probe.
- Cite narrowly under **Implemented by**. A citation of a whole function marks all of its behaviour as specified, which hides the unspecified parts from `ancestry`. On Delos this hid the `add_node` rewiring rule, the site of a confirmed defect (0 of its 6 lines flagged before narrowing, 6 of 6 after).
- Narrowed citations need review like new ones. Cutting too much hides specified behaviour: on Delos a blind pass cut cancellation checks and the GPU half of the blend kernel out of the clauses that promise them.
- Calibrate `ancestry` on enrichment, not overlap. Overlap counts an item as found when any cited line touches flagged code, which random ranges do too.
- A heading that does not match `## ID · Title {#anchor}` exactly is not an entry, and it silently ends the previous one. If an entry is missing from the counts, check the separator (`·`, not `-`) and the anchor.
- A `>` line after the first label is not a quote.
- Links into other directories (`../invariants/findings.md#…`) are not checked. Check them by hand.
- `uncited` and `index` count citations in Notes and note entries; `ancestry` does not. Use `ancestry` for "what is unspecified".

## Done checklist

- [ ] `$T check $M` exits 0; every note from `-v` has been read.
- [ ] `$T units $M --uncovered` lists only non-normative code blocks.
- [ ] Every `Partial`, `Diverges` and `Missing` entry links to a finding; every finding ends with a decision.
- [ ] Every defect probe confirms and is claimed (`$T probe $M` exits 0); `git status --short tests/` is unchanged.
- [ ] `ancestry` reviewed; user-visible unspecified behaviour is in `unspecified.md`.
- [ ] An independent review was done and recorded in `review.md`.
- [ ] README commit, summary block, views and `pins.json` are current.
