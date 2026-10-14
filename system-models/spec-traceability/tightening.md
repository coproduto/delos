# Tightening the citations: an experiment

`spectrace ancestry` finds code that no spec clause leads to. A first
calibration suggested that its blind spots were *over-broad citations*: a
clause entry that cites a whole function marks all of that function's
behaviour as specified. This file records what happened when the
citations were tightened, and how the result was measured.

## Method

1. **Baseline.** Ancestry was measured on the matrix as first written.
2. **Rules fixed in advance.** An *Implemented by* citation covers only the lines that implement the entry's quoted sentences, split if they are not contiguous (rule 1). Behaviour the quoted sentences do not ask for moves to **Notes**, which are not seeds (rule 2).
3. **Blind application.** Four agents each tightened some of the six clause files. They saw the rules and the code, but not [unspecified.md](unspecified.md), [findings.md](findings.md), the README or the ancestry report, so they could not aim at the calibration set.
4. **Precision review.** A fifth agent, given only SPEC.md and the list of newly flagged items, classified each as **A** (behaviour the spec does not ask for), **B** (plumbing) or **C** (specified behaviour that tightening wrongly cut out).
5. **Repair.** A sixth agent restored the C items under *Implemented by*, again without seeing the calibration set. It declined 9 of them with reasons: 6 it judged to be extras (for example the layer-stack ▲▼ buttons and the extra blend modes), and 3 whose lines did not implement the claimed sentence.
6. **Correction after measuring (disclosed).** Four *Implemented by* bullets cited the *evidence of a shortfall*, not an implementation. One was the "Add node after a split" defect itself; the others were the PNG-only streaming, the missing cleanup after a failed import, and the CPU-only steps. They broke rule 1, but the agents had kept them, so I moved them to Notes after seeing the first measurement.

## How to measure recall honestly

The first calibration counted an item in [unspecified.md](unspecified.md)
as *found* when any of its cited lines touched flagged code. That measure
is useless. Moving every citation to a random place in the same file
scores **12.8 of 16** on the original matrix, against a real **10 of 16**.
The flagged code is dense in UI files, so almost any range touches it.

The measure used here is **enrichment**: the share of the hand-written
items' cited code lines that ancestry flags, divided by the flagged share
of all code lines in the same files. 1.0 is chance.
`spectrace ancestry M --recall U` reports both measures, and labels the
overlap measure as lax.

## Results

| | Before | After tightening and repair |
|---|---|---|
| *Implemented by* citations | 344 (6,627 lines) | 447 (4,522 lines) |
| … of which longer than 12 lines | 159 | 91 |
| Citations in Notes | 8 | 69 |
| Overlap "recall" (lax) | 10/16 | 16/16 |
| … with citations placed at random | 12.8/16 | 14.7/16 |
| **Enrichment** | 33% vs 24%: **×1.36** | 51% vs 32%: **×1.57** |
| Unspecified functions (lines) | 122 (589) | 162 (713) |
| Uncited spans (code lines) | 46 (780) | 98 (1,123) |
| Unspecified state fields | 9 | 20 |
| Triggered unspecified groups | 23 | 36 |

Per hand-written item, the flagged share of its cited code lines changed
as follows. The large gains are the items the hypothesis named:

| Item | Before | After |
|---|---|---|
| U3 `add_node`/`delete_node` rewiring | 0/51 | 38/51 |
| U15 layer-stack extras | 0/44 | 39/44 |
| U13 run-time backend toggle | 0/15 | 12/15 |
| U9 "keep both sizes" | 0/5 | 4/5 |
| U5 preview tuning | 0/106 | 23/106 |
| U8 extra operator parameters | 0/175 | 4/175 |

U8 barely moves, because its citations cover whole parameter lists in
which most parameters are specified.

**Defects.** The "Add node after a split" defect's lines went from 0/6 to
**6/6 flagged**, and the stale-image defect is flagged through
`Graph.next_id` both before and after. The other findings are not
flagged, and should not be: they are specified behaviour done wrongly
(a missing cleanup, an encoder that buffers), and the clause entries
catch those, not ancestry.

**Precision of what tightening added.** Of the 125 items flagged only
after the blind pass, the reviewer judged:

| | A: unspecified | B: plumbing | C: specified, wrongly cut |
|---|---|---|---|
| Functions | 8 | 29 | 6 |
| State fields | 11 | 0 | 0 |
| Spans | 20 | 16 | 35 |
| **Total** | **39 (31%)** | **45 (36%)** | **41 (33%)** |

So the blind pass also produced about 40 under-citations: it cut out
cancellation checks, the GPU half of the blend kernel, `unpack`'s
extraction loop and more. The repair restored most of them. The 9 it
declined were the reviewer and the repairer disagreeing about what
counts as an extra. That boundary is a judgment, and the two independent
judges drew it differently in about a fifth of the cases.

## What this says

- **Tightening works, modestly.** Enrichment rose from ×1.36 to ×1.57, and the items it was meant to rescue moved from fully hidden to mostly flagged, including the code behind a confirmed defect. Most of the raw rise in flagged code is the report getting bigger, not sharper.
- **The state signal is the most stable, but not immune.** Before tightening, the undo history (50 clause paths through its writers) and `Graph.next_id` (49) topped the unspecified-state list. After tightening and repair, the undo history is still near the top (44), but `App.selected_edge` (45) moved above it and `next_id` dropped to sixth (36): narrower citations mean fewer paths reach each writer. The undo machinery stays in the top group either way. The line-level reports (spans, unspecified functions) changed much more.
- **Regenerate before you quote.** An earlier version of this file said the state ranking was identical before and after. That claim came from a report that had silently failed to regenerate (a shell loop that never ran, with its errors suppressed). A peer session caught it.
- **Narrow citations cost accuracy.** A third of what the blind pass newly exposed was specified code it had wrongly cut out. Tightening needs the same independent review as writing entries does. [review.md](review.md) found the same pattern for the first draft.
- **"Extra" is a judgment.** Whether the ▲▼ reorder buttons implement "allow editing the stack" is a reading of the spec. When two careful readers disagree, the spec is underspecified at that point, which is exactly what ancestry is for.
