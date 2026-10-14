# 03 · Inspector: parameters, inputs, layers, node actions

The inspector ([inspector.rs:14](../../src/ui/inspector.rs#L14)) shows `NodePanel`
for the selected node, or `DocumentPanel` (which wraps the Output node's
panel) when nothing is selected. **Its forms are generated, not written by
hand.** Each node kind declares `ParamDef`s ([nodes/mod.rs:77](../../src/nodes/mod.rs#L77)),
and `ParamField` ([inspector.rs:249](../../src/ui/inspector.rs#L249)) picks a
widget from the `ParamKind`. Every widget ends in the same call, so all of
the following inputs share one trace.

| Input (by `ParamKind`) | Widgets and entries | Value written |
|---|---|---|
| `Float {min,max,step}` | range [inspector.rs:269](../../src/ui/inspector.rs#L269), number [inspector.rs:276](../../src/ui/inspector.rs#L276) | `Float(x)`, **not clamped** |
| `Fraction {axis}` | range 0–1 [inspector.rs:300](../../src/ui/inspector.rs#L300), pixels [inspector.rs:307](../../src/ui/inspector.rs#L307) | `Float(x)` or `Float(px / extent)`, **not clamped** |
| `Int {min,max}` | range (if span ≤ 64) [inspector.rs:328](../../src/ui/inspector.rs#L328), number [inspector.rs:337](../../src/ui/inspector.rs#L337) | `Int(x)`, the number field **clamps** to min–max |
| `Bool` | checkbox [inspector.rs:347](../../src/ui/inspector.rs#L347) | `Bool` |
| `Choice(options)` | select [inspector.rs:357](../../src/ui/inspector.rs#L357) | `Choice(key)` |
| `Color` | color picker [inspector.rs:375](../../src/ui/inspector.rs#L375), opacity range [inspector.rs:384](../../src/ui/inspector.rs#L384) | `Color([r,g,b,a])` |
| `Text` | text field [inspector.rs:395](../../src/ui/inspector.rs#L395) | `Text(s)` (e.g. the background-removal model path) |
| `Hidden` | none (image id, width, height) | set only by import and load |

Other inspector inputs:

| Input | Entry |
|---|---|
| Node title field | [inspector.rs:108](../../src/ui/inspector.rs#L108) |
| "View as output" / "View left/right" | [inspector.rs:146](../../src/ui/inspector.rs#L146) |
| "Export…" for this node (output) | [inspector.rs:154](../../src/ui/inspector.rs#L154) → [06-export.md](06-export.md) |
| "Delete" | [inspector.rs:155](../../src/ui/inspector.rs#L155) |
| Input picker `<select>` per port | [inspector.rs:167](../../src/ui/inspector.rs#L167) |
| Output panel: "Canvas = window size" | [inspector.rs:204](../../src/ui/inspector.rs#L204) |
| Layer stack: enable/disable, select, ▲, ▼, ✕, + generator, + empty slot | [inspector.rs:448](../../src/ui/inspector.rs#L448), [inspector.rs:466](../../src/ui/inspector.rs#L466), [inspector.rs:473](../../src/ui/inspector.rs#L473), [inspector.rs:480](../../src/ui/inspector.rs#L480), [inspector.rs:486](../../src/ui/inspector.rs#L486), [inspector.rs:508](../../src/ui/inspector.rs#L508), [inspector.rs:523](../../src/ui/inspector.rs#L523) |
| Layer stack: "+ Image layer" | [inspector.rs:499](../../src/ui/inspector.rs#L499) → [01-images-in.md](01-images-in.md) |
| Keystrokes inside the inspector stop here | [inspector.rs:19](../../src/ui/inspector.rs#L19) → [05-keyboard.md](05-keyboard.md) |

## Trace: drag the Noise "Contrast" slider

1. Each `oninput` ([inspector.rs:269](../../src/ui/inspector.rs#L269)) parses the string and calls the local `set` closure ([inspector.rs:255](../../src/ui/inspector.rs#L255)) → `App::set_param(id, "contrast", Float(x))` ([state.rs:217](../../src/ui/state.rs#L217)).
2. `edit_coalesced("<id>:contrast", …)`: the first event of the drag pushes an undo snapshot. Later events within 1 s of each other reuse it, so **one undo step undoes the whole drag**. `Graph::set_param` writes the value. `after_edit` checks for size conflicts.
3. ⇢ **preview** effect ([image_view.rs:83](../../src/ui/image_view.rs#L83)): the structural hash of the shown target now differs, so a new `seq` and `PreviewJob` are created.
4. ⇢ thread: the preview mailbox holds one job, so a newer slider position **replaces** an unstarted one. A render in flight sees `latest != seq` at its next node or kernel and stops ([worker.rs:182](../../src/worker.rs#L182)). If the last full render took over 80 ms, a quarter-resolution provisional frame comes first ([worker.rs:196-207](../../src/worker.rs#L196)).
5. ⇢ `Msg::Frame`. `pump` swaps the `<img>` source. When the frame is final and current, the spinner stops ([mod.rs:134-160](../../src/ui/mod.rs#L134)).
6. ⇢ **autosave** (in a project): one save per `oninput` event.

Typed number fields use `onchange` instead, so they edit once, when the
field commits.

## Trace: choose an input from the picker

The candidates ([inspector.rs:76-82](../../src/ui/inspector.rs#L76)) are every
output of every node that is not the node itself, not the Output, not an
interior stack link, and that **would not create a cycle** (`!depends_on`).
So the picker cannot offer a cycle in the first place. The graph view's wire
drag cannot prevent one in advance, so it rejects the connection when the
wire is dropped.

`onchange` ([inspector.rs:167-174](../../src/ui/inspector.rs#L167)) parses the
value `"node:output"`:

- a link → `connect_port(app, src, PortRef::Input(id, port))`, the same function a wire drop calls ([02-graph-view.md](02-graph-view.md));
- "— none —" → `App::edit(Graph::disconnect)`.

## Trace: Layer stack ▲ / ▼ / ✕ / 👁

All of them run through the local `set_layers` closure ([inspector.rs:428](../../src/ui/inspector.rs#L428)).
It does **one `App::edit`** that reads the chain, lets the button edit the
`Vec<Option<Link>>` of layers, and writes it back with
`Graph::set_stack_layers` ([graph.rs:470](../../src/graph.rs#L470)).
`set_stack_layers` keeps the head node's id, so whatever consumes the
stack stays connected. It adds or removes interior `stack` nodes as needed.

- **👁 disable** ([inspector.rs:448-461](../../src/ui/inspector.rs#L448)) wraps the layer in a new `Disable` node. **Enable** unwraps it and removes the `Disable` node.
- **+ Generator layer** ([inspector.rs:508-517](../../src/ui/inspector.rs#L508)) adds a generator node and inserts it as layer 0.
- **Clicking a layer name** ([inspector.rs:466](../../src/ui/inspector.rs#L466)) selects that layer's node.

## Trace: Delete

[inspector.rs:155](../../src/ui/inspector.rs#L155) → `App::delete_node` ([state.rs:505](../../src/ui/state.rs#L505)),
one edit:

- a layer stack: deletes its `stack` nodes, which also disconnects the layers;
- a node with exactly one connected input: removes it and **bridges** that input to all its consumers;
- otherwise: removes it, and its consumers lose that input.

`Graph::remove_node` also drops image data that no node uses any more
([graph.rs:199](../../src/graph.rs#L199)). The undo snapshot still holds it.
Selection and the relocated output are cleared if they pointed at the node.

## Decisions

- **Validation happens where the input enters, and differs by path.** For the same parameter: typed Int values are clamped, typed Float and Fraction values are not, on-canvas drags are clamped (to 0–1 or −1–2), and `project.delos` checks types and choice names but not ranges. See [findings](findings.md#observations).
- **The form is derived from the node kind.** Adding a parameter to a kind adds a widget, an expression attribute, a hash input and an autosaved field at once. The `ParamKind` is the only per-path control.
- **Disable is structural.** It inserts a `Disable` node. The document contains it, and so do saved projects.
