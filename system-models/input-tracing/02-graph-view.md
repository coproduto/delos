# 02 · Graph view: selecting, wiring, moving, relocating

The graph view ([graph_view.rs:189](../../src/ui/graph_view.rs#L189)) draws
`Graph` through `build` ([graph_view.rs:62](../../src/ui/graph_view.rs#L62)). Chains
of `stack` nodes collapse into one "Layer Stack" node with a port per layer.
Its pan, zoom and current drag are **local signals** of the component, so
camera moves never touch `App`.

| Input | Entry |
|---|---|
| Press on empty canvas: deselect + start panning | [graph_view.rs:266](../../src/ui/graph_view.rs#L266) |
| Pointer move: pan / move node / extend wire | [graph_view.rs:272](../../src/ui/graph_view.rs#L272) |
| Release or leave: end any drag | [graph_view.rs:293](../../src/ui/graph_view.rs#L293), [graph_view.rs:294](../../src/ui/graph_view.rs#L294) |
| Wheel: zoom about the cursor (0.2–3×) | [graph_view.rs:256](../../src/ui/graph_view.rs#L256) |
| Press on a node: select + start a move | [graph_view.rs:337](../../src/ui/graph_view.rs#L337) |
| Double-click a node: relocate the output to it / back | [graph_view.rs:346](../../src/ui/graph_view.rs#L346) |
| Press on an output port: start a wire | [graph_view.rs:375](../../src/ui/graph_view.rs#L375) |
| Double-click an output port: relocate the output to that output | [graph_view.rs:381](../../src/ui/graph_view.rs#L381) |
| Press on a connected input port: pick the wire up | [graph_view.rs:404](../../src/ui/graph_view.rs#L404) |
| Release on an input port: connect | [graph_view.rs:414](../../src/ui/graph_view.rs#L414) |
| Press on a wire: select the edge | [graph_view.rs:306](../../src/ui/graph_view.rs#L306) |
| Home / + / − buttons | [graph_view.rs:436](../../src/ui/graph_view.rs#L436), [graph_view.rs:452](../../src/ui/graph_view.rs#L452), [graph_view.rs:453](../../src/ui/graph_view.rs#L453) |
| Toolbar strip swallows presses | [graph_view.rs:433](../../src/ui/graph_view.rs#L433) |
| Mount / resize: remember the element's screen origin | [graph_view.rs:251](../../src/ui/graph_view.rs#L251), [graph_view.rs:255](../../src/ui/graph_view.rs#L255) |
| Toolbar **Add node ▾** menu | [mod.rs:362](../../src/ui/mod.rs#L362), [mod.rs:364](../../src/ui/mod.rs#L364), [mod.rs:372](../../src/ui/mod.rs#L372) |
| "Different input sizes" dialog | [dialogs.rs:42](../../src/ui/dialogs.rs#L42), [dialogs.rs:51](../../src/ui/dialogs.rs#L51), [dialogs.rs:58](../../src/ui/dialogs.rs#L58) |

Screen points become graph points with `to_graph` (origin, pan, zoom). The
origin comes from `get_client_rect` on mount and resize.

## Trace: click a node, then drag it

1. **Press** ([graph_view.rs:337-345](../../src/ui/graph_view.rs#L337)): stops propagation (so the canvas does not deselect), then:
   - `App::select(Some(id))` ([state.rs:444](../../src/ui/state.rs#L444)) writes `selected` and clears `selected_edge`. ⇢ effect: `disp` changes, and with "Preview selection" on the image view now shows this node. The inspector re-renders for it.
   - `App::edit_coalesced("move:<id>", |_| {})` takes an undo snapshot **before** the node moves, with a no-op edit. This snapshot doubles as the move's undo step.
   - Local `drag = Node { id, dx, dy }`.
2. **Move** ([graph_view.rs:280-285](../../src/ui/graph_view.rs#L280)): writes `node.pos` straight into `app.graph.write()`. This bypasses `edit`, so there is no history entry per move and no `after_edit`.
   ⇢ effects on every move: the **preview** effect reruns but its key is unchanged (position is not part of the structural hash, [graph.rs:381](../../src/graph.rs#L381)), so nothing renders. **Autosave** reruns and, in a project, saves `project.delos` again (`:at` changed).
3. **Release**: `drag = None`.

Consequence: **a plain click on a node is an undo step that changes
nothing, and it erases the redo stack** ([finding 1](findings.md#finding-1),
confirmed by probe).

## Trace: drag a wire from an output to an input

1. **Press on an output port** ([graph_view.rs:375-380](../../src/ui/graph_view.rs#L375)): `drag = Wire { from: Link(node, output) }`. Nodes with several outputs (splits) have one port per output, so `from` names exactly one.
2. **Move**: updates the wire's end point, which is drawn as a dashed path.
3. **Release on an input row** ([graph_view.rs:414-419](../../src/ui/graph_view.rs#L414)) calls `connect_port(app, from, port)` ([graph_view.rs:112](../../src/ui/graph_view.rs#L112)), always inside **one `App::edit`**:
   - an ordinary port goes through `Graph::connect` ([graph.rs:243](../../src/graph.rs#L243)). It refuses unknown nodes, the output as a source, missing outputs or ports, and **cycles** (`depends_on`). The refusal comes back as an error toast. The undo step was still pushed.
   - a layer-stack row or "+ add layer" goes through a cycle check, then rewrites the chain with `set_stack_layers`.
   - after the edit, `after_edit` may raise a size conflict (below).
4. **Release anywhere else**: the canvas `onpointerup` ([graph_view.rs:293](../../src/ui/graph_view.rs#L293)) ends the drag, and the wire is dropped.

## Trace: pick up an existing wire

**Press on a connected input port** ([graph_view.rs:404-413](../../src/ui/graph_view.rs#L404)):
`disconnect_ref` ([graph_view.rs:163](../../src/ui/graph_view.rs#L163)) removes
the edge right away (one edit; for a layer row it **removes the layer**).
Then a wire drag starts from the old source. Dropping it on another port
connects it there (a second edit). Dropping it on nothing leaves it
disconnected, which is how you delete a wire by dragging it off.

## Trace: select a wire, press Delete

Pressing the wire's hit path ([graph_view.rs:306-310](../../src/ui/graph_view.rs#L306))
clears `selected` and sets `selected_edge` to `encode_edge(port)`. Layer
rows are encoded as `port + 1_000_000` ([graph_view.rs:155](../../src/ui/graph_view.rs#L155)).
Delete then runs `delete_selection` → `disconnect_port` → `decode_edge` →
`disconnect_ref` ([state.rs:491](../../src/ui/state.rs#L491), [graph_view.rs:141](../../src/ui/graph_view.rs#L141)).
See [05-keyboard.md](05-keyboard.md).

## Trace: double-click a node ("relocate the output")

[graph_view.rs:346-356](../../src/ui/graph_view.rs#L346): on the Output node, or
on the node that is already relocated, `pinned = None`. Otherwise
`pinned = Some(id)` (its first output), and the selection is cleared. An
output port's double-click ([graph_view.rs:381-385](../../src/ui/graph_view.rs#L381))
pins that specific output. `pinned` is **view state, not document state**:
no undo, no autosave. ⇢ effects: `displayed()` now uses the pinned output
as the base, and `locked` becomes true. Selecting other nodes then edits
them *over* the pinned image instead of previewing them
([04-image-view.md](04-image-view.md#overlays)). The toolbar shows "Output
relocated to …" with **Reset** ([mod.rs:422](../../src/ui/mod.rs#L422)).

## Trace: Add node ▾ → Invert (with a node selected)

1. **Add node ▾** toggles the local `menu_open` ([mod.rs:362](../../src/ui/mod.rs#L362)). The backdrop closes it ([mod.rs:364](../../src/ui/mod.rs#L364)). The menu lists `nodes::registry()` kinds that are `user_creatable`, grouped by category.
2. **Item** ([mod.rs:372-375](../../src/ui/mod.rs#L372)) → `App::add_node(kind)` ([state.rs:452](../../src/ui/state.rs#L452)), one edit:
   - selected node (not the output) as the **anchor**: the new node goes 220 px to its right. If the new kind has inputs, it **takes over every consumer of the anchor** (`Graph::consumers`, all of the anchor's outputs) and is fed by the anchor's output 0.
   - no anchor, or a kind without inputs (generators): the node connects to the output only if the output is empty.
3. Generators then call `fit_canvas_to_window(false)` (not undoable; see [04-image-view.md](04-image-view.md#canvas)). The new node is selected.

The "consumers of all outputs" rule is the invariants model's
[finding 2](../invariants/findings.md#finding-2): adding a node after a
split moves the *right* side's consumers onto the *left* output.

## Size conflict

Any edit that leaves a combining node (Blend, Stack) with two inputs of
different definite sizes makes `after_edit` set `App.conflict`
([state.rs:304](../../src/ui/state.rs#L304)), and the modal opens. Its three
answers:

- **Use A's size** ([dialogs.rs:42-45](../../src/ui/dialogs.rs#L42)) or **Use B's size** ([dialogs.rs:51-54](../../src/ui/dialogs.rs#L51)): `App::edit(Graph::insert_resize)` puts a Resize node in front of the other input ([graph.rs:367](../../src/graph.rs#L367)), then closes the dialog.
- **Keep both sizes** ([dialogs.rs:58](../../src/ui/dialogs.rs#L58)) → `ignore_conflict` remembers `(node, sizes)` for the rest of this document.

Undo and redo do not recompute the conflict ([finding 2](findings.md#finding-2),
confirmed by probe). The dialog can stay open after the conflict is undone,
and it can open later, on an unrelated edit, for a conflict that an undo
brought back.

## Decisions

- **What is an undo step?** Today: every `edit` call, including no-ops (clicks, refused connections). Rewiring by pick-up is two steps.
- **Position changes skip history and `after_edit` but not autosave.** One file write per pointer move during a drag (see [findings](findings.md#observations)).
- **Insert-after semantics for nodes with several outputs** (invariants finding 2).
- **Relocating the output is session view state**, so it is not saved with the project.
