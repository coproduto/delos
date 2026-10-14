# Findings

Tracing each input to its end exposed five defects. Four are **confirmed**
by tests in [probes/](probes/) that fail on commit `197e30c`. One is
derived from the code and was not reproduced in a window. A list of
observations follows: decisions that the traces make visible but that are
not bugs.

Nothing in `src/` or `tests/` was changed.

| # | Defect | Input path | Status |
|---|---|---|---|
| [1](#finding-1) | Clicking a node in the graph view adds an empty undo step and erases redo | graph view press | confirmed (2 probes) |
| [2](#finding-2) | Undo/redo do not update the size-conflict prompt | ⌘Z / toolbar undo | confirmed (2 probes) |
| [3](#finding-3) | Opening a project whose output is size-less rewrites its canvas size and its file, with no undo | project open + canvas effect | confirmed (1 probe) |
| [4](#finding-4) | Dragging a point handle pushes two undo steps per pointer move and wipes the history | image-view point drag | confirmed (2 probes) |
| [5](#finding-5) | Keys typed in dialog text fields trigger global shortcuts (Backspace deletes the selected node) | keyboard | from code |

## The common root: five ways to change the document

Every finding except 5 comes from the same place. The program has several
paths that write `App.graph`, and each does a **different subset** of the
bookkeeping that should go with an edit:

| Path | Undo snapshot | Clears redo | Recomputes conflicts (`after_edit`) | Reruns preview + autosave |
|---|---|---|---|---|
| `App::edit` [state.rs:190](../../src/ui/state.rs#L190) | yes, **even for a no-op** | yes | yes | yes |
| `App::edit_coalesced`, new key [state.rs:199](../../src/ui/state.rs#L199) | yes | yes | yes | yes |
| `App::edit_coalesced`, same key within 1 s | no | no | yes | yes |
| `App::undo` / `redo` [state.rs:262](../../src/ui/state.rs#L262) | swaps | n/a | **no** | yes |
| `App::replace_document` [state.rs:247](../../src/ui/state.rs#L247) | resets history | resets | yes | yes (autosave skips one run) |
| Direct `graph.write()`: node drag [graph_view.rs:282](../../src/ui/graph_view.rs#L282), backend badge [mod.rs:439](../../src/ui/mod.rs#L439), `fit_canvas_to_window` [state.rs:337](../../src/ui/state.rs#L337) | **no** | no | **no** | yes |

Findings 1 and 4 come from the first three rows: no-op snapshots, and keys
chosen per gesture. Finding 2 is the `undo` row. Finding 3 is the last row,
reached from an effect rather than from an input. **The decision for a
human:** should "an edit" become one function that owns all four columns,
with view-only changes (such as node positions) as an explicit second kind?

---

<a id="finding-1"></a>
## Finding 1 · Clicking a node in the graph view adds an empty undo step and erases redo

**Path.** A press on a node ([graph_view.rs:337-345](../../src/ui/graph_view.rs#L337))
calls `edit_coalesced("move:<id>", |_| {})` so that a following drag has an
undo snapshot. The snapshot is taken on the press, whether or not the
pointer moves, and `push_history` clears redo ([state.rs:186](../../src/ui/state.rs#L186)).

**Effect.**
- Undo some edits, click a node to look at it in the inspector, and **redo is gone**.
- After a click, the next ⌘Z appears to do nothing: it restores an identical graph.

**Confirmed.** `selecting_a_node_in_the_graph_view_keeps_redo` and
`selecting_a_node_in_the_graph_view_adds_no_undo_step` in
[probes/input_probes.rs](probes/input_probes.rs).

**Same family, from code.** A refused connection (cycle, wrong port) is
also an empty undo step, because `connect_port` runs inside `App::edit`
whatever the result ([graph_view.rs:112-137](../../src/ui/graph_view.rs#L112)).

**Repair options** (not implemented): take the snapshot on the first
*move* instead of on the press, or make `edit`/`edit_coalesced` skip the
push when the graph did not change.

---

<a id="finding-2"></a>
## Finding 2 · Undo and redo do not update the size-conflict prompt

**Path.** `after_edit` ([state.rs:304](../../src/ui/state.rs#L304)) is the only
code that sets `App.conflict`. `undo` and `redo` ([state.rs:262-284](../../src/ui/state.rs#L262))
swap the graph and call only `fix_selection`.

**Effect.**
- Connect two differently sized images to a Blend: the prompt opens. Press ⌘Z: the connection is undone, but **the prompt stays open** for a conflict that no longer exists. Its buttons then add a no-op step (`insert_resize` finds no input).
- Resolve a conflict with "Use 100×100", then undo: the conflict is back but **no prompt appears**. It opens later, **on an unrelated edit**, such as moving a slider on another node.

**Confirmed.** `undoing_a_conflicting_connection_dismisses_the_prompt` and
`undoing_a_resolution_does_not_reopen_the_prompt_on_a_later_unrelated_edit`.

**Repair** (not implemented): call `after_edit` from `undo` and `redo`, or
derive the prompt from the graph instead of storing it.

---

<a id="finding-3"></a>
## Finding 3 · Opening a project whose output is size-less rewrites its canvas and its file

**Path.** No user input is involved. After `on_project_opened`
([session.rs:202](../../src/ui/session.rs#L202)), `replace_document` resets
`canvas_user_set` to `false` ([state.rs:257](../../src/ui/state.rs#L257)). The image
view's **canvas effect** ([image_view.rs:70-76](../../src/ui/image_view.rs#L70))
sees a size-less output and calls `fit_canvas_to_window(false)`
([state.rs:323](../../src/ui/state.rs#L323)). That writes `canvas_w`/`canvas_h`
straight into the graph, with no history. ⇢ The **autosave** effect sees a
graph change after its "just opened" skip and writes `project.delos`.

**Effect.** A project whose output is a generator (or a stack of
generators), saved with a 1920×1080 canvas, comes back as **92 % of the
current window**. The file on disk is rewritten in canonical form within a
second of opening, and ⌘Z cannot restore the size. The next export has the
wrong size. This contradicts the README's "Opening a project changes
nothing on disk until you edit it". The existing test
`opening_a_project_does_not_rewrite_it` uses a sized image, so it does not
catch this.

**Confirmed.** `opening_a_project_with_a_sizeless_output_does_not_rewrite_it`
in [probes/project_probes.rs](probes/project_probes.rs) replays the effect's
body. The test cannot mount the private image view, so it asserts the
effect's condition and then calls what the effect calls. Result: canvas
1920×1080 → 920×644, the file is rewritten, and `can_undo() == false`.

**Also from code.** Typing a canvas size in the Output panel does not set
`canvas_user_set` (only dragging the corner does), so the next window resize
overwrites a typed size too.

**Repair options** (not implemented): treat a canvas size that came from a
file as user-set, and save `canvas_user_set` with the document; or never
let an effect write the document, and size new generators' canvases in
`add_node` instead.

---

<a id="finding-4"></a>
## Finding 4 · Dragging a point handle pushes two undo steps per pointer move

**Path.** `Drag::Point` ([image_view.rs:249-252](../../src/ui/image_view.rs#L249))
sets `x` and `y` with two `App::set_param` calls. Their coalescing keys
(`"<node>:cx"`, `"<node>:cy"`) alternate, so neither merges with the
previous one ([state.rs:199-207](../../src/ui/state.rs#L199)). Rectangle drags avoid
this with one shared key, `"<node>:rect"` ([image_view.rs:260](../../src/ui/image_view.rs#L260)).

**Effect.** For Linear Gradient's A/B handles and Radial Gradient's centre,
every pointer move costs two undo steps. History is capped at 60
([state.rs:142](../../src/ui/state.rs#L142)), so **a drag of 30 pointer moves
(about half a second) evicts every edit made before it.** One ⌘Z undoes
only half of the last move.

**Confirmed.** `one_undo_reverts_a_whole_point_drag` and
`a_point_drag_does_not_evict_earlier_undo_history`. The second reports "the
drag pushed 60 undo steps" for a 40-move drag.

**Repair** (not implemented): one `edit_coalesced("<node>:point:<kx>")`
that sets both parameters, like the rectangle.

---

<a id="finding-5"></a>
## Finding 5 · Keys typed in dialog text fields trigger global shortcuts

**Path.** The only key handler is on the root `div.app` ([mod.rs:84](../../src/ui/mod.rs#L84)).
The only element that stops keys from reaching it is the inspector
([inspector.rs:19](../../src/ui/inspector.rs#L19)). The export and project dialogs
are rendered inside `div.app` ([mod.rs:119-121](../../src/ui/mod.rs#L119)), and their
text fields ([dialogs.rs:150-155](../../src/ui/dialogs.rs#L150), [dialogs.rs:246-250](../../src/ui/dialogs.rs#L246))
do not stop `keydown`.

**Effect (from code).** With a node selected, ⌘E opens the export dialog for
that node. Pressing Backspace in *File name* edits the name **and deletes
the node** (`delete_selection`). The export then fails with a missing-node
error. ⌘Z in the field undoes graph edits, and F turns on fit mode.
`tests/ui_state.rs` cannot dispatch DOM events, so this was **not
reproduced**. The inspector's `stop_propagation` exists precisely because
the same bubbling affected its own fields.

**Repair** (not implemented): stop `keydown` on the modals, or have
`on_key` ignore events whose target is editable.

---

<a id="observations"></a>
## Observations

These are traced behaviours that are *decisions*, not bugs. Each one is a
place where the code made a choice that a human may want to own.

1. **Autosave runs once per graph write.** Node drags, slider drags and overlay drags write the graph on every pointer move. In a project, each write starts a background thread that runs `sync_inputs` (reading `inputs/`), serialises the graph, compares it with the file and usually rewrites `project.delos` atomically ([session.rs:251](../../src/ui/session.rs#L251), [worker.rs:356](../../src/worker.rs#L356)). This is correct but chatty. Debouncing would be a product decision about how much work can be lost.
2. **Validation lives at the input sites, and differs by path.**

   | Path | Range check |
   |---|---|
   | Inspector Int number field | clamped to min–max [inspector.rs:337](../../src/ui/inspector.rs#L337) |
   | Inspector Float / Fraction number fields | **none** [inspector.rs:276](../../src/ui/inspector.rs#L276), [inspector.rs:307](../../src/ui/inspector.rs#L307) |
   | Inspector sliders | browser limits to min–max |
   | On-canvas drags | clamped (0–1, points −1–2) [image_view.rs:247](../../src/ui/image_view.rs#L247) |
   | `project.delos` | types and choice names only [expr.rs:698](../../src/expr.rs#L698) |

   For example, Resize's Int *width* field caps at 65,536, but its Float *scale* field accepts any value, and the output size is input × scale ([resize.rs:64-72](../../src/nodes/resize.rs#L64)). The node kind declares ranges (`ParamDef`), but nothing enforces them where values enter the document.
3. **Untrusted files.** Project folders and archives are designed to be shared. `unpack` blocks path traversal (`enclosed_name`) and cleans up after itself on failure. Image decoding is deliberately unlimited (`no_limits`, [formats.rs:75](../../src/formats.rs#L75)), and so is zip extraction. Opening a shared project therefore trusts its sizes.
4. **"Cancel" discards imports.** ×, Esc and Cancel on the first-import prompt drop the files ([session.rs:147](../../src/ui/session.rs#L147)). This is tested, so it is intended.
5. **Mixed drops.** Dropping a project together with images opens the project and ignores the images ([mod.rs:93](../../src/ui/mod.rs#L93)).
6. **Mobile never asks.** `platform::confirm` answers yes where there is no native dialog ([platform.rs:231](../../src/ui/platform.rs#L231)), so ending a quick session on mobile discards its work without a prompt. Exports on mobile overwrite `~/Documents/<name>` without a dialog.
7. **Folders and `project.delos` files open in place.** Autosave then writes into whatever folder was opened, including one received from someone else. Only `.delosproject` archives are copied first.
8. **The backend badge's `graph.write()` is redundant** ([mod.rs:439](../../src/ui/mod.rs#L439)). The preview key already includes the backend, so the write only causes an extra autosave pass.
9. **DPR is read once** ([image_view.rs:44-48](../../src/ui/image_view.rs#L44)). Moving the window to a display with a different pixel density keeps the old resolution until the image view is remounted.
