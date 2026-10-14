# 04 · Image view: camera, on-canvas controls, canvas size

The image view ([image_view.rs:31](../../src/ui/image_view.rs#L31)) takes two
kinds of input that look the same, since both are pointer gestures on one
`div`, but go to different places:

- **camera** inputs (zoom, pan, fit) change `App.view`, which is view state;
- **control** inputs (dragging an axis, a split border, a point, a rectangle, the canvas corner) change **node parameters**, which are document state.

A local `drag` signal ([image_view.rs:18-28](../../src/ui/image_view.rs#L18)) tells
them apart. Every gesture starts with a press and is then driven by the one
`onpointermove` handler.

| Input | Entry | Changes |
|---|---|---|
| Wheel: zoom about the cursor (10⁻⁴–64×) | [image_view.rs:207](../../src/ui/image_view.rs#L207) | `view` (fit off) |
| Press on the image: start panning | [image_view.rs:222](../../src/ui/image_view.rs#L222) | local `drag` |
| Pointer move: pan or drag a control | [image_view.rs:227](../../src/ui/image_view.rs#L227) | `view` or node parameters |
| Release / leave: end the gesture | [image_view.rs:289](../../src/ui/image_view.rs#L289), [image_view.rs:290](../../src/ui/image_view.rs#L290) | local `drag` |
| Double-click: fit | [image_view.rs:291](../../src/ui/image_view.rs#L291) | `view.fit` |
| Status bar: Fit, 1:1, +, − (presses do not start a pan) | [image_view.rs:346](../../src/ui/image_view.rs#L346), [image_view.rs:349](../../src/ui/image_view.rs#L349), [image_view.rs:356](../../src/ui/image_view.rs#L356), [image_view.rs:357](../../src/ui/image_view.rs#L357), [image_view.rs:341](../../src/ui/image_view.rs#L341) | `view` |
| Press on an axis / split border | [image_view.rs:402](../../src/ui/image_view.rs#L402), [image_view.rs:436](../../src/ui/image_view.rs#L436) | starts `Drag::Axis` |
| Press on a point (gradient A/B, radial centre) | [image_view.rs:457](../../src/ui/image_view.rs#L457) | starts `Drag::Point` |
| Press inside a rectangle (Clip, Rectangle Split) / on a corner | [image_view.rs:484](../../src/ui/image_view.rs#L484), [image_view.rs:498](../../src/ui/image_view.rs#L498) | starts `Drag::RectMove` / `RectCorner` |
| Press on the canvas corner (size-less output) | [image_view.rs:313](../../src/ui/image_view.rs#L313) | starts `Drag::Canvas` |
| Element mounted / resized | [image_view.rs:197](../../src/ui/image_view.rs#L197), [image_view.rs:201](../../src/ui/image_view.rs#L201) | `viewport`, screen origin |
| Device pixel ratio (asked once from the webview) | [image_view.rs:44-48](../../src/ui/image_view.rs#L44) → [platform.rs:160](../../src/ui/platform.rs#L160) | local `dpr` |

## Trace: scroll to zoom

1. `onwheel` ([image_view.rs:207-221](../../src/ui/image_view.rs#L207)): factor `exp(-dy·0.0015)`, clamped to ½–2× per event. It keeps the image point under the cursor fixed and turns `fit` off.
2. ⇢ **preview** effect: the visible region and the request size changed, so the key changes and a new `PreviewJob` is sent ([image_view.rs:94-124](../../src/ui/image_view.rs#L94)). The request covers **only the visible part of the image, at screen resolution** (capped at 10 MP). This is the "pull-based, resolution-independent" design in action.
3. Until the new frame arrives, the old frame stays where it is, scaled with the view. The status bar shows the render time (or "(preview)" for a provisional frame).

Panning works the same way ([image_view.rs:239-244](../../src/ui/image_view.rs#L239)).
No camera input changes the document, so none of them triggers autosave.

<a id="overlays"></a>
## Trace: drag a split's border over the output

What the user sees depends on `App::displayed()` ([state.rs:351](../../src/ui/state.rs#L351)):

- **"Preview selection" on, nothing relocated**: selecting the Vertical Split shows the split's *edit view*. Its `EditView::Input(0)` means the uncut input, with the border on top.
- **Relocated output, or "Preview selection" off** (`locked`): the view stays on the output, and `Graph::space_map` composes each downstream node's `input_map` to place the split's border in **output coordinates** ([graph.rs:313](../../src/graph.rs#L313)). If the split does not feed the shown image, the overlay is hidden and a badge says so.

Then:

1. **Press on the border** ([image_view.rs:436-440](../../src/ui/image_view.rs#L436)): freezes `frac_map`, the screen → overlay-fraction map, so the mapping cannot shift under the cursor while the parameter changes it. Sets `Drag::Axis { node, key: "position" }`. Stops propagation, so no pan starts.
2. ⇢ effect: the **fit** effect reads `drag` and holds the zoom while a control is dragged ([image_view.rs:56-59](../../src/ui/image_view.rs#L56)).
3. **Each move** ([image_view.rs:245-248](../../src/ui/image_view.rs#L245)) maps the cursor through `frac_map`, clamps it to 0–1 and calls `App::set_param(node, "position", …)`. That is the same path as the inspector slider ([03-inspector.md](03-inspector.md)): coalesced into one undo step, then preview, then autosave.

Rectangles use a single coalescing key `"<node>:rect"` for their four
parameters ([image_view.rs:260](../../src/ui/image_view.rs#L260), [image_view.rs:270](../../src/ui/image_view.rs#L270)).
**Points do not.** `Drag::Point` sets `x` and `y` with two `set_param` calls
whose keys alternate ([image_view.rs:249-252](../../src/ui/image_view.rs#L249)).
Every pointer move therefore pushes two undo snapshots, and a short drag
fills the 60-step history ([finding 4](findings.md#finding-4), confirmed
by probe).

<a id="canvas"></a>
## Trace: the output canvas (size-less content)

Generators have no size. When the shown node is size-less, its extent is
the Output node's `canvas_w × canvas_h` ([graph.rs:301](../../src/graph.rs#L301)).
Three inputs set it:

| Input | Path | Undoable? |
|---|---|---|
| Dragging the canvas corner | `Drag::Canvas` → `edit_coalesced("canvas")` ([image_view.rs:277-286](../../src/ui/image_view.rs#L277)), and `canvas_user_set = true` | yes |
| Typing Canvas width/height (Output panel) | `set_param` ([03-inspector.md](03-inspector.md)) | yes, but `canvas_user_set` stays false |
| "Canvas = window size" button | `fit_canvas_to_window(true)` ([inspector.rs:204](../../src/ui/inspector.rs#L204), [state.rs:323](../../src/ui/state.rs#L323)) | **no**: writes `App.graph` directly |
| *No input at all*: the **canvas effect** | When `disp.sizeless` and the viewport is known, `fit_canvas_to_window(false)` ([image_view.rs:70-76](../../src/ui/image_view.rs#L70)) sets the canvas to 92 % of the viewport, unless `canvas_user_set` | **no** |

The last row is an effect that **edits the document**. `canvas_user_set` is
per session and is reset by `replace_document`. So opening a project whose
output is a generator, or typing a canvas size and then resizing the
window, quietly replaces the saved canvas size, and autosave writes it to
disk ([finding 3](findings.md#finding-3), confirmed by probe).

## Trace: the window is resized

`onresize` ([image_view.rs:201-206](../../src/ui/image_view.rs#L201)) writes
`App.viewport` ⇢ effects: **fit** refits in fit mode, **preview** renders
the newly visible region and **canvas** may resize a size-less output
(above). The graph view's `onresize` ([graph_view.rs:255](../../src/ui/graph_view.rs#L255))
only refreshes its screen origin.

## Decisions

- **The same gesture edits either the view or the document,** depending on where the press lands. Only document edits are undoable and saved.
- **Coalescing keys are chosen per gesture by hand** (`"node:param"`, `"node:rect"`, `"canvas"`, `"move:node"`). A gesture that touches two parameters has to pick one key, and Point forgot to.
- **An effect that writes the document** is convenient for new generators, but it is the only place where the program changes the document without the user asking.
