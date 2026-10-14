# 2 · Editing, views and export

The spec's *Basic functionalities* (items 1 to 5). IDs follow the spec's
own numbering: F3.1.1 is item 3.1.1.

## F1 · Load and display images as graph nodes {#f1}

> Delos must be able to load and display existing images.
> Opening an image creates a "layer" containing that image.
> Layers are treated as graph nodes which can be connected using "operators".

**Status.** Implemented.

**Implemented by.**
- Decoding goes through the `Codec` registry (`decode` picks the first codec that recognises the bytes) ([formats.rs:183-186](../../src/formats.rs#L183)). PNG, JPEG, WebP, GIF, BMP, TIFF and ICO are listed in `CODECS` ([formats.rs:152-164](../../src/formats.rs#L152)).
- `Worker::import` reads and `decode`s the bytes in the background and builds a `SourceImage` with its mip pyramid, which it sends back as `Msg::Imported` ([worker.rs:248-256](../../src/worker.rs#L248), [worker.rs:266](../../src/worker.rs#L266), [worker.rs:270-277](../../src/worker.rs#L270)); `SourceImage::from_rgba8` premultiplies and builds the levels ([source.rs:43-65](../../src/engine/source.rs#L43)).
- The "layer" is a `StaticImage` node ([basic.rs:62](../../src/nodes/basic.rs#L62)) whose `params` hold the image id and size as hidden parameters and whose `render` samples the image ([basic.rs:77-91](../../src/nodes/basic.rs#L77)). Nodes are connected by `Graph::connect` ([graph.rs:243-264](../../src/graph.rs#L243)).
- Entry point: the toolbar's Import file input ([mod.rs:344-360](../../src/ui/mod.rs#L344)), through `session::import`, which hands the files to `start_imports` ([session.rs:100-103](../../src/ui/session.rs#L100), [session.rs:119-120](../../src/ui/session.rs#L119)).

**Tests.** `tests/engine.rs::native_render_reproduces_source`, `tests/project_e2e.rs::first_import_asks_and_creates_a_project`, `tests/project_e2e.rs::quick_mode_imports_without_copying`.

**Notes.** Dropping image files on the window (`ondrop`) also imports them through `session::import` ([mod.rs:86-90](../../src/ui/mod.rs#L86), [mod.rs:102-103](../../src/ui/mod.rs#L102)).

## F1.1 · Import creates a Static Image connected to the output {#f1-1}

> Importing an image to Delos specifically creates a "Static Image" node connected directly to the output.

**Status.** Implemented.

**Implemented by.**
- `finish_import` adds the `ids::IMAGE` node and, for `ImportTarget::Output`, connects it to the output's only port with `connect` ([mod.rs:255-268](../../src/ui/mod.rs#L255)). The target enum cites this spec item: `ImportTarget` ([state.rs:68-72](../../src/ui/state.rs#L68)). Image files dropped on the window are imported with `ImportTarget::Output` too (`ondrop`) ([mod.rs:86-90](../../src/ui/mod.rs#L86), [mod.rs:102-103](../../src/ui/mod.rs#L102)).
- The kind's display name is "Static Image": `StaticImage::name` ([basic.rs:68-70](../../src/nodes/basic.rs#L68)).

**Tests.** `tests/project_e2e.rs::first_import_asks_and_creates_a_project` (checks the saved expression).

**Notes.** Connecting replaces whatever fed the output, so importing a second image leaves the first one in the graph but detached, and resets the relocated output and selection (`pinned`, `select`). That follows the clause literally; whether it is the intended experience is a decision ([D-IMPORT](findings.md#d-import)). Imports into a layer stack use `ImportTarget::StackLayer` instead.

## F1.2 · Operator parameters: WYSIWYG or form {#f1-2}

> Operators may have parameters.
> In case they do, adding the operator node will allow the user to input the operator parameters either via a WYSIWYG interface or via form inputs.

**Status.** Implemented. Every parameter has a generated form field; the five kinds of on-canvas control cover the spatial parameters (axes, borders, rectangles, points, canvas size).

**Implemented by.**
- Parameters are declared as `ParamDef` values ([nodes/mod.rs:76-85](../../src/nodes/mod.rs#L76)) of a few `ParamKind`s ([nodes/mod.rs:53-66](../../src/nodes/mod.rs#L53)); on-canvas controls as `Overlay` values ([nodes/mod.rs:170-182](../../src/nodes/mod.rs#L170)).
- `App::add_node` selects the new node ([state.rs:487](../../src/ui/state.rs#L487)), so the inspector's `NodePanel` shows its parameters ([inspector.rs:189-198](../../src/ui/inspector.rs#L189)), rendered per kind by `ParamField` ([inspector.rs:249-401](../../src/ui/inspector.rs#L249)).
- The image view draws and drags each `Overlay` in `render_overlay` ([image_view.rs:370-510](../../src/ui/image_view.rs#L370)); drags write parameters with `set_param`, and the canvas corner writes `canvas_w`/`canvas_h` ([image_view.rs:245-286](../../src/ui/image_view.rs#L245)).

**Tests.** None for the forms or overlays (UI rendering is not tested).

**Notes.** Which nodes get WYSIWYG controls: Reflection (axis), Clip and Rectangle Split (rectangle), Vertical/Horizontal Split (line), Linear and Radial Gradient (points), and the output canvas of size-less content (corner handle). Fraction parameters show and accept pixels in the form (`ParamKind::Fraction`), converted with the extent of the space the controls live in.

## F2 · The document is a DAG {#f2}

> While an image is being edited in Delos, we will be able to visualize the final results.
> During the process of editing, however, the image will be stored as a Directed Acyclic Graph (DAG) representing image layers and operations applied to them.
> That way, we can easily see and modify effects are being applied to which parts of the image.

**Status.** Implemented.

**Implemented by.**
- `Graph` holds `nodes`, the `output` node id and the decoded `images` ([graph.rs:66-72](../../src/graph.rs#L66)); edges live on the consumer as `inputs`, an `Option<Link>` per input port ([graph.rs:41](../../src/graph.rs#L41)).
- Acyclicity: `connect` refuses an edge when `depends_on` finds a path back ([graph.rs:255-257](../../src/graph.rs#L255), [graph.rs:224-239](../../src/graph.rs#L224)). Wires drawn in the graph view go through `connect` (`connect_port`) ([graph_view.rs:115-118](../../src/ui/graph_view.rs#L115)); stack edits check the same with `depends_on` ([graph_view.rs:120-124](../../src/ui/graph_view.rs#L120)).
- Rendering pulls through the graph: `Ctx::render` asks a node's `render_output`, which renders its inputs through `render_input` ([engine/mod.rs:208](../../src/engine/mod.rs#L208), [engine/mod.rs:219-224](../../src/engine/mod.rs#L219)).

**Tests.** `tests/engine.rs::cycles_are_rejected`, `tests/engine.rs::invert_and_stack`.

**Notes.** Some writers set `inputs` directly without the cycle check (`set_stack_layers`, `insert_resize`, the expression builder). They are safe by construction today; the invariants model lists this as fragile spot 2 ([../invariants/findings.md](../invariants/findings.md)).

## F2.1 · The image view shows what goes to the output {#f2-1}

> The image view must show the image that is being sent to the output.

**Status.** Partial. By default the view shows the output, as specified. A known defect can leave a stale image on screen after an undo or a new document ([D-STALE](findings.md#d-stale)).

**Implemented by.**
- `App::displayed` falls back to `output` (or the relocated output) as `base` when nothing is selected or the view is locked, and returns it as the `target` ([state.rs:351-356](../../src/ui/state.rs#L351), [state.rs:376-378](../../src/ui/state.rs#L376), [state.rs:385](../../src/ui/state.rs#L385)).
- The image view sends a `PreviewJob` for `d.target` whenever its key changes ([image_view.rs:83-125](../../src/ui/image_view.rs#L83)). The worker renders it with latest-request-wins `run_preview` ([worker.rs:179-182](../../src/worker.rs#L179), [worker.rs:208-216](../../src/worker.rs#L208)); `pump` accepts only newer frames ([mod.rs:134-151](../../src/ui/mod.rs#L134)).

**Tests.** None for the default view (no test calls `displayed` without a relocation or selection). Rendering itself: `tests/engine.rs::invert_and_stack`.

**Notes.** The preview key is the structural hash of the target, so edits that do not change the shown image (moving nodes) do not re-render. That hash is also the root of the stale-image defect.

## F3 · Graph view, alone or beside the image view {#f3}

> We must be able to open a graph view instead of the image view or alongside the image view.
> It must also be possible to visualize any part of the graph which produces an image.

**Status.** Implemented.

**Implemented by.**
- `Layout` has `Image`, `Split` and `Graph` ([state.rs:11-16](../../src/ui/state.rs#L11)); the toolbar's segmented buttons switch it ([mod.rs:398-402](../../src/ui/mod.rs#L398)) and `Root` sets the `main_class` for the layout and shows one or both panes ([mod.rs:72-77](../../src/ui/mod.rs#L72), [mod.rs:107-115](../../src/ui/mod.rs#L107)).
- Any part of the graph: selecting a node previews it (see [F3.2](#f3-2)) and relocating the output pins it (see [F3.1](#f3-1)).

**Tests.** None for the layout switch.

## F3.1 · Relocating the output {#f3-1}

> In other terms, we can temporarily relocate the output to any image node.

**Status.** Implemented.

**Implemented by.**
- The relocated output is `App::pinned`, an `Option<Link>` ([state.rs:114-115](../../src/ui/state.rs#L114)); `displayed` uses it as the `base` while it names an existing output (`has_link`) ([state.rs:351-356](../../src/ui/state.rs#L351)).
- Ways to set `pinned`: double-click in the graph view ([F3.1.1](#f3-1-1)) and the inspector's "View as output" buttons ([inspector.rs:131-153](../../src/ui/inspector.rs#L131)). The toolbar badge's Reset clears `pinned` ([mod.rs:422](../../src/ui/mod.rs#L422)).
- Undo, redo and deletions drop a `pinned` link that no longer exists: `fix_selection` ([state.rs:291](../../src/ui/state.rs#L291), [state.rs:296-298](../../src/ui/state.rs#L296)), `delete_node` ([state.rs:531-533](../../src/ui/state.rs#L531)).

**Tests.** `tests/ui_state.rs::delete_relocated_node_resets_output`, `tests/ui_state.rs::delete_selected_node_while_output_is_relocated`, `tests/ui_state.rs::a_split_output_can_be_viewed_and_exported`.

**Notes.** "Temporarily": `pinned` is UI state, never saved in the project, and is cleared by `replace_document` when a project is opened or closed.

## F3.1.1 · Double-click to relocate and to reset {#f3-1-1}

> Double-clicking a node in the graph view relocates the output to it.
> For nodes with several outputs, double-clicking one of its output ports relocates the output to that output.
> Double-clicking the relocated node again, or the Output node, moves the output back to its default.

**Status.** Implemented.

**Implemented by.**
- The node's `ondoubleclick` toggles `pinned`: the output node or an already relocated node resets it, any other node sets its first output ([graph_view.rs:346-354](../../src/ui/graph_view.rs#L346)).
- Each output port has its own `ondoubleclick`, which pins `Link::new(id, o)` or resets it if that port is the one shown ([graph_view.rs:367-368](../../src/ui/graph_view.rs#L367), [graph_view.rs:381-385](../../src/ui/graph_view.rs#L381)).

**Tests.** None (graph-view events are not tested; `tests/ui_state.rs::a_split_output_can_be_viewed_and_exported` sets `pinned` directly).

**Notes.** Double-clicking the *body* of a split relocates to output 0 (`id.into()`), and double-clicking the body of a split pinned on output 1 resets the output instead of switching to output 0.

## F3.2 · Selecting a node previews it with its controls {#f3-2}

> If we select an operator node, the image view should show the image at the operator's output overlaid with controls for the specific operator (examples of controls: selection boxes, rules).
> Examples are given in the operators section below.

**Status.** Implemented.

**Implemented by.**
- `displayed`, when not locked, targets the selected node and sets `editing` with an identity `overlay_space` ([state.rs:357-365](../../src/ui/state.rs#L357)).
- `ImageView` draws the overlays of the `editing` node ([image_view.rs:159-160](../../src/ui/image_view.rs#L159)) with `render_overlay` ([image_view.rs:370-510](../../src/ui/image_view.rs#L370)).
- "Preview selection" is the `follow_selection` checkbox ([mod.rs:410-416](../../src/ui/mod.rs#L410)), on by default (`App::new`) ([state.rs:152](../../src/ui/state.rs#L152)).

**Tests.** `tests/ui_state.rs::previewing_a_split_shows_its_input_with_the_border`.

## F3.2.1 · Some operators are edited on their input {#f3-2-1}

> Some operators are edited on their input instead.
> For example, Clip shows the uncropped input with the crop rectangle.

**Status.** Implemented.

**Implemented by.**
- The `EditView` enum: `Output` or `Input(port)` ([nodes/mod.rs:184-192](../../src/nodes/mod.rs#L184)), returned by `NodeKind::edit_view` ([nodes/mod.rs:226-228](../../src/nodes/mod.rs#L226)).
- `Clip::edit_view` and `Split::edit_view` return `EditView::Input(0)` ([basic.rs:340-342](../../src/nodes/basic.rs#L340), [split.rs:103-106](../../src/nodes/split.rs#L103)).
- `displayed` shows the input link for `EditView::Input` ([state.rs:360-363](../../src/ui/state.rs#L360)); in the locked case it composes the kind's `input_map` so the controls land on the input's space ([state.rs:370-374](../../src/ui/state.rs#L370)).
- The inspector converts fractions with the input's extent for such kinds (`param_extent`) ([inspector.rs:63-71](../../src/ui/inspector.rs#L63)).

**Tests.** `tests/ui_state.rs::previewing_a_split_shows_its_input_with_the_border`.

## F3.3 · Keep the view fixed while editing upstream {#f3-3}

> It must be possible to keep the image view fixed while editing a node further up the graph.
> While the output is relocated (or when "Preview selection" is turned off), selecting another node keeps the view as it is and draws that node's controls over the shown image.
> Editing those controls modifies the shown image directly.

**Status.** Implemented.

**Implemented by.**
- "Preview selection" is the toolbar's `follow_selection` checkbox ([mod.rs:403](../../src/ui/mod.rs#L403), [mod.rs:410-416](../../src/ui/mod.rs#L410)). `displayed` is `locked` when `pinned` is set or `follow_selection` is off; the target stays on `base` and `overlay_space` comes from `Graph::space_map` ([state.rs:356](../../src/ui/state.rs#L356), [state.rs:366-377](../../src/ui/state.rs#L366)).
- The overlays are mapped into the shown image's coordinates through `to_frac` and `frac_to_screen` ([image_view.rs:161-165](../../src/ui/image_view.rs#L161)); the mapping is frozen while dragging (`frac_map`) ([image_view.rs:38-41](../../src/ui/image_view.rs#L38)).
- A drag writes the upstream node's parameters (`set_param`, `edit_coalesced`); the shown image's hash changes and it is re-rendered ([image_view.rs:245-276](../../src/ui/image_view.rs#L245)).

**Tests.** `tests/engine.rs::space_map_follows_geometry_upstream` (the mapping). The locked UI path has no test.

**Notes.** The edit `badge` says "Editing X · showing Y" ([image_view.rs:171-172](../../src/ui/image_view.rs#L171)).

## F3.3.1 · Controls follow geometry through intermediate nodes {#f3-3-1}

> Controls must be placed correctly even when nodes between the edited node and the shown image move or scale pixels (Clip, Resize, Downscale, …).
> Every node type describes how it maps coordinates to its inputs, so new node types can take part.

**Status.** Partial. Clip, Resize and Downscale describe their maps, and `space_map` composes them. The map model is a separable affine map along one path, so two cases are not handled: a node whose map is not affine (Reflection folds the image), and an edited node that reaches the shown image along several paths with different geometry, where only the first path is used ([D-OVERLAY](findings.md#d-overlay)).

**Implemented by.**
- The extension point: `NodeKind::input_map`, defaulting to an identity, top-left aligned map ([nodes/mod.rs:229-235](../../src/nodes/mod.rs#L229)).
- Overrides: `Clip::input_map` (offset) ([basic.rs:343-348](../../src/nodes/basic.rs#L343)), `Resize::input_map` (scale, using the rounded effective factor) ([resize.rs:73-81](../../src/nodes/resize.rs#L73)), `Downscale::input_map` ([resize.rs:238-245](../../src/nodes/resize.rs#L238)).
- Composition: `Graph::space_map` and `space_map_rec` walk the inputs depth-first, compose `Map2`s with `after`, and return the first path that reaches the edited node ([graph.rs:313-343](../../src/graph.rs#L313)).
- `Map2` is separable (no rotation or shear) ([geom.rs:95-102](../../src/engine/geom.rs#L95)).

**Tests.** `tests/engine.rs::space_map_follows_geometry_upstream`. `tests/engine.rs::clip_offsets_input` checks the rendered pixels, which supports the note below.

**Notes.** Renders use the same `input_map` (Clip, Resize and Downscale call it from `render`), so the overlay mapping and the pixels cannot disagree for these kinds.

## F3.3.2 · Say so when the selected node does not contribute {#f3-3-2}

> If the selected node does not contribute to the shown image, the view says so instead of showing controls.

**Status.** Implemented.

**Implemented by.**
- `space_map` returns `None` when the node does not feed the shown one, so `overlay_space` is `None` ([state.rs:368](../../src/ui/state.rs#L368), [state.rs:376](../../src/ui/state.rs#L376)).
- With no `overlay_space`, no overlays are drawn and the badge reads "X does not feed Y · double-click it in the graph to view it" (`badge`) ([image_view.rs:159-160](../../src/ui/image_view.rs#L159), [image_view.rs:171](../../src/ui/image_view.rs#L171), [image_view.rs:173](../../src/ui/image_view.rs#L173)).

**Tests.** `tests/engine.rs::space_map_follows_geometry_upstream` asserts `space_map` is `None` for a node that does not feed the output. The badge is untested.

## F4 · Export any operator's output; export the visible image by default {#f4}

> It must be possible to export the image output by any Delos image operator, but by default the easiest operation to perform should be to export the output currently visible in image view.

**Status.** Partial. Any operator's output can be exported, and the default is the visible image, with one deliberate exception: while a node edited on its input (Clip, splits) is previewed, the default export is that node's output, not the input shown on screen ([D-EXPORT-TARGET](findings.md#d-export-target)).

**Implemented by.**
- Default: the toolbar's Export button and ⌘E call `open_export(None)` ([mod.rs:387-393](../../src/ui/mod.rs#L387), [mod.rs:294](../../src/ui/mod.rs#L294)), which exports `displayed().export_target` ([state.rs:392-397](../../src/ui/state.rs#L392)) by opening the dialog on that `target` (`ExportState`) ([state.rs:407-408](../../src/ui/state.rs#L407)).
- Any operator: every non-output node's inspector has one Export button; for a split it exports the output being viewed, else output 0 (`export_link`) ([inspector.rs:87-90](../../src/ui/inspector.rs#L87), [inspector.rs:154](../../src/ui/inspector.rs#L154)). To export a split's second output, view it first.
- `export_target` is the edited node when not locked, else the shown target ([state.rs:380](../../src/ui/state.rs#L380)).

**Tests.** None for the default target or the inspector button; `tests/ui_state.rs::a_split_output_can_be_viewed_and_exported` covers the relocated case ([F4.1](#f4-1)).

## F4.1 · Export follows the relocated output {#f4-1}

> While the output is relocated, exporting the current image will export the content of the node that is currently attached to output.

**Status.** Implemented.

**Implemented by.**
- When `pinned` is set, `displayed` is `locked` and `export_target` is the pinned link, returned in `Displayed` ([state.rs:353-354](../../src/ui/state.rs#L353), [state.rs:356](../../src/ui/state.rs#L356), [state.rs:380](../../src/ui/state.rs#L380), [state.rs:385](../../src/ui/state.rs#L385)).
- The export job carries that `Link`, so a split's specific output is exported: `ExportJob` ([worker.rs:28](../../src/worker.rs#L28), [worker.rs:31](../../src/worker.rs#L31)).

**Tests.** `tests/ui_state.rs::a_split_output_can_be_viewed_and_exported`.

## F4.2 · Banded, bounded, cancellable export and its dialog {#f4-2}

> Export renders at full resolution in bands (to bound memory), shows progress and can be cancelled.
> The export dialog can be closed with its close button, the Escape key or by clicking outside it, and closes by itself after a successful export.

**Status.** Partial. Rendering is banded for every format, but only the PNG encoder streams bands. The other encoders collect the whole RGBA8 image before encoding, and exports to memory (the web, and the mobile fallback) hold the whole encoded file, so memory is bounded only for PNG written to a file ([D-EXPORT-MEMORY](findings.md#d-export-memory)). The dialog behaviour is as specified.

**Implemented by.**
- Bands: `GraphBands::new` picks about 4 Mpx per band and `next_band` renders each with `exact` requests ([formats.rs:230-233](../../src/formats.rs#L230), [formats.rs:244-256](../../src/formats.rs#L244)). On native targets `export` renders on another thread with a one-band channel; elsewhere bands render inline ([formats.rs:259-300](../../src/formats.rs#L259)). `ExportDest::Memory` collects the encoded file in a `Vec` ([worker.rs:311-315](../../src/worker.rs#L311)).
- Streaming PNG: `Png::encode` writes each band to a `stream_writer` ([formats.rs:135-149](../../src/formats.rs#L135)).
- Progress: `Prefetched::next_band` reports the fraction ([formats.rs:320-327](../../src/formats.rs#L320)), forwarded as `Msg::ExportProgress` by the `progress` callback that `Worker::export` passes to `formats::export` ([worker.rs:289-291](../../src/worker.rs#L289), [worker.rs:303](../../src/worker.rs#L303)), stored by `pump` ([mod.rs:188-193](../../src/ui/mod.rs#L188), [mod.rs:197-198](../../src/ui/mod.rs#L197)) and drawn as a `progress` bar ([dialogs.rs:170-173](../../src/ui/dialogs.rs#L170)).
- Cancel: `Worker::export` registers a `cancel_flag` per job and passes it as `cancel` ([worker.rs:281-282](../../src/worker.rs#L281), [worker.rs:287](../../src/worker.rs#L287)); `cancel_export` sets the job's flag ([worker.rs:392-396](../../src/worker.rs#L392)), which `Ctx::render` and `Ctx::kernel` check through `cancelled` and stop with `Cancelled` ([engine/mod.rs:179-181](../../src/engine/mod.rs#L179), [engine/mod.rs:201-203](../../src/engine/mod.rs#L201), [engine/mod.rs:275-277](../../src/engine/mod.rs#L275)); a partial file is deleted (`remove_file`) ([worker.rs:304-307](../../src/worker.rs#L304)).
- Closing: the × button and Close, both `export.set(None)` ([dialogs.rs:124-126](../../src/ui/dialogs.rs#L124), [dialogs.rs:194](../../src/ui/dialogs.rs#L194)), `Key::Escape` when no export runs ([mod.rs:297-304](../../src/ui/mod.rs#L297)), a click on the backdrop that also started there (`export_backdrop_clicked`) ([state.rs:436-440](../../src/ui/state.rs#L436)). `pump` closes the dialog after `ExportDone` succeeds ([mod.rs:199](../../src/ui/mod.rs#L199), [mod.rs:207-210](../../src/ui/mod.rs#L207)).

**Tests.** `tests/export.rs::export_matches_sequential_render`, `tests/export.rs::cancelling_stops_the_export`, `tests/export.rs::write_errors_stop_the_export`, `tests/large_images.rs::export_16k_streams_in_bands`, `tests/large_images.rs::export_16k_png`, `tests/ui_state.rs::dragging_out_of_the_export_dialog_keeps_it_open`.

**Notes.** The shortfall: other formats go through `ImageCrate::encode`, which appends every band to `all` first ([formats.rs:83-88](../../src/formats.rs#L83)).


## F5 · Combination through operators, with display modalities {#f5}

> All combinations of images will be done using operators.
> For example, rather than stacking layers by default, Delos should make it possible to combine layers with specific operators.
> Delos shall define specific "display modalities" for standard operators to make user experience more pleasurable - for example, if we combine a sequence of layers using the "stack" operator, we can display this as a layer stack rather than as a degenerate graph of the format `(layer3 STACK (layer2 STACK layer1))` (i.e. a Lisp list of stacks, with the exception that it will never be empty hence a NIL-node is not required.)

**Status.** Partial. All combination is done by operator nodes, and the layer stack has its display modality in the graph view, the inspector and the expression format. No other operator has one: the spec says modalities (plural) for standard operators, but only gives the stack as an example ([D-MODALITIES](findings.md#d-modalities)).

**Implemented by.**
- Combining kinds are ordinary nodes (`Stack`, `Blend`) with `combines_inputs` ([basic.rs:114-133](../../src/nodes/basic.rs#L114), [basic.rs:213-234](../../src/nodes/basic.rs#L213)).
- Chains: `is_interior_stack`, `chain_head` and `stack_chain` recognise a chain of stacks linked through the back port ([graph.rs:426-465](../../src/graph.rs#L426)); `set_stack_layers` rewrites it from a layer list ([graph.rs:470-491](../../src/graph.rs#L470)).
- Graph view: interior stacks are hidden (`is_interior_stack`) and the head shows one row per layer plus "+ add layer" (`stack_chain`) ([graph_view.rs:66-68](../../src/ui/graph_view.rs#L66), [graph_view.rs:75-87](../../src/ui/graph_view.rs#L75)); a wire dropped on a row replaces that layer and one dropped on "+ add layer" appends it, through `set_stack_layers` (`connect_port`) ([graph_view.rs:125-132](../../src/ui/graph_view.rs#L125)).
- Inspector: the `LayerStack` editor lists the chain's layers front to back ([inspector.rs:404-421](../../src/ui/inspector.rs#L404), [inspector.rs:439-443](../../src/ui/inspector.rs#L439), [inspector.rs:464-468](../../src/ui/inspector.rs#L464)).
- Expression format: `(stack front … back)` (see [E4](05-expressions.md#e4)).

**Tests.** `tests/engine.rs::stack_chain_editing`, `src/expr.rs::stacks_are_lists_of_layers`.

**Notes.** "Never empty": a chain always has at least one stack node (`needed` is at least 1 in `set_stack_layers`), but layers can be empty slots ("+ Empty slot" in the inspector).
