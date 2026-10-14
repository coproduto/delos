# 5 · Concurrency, background work and UI state

The UI thread owns the document (Dioxus signals in `App`,
[ui/state.rs](../../src/ui/state.rs)). Rendering, importing, exporting and saving
run in the background ([worker.rs](../../src/worker.rs)) and report back through
a channel that `pump` applies ([ui/mod.rs:128-242](../../src/ui/mod.rs#L128)).
These invariants keep the two sides consistent without locks on the document.

---

## W1 · Background jobs work on immutable snapshots {#w1}

**Statement.** Every job receives its own `Graph` clone: `PreviewJob.graph`,
`ExportJob.graph` and the `graph` argument of `save_project`
([image_view.rs:84](../../src/ui/image_view.rs#L84), [dialogs.rs:98](../../src/ui/dialogs.rs#L98),
[session.rs:253](../../src/ui/session.rs#L253)). Pixel data is shared through
`Arc<SourceImage>`, which is immutable apart from its `file` record
([J6](06-persistence.md#j6)).

**Depended on by.** Rendering without locks and with no torn reads. An export
keeps rendering the graph as it was when the user clicked Export, even while
editing continues.

**Status.** Holds, and holds structurally: the worker API takes graphs by value.

---

## W2 · The latest preview wins {#w2}

**Statement.**
1. `app.seq` increases by one for every preview request ([image_view.rs:121-124](../../src/ui/image_view.rs#L121)).
2. The worker keeps **at most one** pending job. A new job overwrites the slot ([worker.rs:159-166](../../src/worker.rs#L159)).
3. A running render is cancelled as soon as `latest ≠ seq` ([worker.rs:182](../../src/worker.rs#L182)), at the next node or kernel boundary ([R12](02-geometry-and-rendering.md#r12)).
4. The UI accepts a frame only when it is newer than the one shown, or when it is the final frame replacing the provisional one with the same seq ([ui/mod.rs:135-138](../../src/ui/mod.rs#L135)).
5. Busy state, errors and status messages are applied only for `seq == latest` ([ui/mod.rs:152-176](../../src/ui/mod.rs#L152)).

**Depended on by.** Responsiveness while a slider is dragged: stale renders are
abandoned instead of queued. The frame on screen only ever moves forward.

**Progressive refinement.** When the last full render took more than 80 ms, a
quarter-resolution provisional frame is sent first ([worker.rs:196-207](../../src/worker.rs#L196)).
This is correct only because of [R11](02-geometry-and-rendering.md#r11): both
renders sample the same function.

**Frame transport.** On native platforms the four most recent BMPs are served
by name, `<seq>[p].bmp` ([platform.rs:16-28](../../src/ui/platform.rs#L16)).
A frame URL stays valid for about four frames, which is enough because the
`<img>` loads at once.

**Status.** Holds.

---

## W3 · Autosaves are applied in order; sharing waits for a save in progress {#w3}

**Statement.** Each autosave carries an increasing `seq`. The worker skips a
save whose `seq` is older than the last one written, and a mutex serialises
saving and sharing ([worker.rs:356-390](../../src/worker.rs#L356)). The project
file is rewritten only when its text changes ([worker.rs:368-370](../../src/worker.rs#L368)).

**Caveat.** The `ProjectSaved` handler ignores `seq`, and its comment says "any
result is the latest state" ([ui/mod.rs:225-231](../../src/ui/mod.rs#L225)).
That is not quite true. If save 1 finishes while save 2 is still queued, the
footer shows "All changes saved" until save 2 reports. Nothing is lost; the
status shown is briefly too optimistic.

**Status.** Holds for disk state. The UI state can lag behind.

---

## W4 · Exports use bounded memory (PNG only) and leave nothing behind on failure {#w4}

**Statement.**
- Rendering happens in bands of about 4 Mpx ([formats.rs:231-232](../../src/formats.rs#L231)) on a separate thread. A `sync_channel(1)` keeps **at most one finished band waiting**, so rendering overlaps encoding with a fixed working set ([formats.rs:270-293](../../src/formats.rs#L270)).
- On error or cancellation the partial file is deleted ([worker.rs:304-307](../../src/worker.rs#L304)).
- Bands are exact 1:1 renders. They join without seams because of [R5](02-geometry-and-rendering.md#r5) and [R11](02-geometry-and-rendering.md#r11).

**Limit.** Only the PNG codec streams. `ImageCrate::encode` collects every band
into one full-resolution RGBA8 buffer before encoding ([formats.rs:84-88](../../src/formats.rs#L84)).
That is 1 GiB for a 16K image, plus whatever the encoder copies. The spec's
"exports must run in bounded memory" holds for PNG only. The README says
"PNG streams" but does not spell out the consequence for other formats.

**Tests.** `tests/export.rs` (banded output equals a sequential render; write
errors and cancellation stop the export) and
`tests/large_images.rs::export_16k_streams_in_bands`.

**Status.** Holds for PNG. Other formats need memory proportional to the image.

---

## U1 · Selection and relocation always name existing things {#u1}

**Statement.** `selected` is `None` or names an existing node. `pinned` is
`None` or names an existing output (`has_link`).

**Maintained by.** `fix_selection` after undo and redo ([state.rs:286-300](../../src/ui/state.rs#L286)),
`delete_node` ([state.rs:528-533](../../src/ui/state.rs#L528)), and
`replace_document` ([state.rs:247-260](../../src/ui/state.rs#L247)). As a second
line of defence, `displayed()` filters both again before using them
([state.rs:353-355](../../src/ui/state.rs#L353)), and the inspector checks
`exists` ([inspector.rs:17](../../src/ui/inspector.rs#L17)).

**Caveat.** Because node ids can be reused across snapshots
([G5](01-document-model.md#g5)), "exists" does not mean "is the node the user
selected". After undo and an edit, a selection can point at a different node
that has the same id. This is harmless in practice.

**Tests.** `tests/ui_state.rs` (deleting while the output is relocated).

**Status.** Holds.

---

## U2 · Undo history is a bounded list of whole-graph snapshots, owned by the session {#u2}

**Statement.**
- Each discrete edit pushes a snapshot. A redo stack is cleared by any new edit. At most 60 snapshots are kept ([state.rs:142](../../src/ui/state.rs#L142), [state.rs:179-187](../../src/ui/state.rs#L179)).
- Continuous edits (slider drags, handle drags, node moves) that share a key within one second share a single snapshot (`edit_coalesced`, [state.rs:199-215](../../src/ui/state.rs#L199)).
- History is not part of a project. It is reset by `replace_document`.

**Depended on by.** [G8](01-document-model.md#g8) makes a clone a complete
snapshot, and `Arc<SourceImage>` makes it cheap. The project layer must keep
every image *any* snapshot can bring back restorable, which is
[J6](06-persistence.md#j6). `App::all_images` gathers them for each save
([state.rs:230-243](../../src/ui/state.rs#L230)).

**Status.** Holds. Snapshot ids go backward on undo, which [I1](04-structural-identity.md#i1)
does not account for.

---

## U3 · Document edits go through `App::edit` {#u3}

**Statement.** A change to `app.graph` that the user should be able to undo
happens inside `edit` or `edit_coalesced`. These push history and then run
`after_edit`, which re-detects size conflicts ([state.rs:190-215](../../src/ui/state.rs#L190), [state.rs:304-313](../../src/ui/state.rs#L304)).

**Deliberate exceptions**
- Dragging a node in the graph view first calls a no-op `edit_coalesced("move:<id>")` to take a snapshot, then writes `pos` directly on each pointer move ([graph_view.rs:343](../../src/ui/graph_view.rs#L343), [graph_view.rs:280-286](../../src/ui/graph_view.rs#L280)).
- Auto-sizing the canvas writes parameters without history ([state.rs:337-339](../../src/ui/state.rs#L337)). Matching the window is not an undo step.
- The backend toggle writes the graph without changing it, only to trigger a re-render ([ui/mod.rs:439](../../src/ui/mod.rs#L439)).

**Depended on by.** Undo correctness, conflict detection, autosave (which
depends on `app.graph` and runs after any write), and preview requests.

**Status.** Holds, with the documented exceptions above.

---

## U4 · Signal borrows end before writes {#u4}

**Statement.** No `read()` or `peek()` borrow of a signal may be alive when the
same signal is written. Doing so panics with `AlreadyBorrowed`, which aborts the
desktop app.

**Maintained by.** Convention, with comments where it is subtle: copying values
out before acting ([state.rs:492-495](../../src/ui/state.rs#L492)), explicit
`drop(g)` calls throughout `state.rs`, and binding before editing
([graph_view.rs:142-144](../../src/ui/graph_view.rs#L142)).

**Tests.** `tests/ui_state.rs` exists specifically as a regression suite for
this (see its header comment).

**Status.** Holds as far as the tests cover. It is purely a coding discipline
and cannot be checked statically.

---

## U5 · Upstream controls are drawn through the composed space map {#u5}

**Statement.** When the view is locked (the output is relocated, or "Preview
selection" is off) and an upstream node is selected, its overlays are drawn in
the shown node's space through `space_map(shown, editing)`, composed with the
editing node's own `input_map` when it edits on its input
([state.rs:366-376](../../src/ui/state.rs#L366)). When the editing node does not
feed the shown node, the map is `None` and no controls are drawn
([image_view.rs:159-161](../../src/ui/image_view.rs#L159)), as spec 3.3.2 requires.

**Sub-invariants**
- Overlay parameters are fractions of the editing node's control space: its extent, or its input's extent for `EditView::Input`. The inspector uses the same space for its pixel fields ([inspector.rs:65-71](../../src/ui/inspector.rs#L65)).
- The screen-to-fraction map is **frozen when a drag starts** ([image_view.rs:38-41](../../src/ui/image_view.rs#L38)), because dragging can change the map itself. Dragging a Clip rectangle, for example, changes the size downstream.
- `space_map` follows **the first input path** that reaches the target ([graph.rs:334-340](../../src/graph.rs#L334)). When a node feeds the shown image along two paths with different geometry, the controls are correct for the first path only.

**Depends on.** [R2](02-geometry-and-rendering.md#r2) (maps compose and
invert), and every kind's `input_map` agreeing with what its `render` really
does. Clip and Resize call their own `input_map` from `render`
([basic.rs:350](../../src/nodes/basic.rs#L350), [resize.rs:84](../../src/nodes/resize.rs#L84)),
which keeps the two in step by construction.

**Tests.** `tests/engine.rs::space_map_follows_geometry_upstream`.

**Status.** Holds, apart from the first-path limitation above.

---

## U6 · "Add node" inserts after the selected node, keeping consumers connected {#u6}

**Statement** (inspector tip, and the comment at [state.rs:449-451](../../src/ui/state.rs#L449)):
with a node selected, the new node takes over that node's consumers and is fed by it.

**Implementation.** Every consumer of *any output* of the anchor is reconnected
to the new node, and the new node is fed the anchor's **output 0**
([state.rs:470-476](../../src/ui/state.rs#L470)).

**Status: VIOLATED for nodes with several outputs.** When the anchor is a split,
consumers of `right` (output 1) are silently rewired to `new(left)`, which
changes the picture. Confirmed by a probe test. See [finding 2](findings.md#finding-2).
Split nodes came after this function was written, and its single-output
assumption was not revisited.

---

## J0 · The session state machine {#j0}

```
             import (desktop/mobile)              create
   Fresh ───────────────────────────▶ prompt ───────────────▶ Project(dir)
     ▲   │  import (web: no prompt)      │ quick                 ▲   │
     │   └──────────────────────────▶ Quick ──"New project"──────┘   │
     │                                   │                           │
     └──────────── close (confirm if Quick has work) ◀───────────────┘
                     open project: any state ─▶ Project(dir)
```

**Invariants.** Only `Project` writes to disk ([session.rs:255](../../src/ui/session.rs#L255)).
Imports in `Fresh` either wait in the prompt or are dropped when it is cancelled
(`tests/project_e2e.rs::cancelling_the_prompt_imports_nothing`). Leaving `Quick`
with work in it asks for confirmation (`has_unsaved_work`, [session.rs:243-245](../../src/ui/session.rs#L243)).
The footer always shows the state.

**Status.** Holds. It is covered end to end by `tests/project_e2e.rs`.
