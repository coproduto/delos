# Delos spec ↔ code traceability

A model of Delos built as a **correspondence matrix** between `SPEC.md`
and the code. Every sentence of the spec is quoted by exactly one or more
*clause entries*; each entry says how far the code implements it, cites
the code that does (`file:line` links), and cites the tests that check it.
The thesis being tested is that such a matrix is the best way to
understand the structure of a project, and to see where a human has to
make a decision.

The analysis is of commit `197e30c`. Nothing in `src/` or `tests/` was
changed. The tool that checks and maintains the matrix is
[`spectrace`](../../system-analysis-tools/spec-traceability/).

Open [`index.html`](index.html) for an interactive view: the spec with
each sentence coloured by status, the clause list, the clause × file
matrix, and the code → spec index.

## Contents

| File | What |
|---|---|
| [01-product-and-platform.md](01-product-and-platform.md) | The product description; Rust, platforms, GPU and CPU fallback (D1, R1–R4) |
| [02-editing-and-views.md](02-editing-and-views.md) | Basic functionalities 1–5: images as nodes, the DAG, image and graph views, relocating the output, editing upstream, export, display modalities (F1–F5) |
| [03-operators.md](03-operators.md) | The basic operators, splits, generators, size conflicts (O0–O10, S1–S7, N1–N6, Z1) |
| [04-sessions-and-projects.md](04-sessions-and-projects.md) | Quick mode and projects, project structure, sharing and importing, platform status (P1–P15, X1–X3) |
| [05-expressions.md](05-expressions.md) | The Delos expression format (E1–E11) |
| [06-quality.md](06-quality.md) | Regression, project and 16K performance tests (Q1–Q3) |
| [findings.md](findings.md) | **Where code and spec disagree**, ranked, each with the decision it needs |
| [review.md](review.md) | The independent review's findings and what was done with each |
| [tightening.md](tightening.md) | The citation-tightening experiment: method, an honest recall measure, results, precision |
| [unspecified.md](unspecified.md) | The reverse direction: behaviour in the code that no spec sentence asks for (U1–U16) |
| [generated/ancestry.md](generated/ancestry.md) | **Code with no spec ancestry**: unspecified state, uncited branches of cited functions, and unspecified code with the keys and buttons that reach it |
| [generated/](generated/) | Other views produced by `spectrace`: the clause × component grid, the code → spec index, uncited functions, JSON and CSV exports |
| [probes/spec_probes.rs](probes/spec_probes.rs) | Two failing tests that confirm the P15 and P6.2 defects (`spectrace probe` runs them) |
| [pins.json](pins.json) | The exact text behind every citation and quoted sentence at `197e30c`, for drift checks |
| [index.html](index.html) | The interactive viewer (generated) |

## How to read an entry

```markdown
## F3.1.1 · Double-click to relocate and to reset {#f3-1-1}

> Double-clicking a node in the graph view relocates the output to it.      ← spec, verbatim
> …

**Status.** Implemented.                                                    ← see below

**Implemented by.**
- The node's `ondoubleclick` toggles `pinned` … ([graph_view.rs:346-356](../../src/ui/graph_view.rs#L346)).

**Tests.** None (graph-view events are not tested; …).

**Notes.** Double-clicking the *body* of a split relocates to output 0 …
```

IDs follow the spec's own numbering where it has one (F3.1.1 is item
3.1.1), and a letter per section otherwise. Statuses:

| Status | Meaning |
|---|---|
| **Implemented** | The code does what the clause says, and nothing known contradicts it. |
| **Partial** | Some of the clause holds. Part is missing, holds only on some platforms or paths, or a known defect contradicts it in some path. The status line says which. |
| **Diverges** | The code does something other than what the clause says. |
| **Missing** | Nothing implements the clause. |
| **Deferred** | The spec itself postpones the clause ("planned", "a later goal"). |
| **Process** | A requirement on how Delos is developed. The entry lists the tests that exist; it cannot show that the process was followed. |

## Summary

<!-- spectrace:summary -->
| Area | Clauses | Implemented | Partial | Diverges | Missing | Deferred | Process |
|---|---|---|---|---|---|---|---|
| 1 · Product and platform requirements | 5 | 2 | 3 |  |  |  |  |
| 2 · Editing, views and export | 17 | 12 | 5 |  |  |  |  |
| 3 · Operators, splits, generators and sizes | 26 | 21 | 4 |  |  | 1 |  |
| 4 · Sessions and projects | 28 | 24 | 3 |  |  | 1 |  |
| 5 · Delos expressions | 11 | 10 | 1 |  |  |  |  |
| 6 · Quality requirements | 3 |  |  |  |  |  | 3 |
| **Total** | **90** | **69** | **16** | **0** | **0** | **2** | **3** |

Spec coverage: 149 of 149 spec sentences are quoted by a clause entry. 594 code citations into 33 files; 73 distinct tests cited.
<!-- /spectrace:summary -->

No clause is *Missing*: everything the spec asks for exists in some form.
The disagreements are in the *Partial* entries, and they are listed, with
evidence, in [findings.md](findings.md).

## What a human should look at

The matrix turns the spec into 90 checkable statements. Most hold. The
places where a person has to decide something are these, from
[findings.md](findings.md):

1. **Five defects contradict the spec.** Two were confirmed by failing probes in the [invariants model](../invariants/findings.md): "Add node" after a split rewires the consumers of its second output ([S5](03-operators.md#s5)), and the view can keep showing an image the document no longer contains ([F2.1](02-editing-and-views.md#f2-1)). Three are new: a failed project import can leave its unpacked folder behind ([P15](04-sessions-and-projects.md#p15)) and opening a generator-only project can rewrite it ([P6.2](04-sessions-and-projects.md#p6-2)), both confirmed by failing probes in [probes/](probes/); and the size dialog hides a second conflict ([Z1](03-operators.md#z1)), derived from the code. These need fixes, not decisions.
2. **Bounded export memory holds for PNG only** ([F4.2](02-editing-and-views.md#f4-2), [Q3](06-quality.md#q3)). Stream the other encoders, or narrow the spec.
3. **Quick-mode work is lost silently** when the window or tab closes, and on mobile even from the menu ([P7](04-sessions-and-projects.md#p7)). Decide whether "leaving" includes quitting.
4. **Platform gaps the spec has not accepted**: no GPU, threads or background removal on the web; no background removal on mobile ([R2](01-product-and-platform.md#r2), [R3](01-product-and-platform.md#r3), [O5](03-operators.md#o5)).
5. **Choices the code made for the spec**: Resize ignores the filter when downscaling and Downscale's "box" is not a box ([O8](03-operators.md#o8), [O4](03-operators.md#o4)); the default export while cropping is the cropped image ([F4](02-editing-and-views.md#f4)); a second import disconnects the first ([F1.1](02-editing-and-views.md#f1-1)); only the stack has a display modality ([F5](02-editing-and-views.md#f5)); overlay placement uses the first path only ([F3.3.1](02-editing-and-views.md#f3-3-1)).
6. **Spec sentences that cannot be checked as written** ([D-SPEC-WORDING](findings.md#d-spec-wording)), and **behaviour no sentence protects** ([unspecified.md](unspecified.md)): undo depth, memory budgets, preview tuning, and every operator parameter that is now part of the file format.

## Method

1. `spectrace units` split the spec into 151 units: 149 sentences and 2 code blocks (the project layout and the expression example, which are illustrations and are not quoted).
2. For each group of sentences, the code was read in full (all of `src/`, about 10,000 lines) and the implementing code located; `spectrace suggest` ranks candidate items for a sentence and was used as a reading list, not as an answer.
3. Each entry quotes its sentences verbatim, so `spectrace check` can prove that every sentence is accounted for and that no quote has drifted from the spec. Citations name an identifier in backticks next to the link, so the checker can confirm that the identifier occurs at the cited lines (*anchoring*: 0 of 424 checkable citations are unanchored).
4. Statuses and findings are judgment. An independent reviewer re-read every cited range and status against the code; its corrections were applied (see *Review* below).
5. The reverse direction: `spectrace uncited` lists the functions no entry cites. The user-visible ones became [unspecified.md](unspecified.md); the rest are plumbing. Later, `spectrace ancestry` automated this at line level (next section).
6. `spectrace pin` recorded the cited text, and `spectrace html` and the `generated/` views were produced from the Markdown.

## Code with no spec ancestry

Many of the defects cluster around undo, which the spec mentions once, to
say it is not part of a project. [generated/ancestry.md](generated/ancestry.md)
looks for code like that mechanically. It starts from the exact lines each
clause cites under *Implemented by*, follows references from there, and
reports what no clause leads to:

- **Unspecified state.** The edge selection (`App.selected_edge`, 45 clause paths through its writers), the undo history (`History.undo`, `redo`, `last_time`, 44) and the graph's id counter (`Graph.next_id`, 36) lead the list. Every edit records history and every new node takes an id, yet no cited line uses them. The stale-image defect [D-STALE](findings.md#d-stale) lives exactly here: undo restores an older `next_id`, so ids are reused.
- **Uncited branches.** The ⌘Z arm of `on_key`, the wheel and zoom handlers of the image view, and the inspector's rename, input pickers and Delete are the parts of cited functions that no clause covers.
- **Unspecified code, with triggers.** `App::undo` and `App::redo` are reached only from ⌘Z and the toolbar buttons. There are 36 such triggered groups; 27 are in neither [unspecified.md](unspecified.md) nor [findings.md](findings.md), for example edge deletion, toasts, zoom buttons, undo/redo button states, the Inspector shell and the mask-resolution choice. Some of these are plumbing (constants, formatting helpers).

**How well it works** ([tightening.md](tightening.md)). Measured
honestly, the flagged share of the code that [unspecified.md](unspecified.md)
cites is 51%, against 32% for the same files overall: ×1.57 over chance.
A cruder "does it touch anything flagged" measure scores 16 of 16, but
random placement scores 14.7, so that measure says little. Reaching
×1.57 needed the citations tightened first (from ×1.36). A citation says
*where* a clause is implemented, not *which behaviour* of that code the
clause covers, so whole-function citations hide unspecified behaviour.
The tightening exposed the code behind the "Add node after a split"
defect (0/6 → 6/6 lines flagged). It also wrongly cut out about 40
pieces of specified code, which an independent review found and a
repair pass restored.

The unspecified-state list changed least. The undo history leads it before
tightening (50 paths) and stays in the top group after (44). `next_id`
drops from second to sixth, because narrower citations reach fewer
writers.

A practical rule follows for humans reviewing a change. When
`spectrace why` says a line has no spec ancestry, or when a diff adds
state that no clause uses, the spec is silent there and a decision is
being made implicitly.

## Review

An independent reviewer (a separate agent with no access to this
analysis's reasoning) checked every entry's citations, statuses, tests
and notes against the code. It found 33 problems in 90 entries; every one
was re-checked against the code and applied. They are listed in
[review.md](review.md). The most useful were three defects the first pass had
missed (P15, P6.2, Z1) and 13 test citations that did not test what
the entry claimed.

## Assessment of the thesis

What the matrix does well:

- **Completeness, mechanically, in both directions.** Every spec sentence is quoted by an entry, and every function is either cited or listed as unspecified or plumbing. No other model of Delos can say "nothing was skipped" with a checker behind it.
- **Gaps against explicit sentences.** The quick-mode exit, the Resize filter choice, the missing error columns, the default export while cropping and the single display modality were not flagged by the earlier models. Export memory, first-path overlays and CPU-only rendering on the web were noted there as fragile spots or plain facts, but not measured against the spec. All of them are invisible from the code's own structure, because the code is consistent with itself; only the spec shows them as shortfalls.
- **Spec quality.** Writing a status for every sentence exposes sentences that cannot be checked ([D-SPEC-WORDING](findings.md#d-spec-wording)) and decisions the code took on the spec's behalf.
- **Impact in both directions.** `spectrace impact` maps a code diff *and* a spec diff onto entries, so a spec change shows the code to revisit and a code change shows the promises it touches.

What it does not do:

- **It shows *where*, not *why*.** The matrix says which function implements a clause, not the design that makes it work (pull-based, resolution-independent rendering; structural hashing; the per-graph id counter). Those cross-cutting decisions do not belong to any one sentence, so they are spread thinly over many entries.
- **Deep defects come from elsewhere.** The matrix did find three defects (P15, P6.2, Z1), but each is local: one function does not do what one sentence says. The two defects that come from a broken premise (graph-local ids reused across snapshots, a single output assumed by `add_node`) were found by the [invariants model](../invariants/), which reasons about assumptions; the matrix only inherits them through the clauses they violate. An entry can be *Implemented* by code that is correct only while an unstated premise holds.
- **Granularity follows the spec, not the code.** One sentence maps to one handler (F3.1.1) while another maps to the whole GPU backend (R3). Where the spec is detailed (projects, expressions), the matrix is precise; where it is vague (display modalities, collaboration), so is the matrix.
- **Process clauses are not traceable to code.** "Bugs that are fixed must get regression tests" needs the bug history, not the source.
- **Truth is not mechanical.** About 430 citations were written by hand. The checker keeps them consistent with the spec and the code, but the first pass still had 33 problems that only a second reader caught, most of them tests cited for more than they test. A matrix needs a review pass, as code does.

The matrix is the best of the Delos models for one question, "does the
code do what we said it would, and where?", and the natural index from a
product decision to the code it affects. For understanding *why* the code
is shaped as it is, the [architecture overview](../architecture-overview/)
and the [invariants](../invariants/) do more.

## Keeping it current

```sh
T=system-analysis-tools/spec-traceability/spectrace
M=system-models/spec-traceability

$T check $M --pins $M/pins.json      # quotes, coverage, citations, tests, links, summary, and drift since 197e30c
$T impact $M                         # entries a working-tree diff touches (code or SPEC.md)
$T why $M src/graph.rs:243           # which clauses depend on this line?
$T units $M --uncovered              # spec sentences no entry quotes yet
```

After updating entries: `$T summary $M --write`, regenerate the views
(see the tool's README), re-pin, and update the commit above.
