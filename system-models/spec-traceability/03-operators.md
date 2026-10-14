# 3 · Operators, splits, generators and sizes

The spec's *Basic Operators*, *Split operators*, *Generative operators* and
*Handling image size differences*. Every operator is a `NodeKind`
registered in `nodes::registry()`; the per-pixel work is a `Kernel` with a
WGSL and a Rust version.

## O0 · The basic operator set {#o0}

> These are basic layer operators which must be implemented in the initial version of Delos:

**Status.** Implemented. All nine are registered.

**Implemented by.**
- `basic::kinds` lists Output, Static Image, Stack, Disable, Invert, Blend, Reflection and Clip ([basic.rs:10-12](../../src/nodes/basic.rs#L10)); `resize::kinds` lists Resize and Downscale ([resize.rs:10-12](../../src/nodes/resize.rs#L10)); `bgremove::kinds` lists Background Removal ([bgremove.rs:18-20](../../src/nodes/bgremove.rs#L18)).
- The `REGISTRY` collects them ([nodes/mod.rs:251-259](../../src/nodes/mod.rs#L251)); the "Add node" menu lists every `user_creatable` kind by category ([mod.rs:325-332](../../src/ui/mod.rs#L325)).

**Tests.** `tests/engine.rs::every_blend_mode_renders`, `tests/engine.rs::invert_and_stack`, `tests/engine.rs::clip_offsets_input`, `tests/engine.rs::reflection_mirrors_pixels`. No test renders Downscale or Background Removal.

## O1 · Stack: over-compositing of a front/back chain {#o1}

> Stack operator - this is a basic operator which is used to form layer stacks.
> A layer stack will be internally represented as `(frontmost STACK (second-frontmost STACK (third-frontmost STACK ... (second-backmost STACK last) ... )))`.
> A layer stack will show the first non-transparent pixel found along the stack for each position.
> Delos implements this as standard "over" alpha compositing: this matches the rule for opaque pixels and also handles partly transparent ones.
> Takes no parameters.

**Status.** Implemented.

**Implemented by.**
- `Stack` has the inputs `front` and `back` and no `params` ([basic.rs:114-116](../../src/nodes/basic.rs#L114)). A chain links each stack's `back` to the next stack ([graph.rs:441-465](../../src/graph.rs#L441)).
- `Stack::render` returns one side when the other is clear, else runs `OVER` ([basic.rs:120-130](../../src/nodes/basic.rs#L120)); `OVER` is Porter-Duff over on premultiplied pixels, front over back ([kernels.rs:228-242](../../src/engine/kernels.rs#L228)).

**Tests.** `tests/engine.rs::invert_and_stack`, `tests/engine.rs::stack_chain_editing`, `tests/split.rs::parts_stack_back_into_the_original`.

**Notes.** `Stack::size` is the union of the input sizes, top-left aligned (`union_size`) ([basic.rs:117-119](../../src/nodes/basic.rs#L117), [basic.rs:20-24](../../src/nodes/basic.rs#L20)).

## O1.1 · Editing a stack: disable, remove {#o1-1}

> If the stack operator is selected it should allow editing the stack, for example temporarily disabling inputs (which will be internally represented as a "Disable" operator which changes all pixels to transparent) or removing them completely.

**Status.** Implemented. Layers can also be reordered and added (image, generator, empty slot).

**Implemented by.**
- `LayerStack` in the inspector lists the layers, marking disabled ones (`ids::DISABLE`), and edits them through `set_layers` ([inspector.rs:404-422](../../src/ui/inspector.rs#L404), [inspector.rs:428-436](../../src/ui/inspector.rs#L428)). The eye button wraps the layer in a new `ids::DISABLE` node or unwraps it ([inspector.rs:444-463](../../src/ui/inspector.rs#L444)); ✕ removes the layer from the list ([inspector.rs:483-488](../../src/ui/inspector.rs#L483)).
- Every change goes through `set_stack_layers`, which reuses, adds or removes stack nodes and keeps the head's id ([graph.rs:470-491](../../src/graph.rs#L470)).
- In the graph view, removing a layer's edge removes the layer (`disconnect_ref`) ([graph_view.rs:166-174](../../src/ui/graph_view.rs#L166)).

**Tests.** `tests/engine.rs::stack_chain_editing`.

**Notes.** Re-enabling deletes the Disable node with `remove_node`. The disabled layer's title is shown greyed (the `enabled` flag). ▲/▼ swap layers (`l.swap`) ([inspector.rs:469-482](../../src/ui/inspector.rs#L469)). "+ Image layer", "+ Generator layer…" and "+ Empty slot" add layers (`ImportTarget::StackLayer`, `l.insert`, `l.push`) ([inspector.rs:492-523](../../src/ui/inspector.rs#L492)). In the graph view, dropping a wire on a layer row or "+ add layer" edits the chain (`connect_port`) ([graph_view.rs:120-133](../../src/ui/graph_view.rs#L120)).

## O2 · Disable {#o2}

> Disable operator - mentioned with stack above.

**Status.** Implemented.

**Implemented by.**
- `Disable` keeps its input's size (`inputs[0]`) and renders `Tile::Clear` without rendering the input ([basic.rs:156-161](../../src/nodes/basic.rs#L156)).

**Tests.** `tests/engine.rs::invert_and_stack` (a disabled front layer lets the back layer through). The inspector's toggle is untested.

## O3 · Simple reflection {#o3}

> Simple reflection operator - this should allow making reflections, i.e. placing axes of reflection on the image which will copy the pixels to one side of them to the other side.
> This tool, when selected, will allow (via WYSIWYG rule or form input) selecting where to position the axis of reflection.
> Via form, it should allow selecting a vertical or horizontal axis of reflection.
> It should also allow specifying whether the reflection should be additive, subtractive or replace the pixels where the reflection will fall.

**Status.** Implemented.

**Implemented by.**
- `Reflect::params`: `orientation` (vertical/horizontal; `ORIENTATIONS`) ([basic.rs:241](../../src/nodes/basic.rs#L241), [basic.rs:263](../../src/nodes/basic.rs#L263)), `position` (a fraction) ([basic.rs:264](../../src/nodes/basic.rs#L264)) and `mode` (replace/additive/subtractive; `REFLECT_MODES`) ([basic.rs:243](../../src/nodes/basic.rs#L243), [basic.rs:266](../../src/nodes/basic.rs#L266)).
- The WYSIWYG rule: `Overlay::Axis` ([basic.rs:272-274](../../src/nodes/basic.rs#L272)), drawn and dragged by `render_overlay` ([image_view.rs:384-411](../../src/ui/image_view.rs#L384)).
- `Reflect::render` renders the input directly and once more through the `mirror` map, only when the destination interval meets the request ([basic.rs:275-301](../../src/nodes/basic.rs#L275)); `REFLECT` picks per pixel and applies the mode ([kernels.rs:290-335](../../src/engine/kernels.rs#L290)).

**Tests.** `tests/engine.rs::reflection_mirrors_pixels` (replace mode, vertical axis only), `tests/engine.rs::gpu_matches_cpu` (parity, includes additive). Subtractive and the horizontal axis are untested.

**Notes.** The `side` parameter (copy left→right or right→left; `SIDES`) is not in the spec ([basic.rs:242](../../src/nodes/basic.rs#L242), [basic.rs:265](../../src/nodes/basic.rs#L265)). Subtractive keeps the destination's alpha; additive clamps in premultiplied space.

## O4 · Downscale {#o4}

> Downscale operator - should allow for downscaling

**Status.** Partial. Downscaling works, but "box" is not a box over the chosen factor: below a factor of 2 it samples like nearest, above it uses a fixed power-of-two pyramid level, and size-less inputs are point-sampled ([D-RESIZE](findings.md#d-resize)).

**Implemented by.**
- `Downscale` with `factor` (1 to 64) ([resize.rs:230-232](../../src/nodes/resize.rs#L230)). Its `size` floors the scaled size ([resize.rs:233-237](../../src/nodes/resize.rs#L233)); `render` maps the request onto the input with `input_map` ([resize.rs:238-248](../../src/nodes/resize.rs#L238), [resize.rs:252-253](../../src/nodes/resize.rs#L252)).
- Box averaging comes from the source pyramid: `level_for` picks the level whose texels match the request ([source.rs:114-121](../../src/engine/source.rs#L114)).

**Tests.** None.

**Notes.** A `method` choice (box average or nearest, `DOWN_METHODS`) is not in the spec ([resize.rs:210](../../src/nodes/resize.rs#L210), [resize.rs:231](../../src/nodes/resize.rs#L231)); `render` uses `Sampling::Point` for nearest ([resize.rs:249-251](../../src/nodes/resize.rs#L249)). For imported images, `level_for` picks the level `floor(log2(step))`, so for steps below 2 it is level 0, sampled nearest ([source.rs:114-133](../../src/engine/source.rs#L114)); above that it samples the chosen power-of-two level bilinearly.

## O5 · Background removal with rembg-rs {#o5}

> Background removal operator - this should allow removing the input image's background using the `rembg-rs` crate.
> Controls set the background removal parameters.

**Status.** Partial. Implemented with `rembg-rs` on desktop. The web and mobile builds compile a stub that reports an error, and on desktop the user must supply the ONNX model and the ONNX Runtime library ([D-PLAT](findings.md#d-plat)).

**Implemented by.**
- `BackgroundRemoval::params`: `threshold`, `binary`, `sticker` (the `rembg-rs` `RemovalOptions`) ([bgremove.rs:42-46](../../src/nodes/bgremove.rs#L42)).
- `render` renders the input over its whole extent (`src_req`), calls `compute_mask`, and applies the mask with the `MASK` kernel at any resolution ([bgremove.rs:62](../../src/nodes/bgremove.rs#L62), [bgremove.rs:77-81](../../src/nodes/bgremove.rs#L77), [bgremove.rs:83](../../src/nodes/bgremove.rs#L83), [bgremove.rs:89-97](../../src/nodes/bgremove.rs#L89)).
- `compute_mask` calls `rembg_rs::rembg` with a shared `ModelManager` ([bgremove.rs:101-126](../../src/nodes/bgremove.rs#L101), [bgremove.rs:133-139](../../src/nodes/bgremove.rs#L133)).
- Without the `rembg` feature, `compute_mask` bails ([bgremove.rs:169-172](../../src/nodes/bgremove.rs#L169)); `rembg` is only in the `desktop` feature ([Cargo.toml:8](../../Cargo.toml#L8)).

**Tests.** None (no model is available to the test suite).

**Notes.** Extra parameters `invert`, `resolution` and `model` ([bgremove.rs:47-49](../../src/nodes/bgremove.rs#L47)). `render` computes the mask once per input hash and settings, at the chosen `resolution`, and caches it in `engine.cache` ([bgremove.rs:64-77](../../src/nodes/bgremove.rs#L64), [bgremove.rs:84-85](../../src/nodes/bgremove.rs#L84)). `compute_mask` turns a missing ONNX Runtime into an error message (`catch_unwind`) ([bgremove.rs:127-132](../../src/nodes/bgremove.rs#L127)); `resolve_model` searches the parameter, `DELOS_REMBG_MODEL` and `~/.delos/models` ([bgremove.rs:146-167](../../src/nodes/bgremove.rs#L146)). The mask cache key builds on the structural hash, so it inherits the stale-identity defect ([D-STALE](findings.md#d-stale)).

## O6 · Invert {#o6}

> Invert operator - inverts colors.
> (i.e. (R,G,B) -> (1-R,1-G,1-B)) with 0 <= R, G, B <= 1.
> No controls.

**Status.** Implemented.

**Implemented by.**
- `Invert` has no `params` and its `render` runs `INVERT` ([basic.rs:187-193](../../src/nodes/basic.rs#L187)).
- `INVERT` computes `a − c` per channel on premultiplied pixels, which is `a·(1 − C)` for the straight colour `C`: the spec's formula, with alpha kept ([kernels.rs:215-226](../../src/engine/kernels.rs#L215)).

**Tests.** `tests/engine.rs::invert_and_stack`, `tests/engine.rs::gpu_matches_cpu`.

**Notes.** Values are sRGB-encoded, not linear (the `tile.rs` header); the spec does not say which.

## O7 · Clip {#o7}

> Clip - allows selecting a subset of image.

**Status.** Implemented.

**Implemented by.**
- `Clip` keeps the rectangle `x`, `y`, `w`, `h` (fractions of the input) ([basic.rs:324-331](../../src/nodes/basic.rs#L324)) with an `Overlay::Rect` and is edited on its input (`EditView::Input`) ([basic.rs:337-342](../../src/nodes/basic.rs#L337)). `size` rounds to whole pixels ([basic.rs:332-336](../../src/nodes/basic.rs#L332)) and `render` maps the request with `input_map` ([basic.rs:343-352](../../src/nodes/basic.rs#L343)).

**Tests.** `tests/engine.rs::clip_offsets_input`.

## O8 · Resize with selectable interpolation {#o8}

> Resize - allows upscaling or downscaling input.
> Allows choosing upscale/downscale algorithm to use.
> Should offer nearest-neighbor, bilinear, bicubic and fourier-based interpolation.

**Status.** Partial. All four filters are offered and used when upscaling. When downscaling on both axes, bilinear, bicubic and Fourier all give the same area-averaged result, so the choice only matters between "nearest" and the rest ([D-RESIZE](findings.md#d-resize)).

**Implemented by.**
- `FILTERS` and `Resize::params` (`scale` and `filter`) ([resize.rs:15](../../src/nodes/resize.rs#L15), [resize.rs:55-56](../../src/nodes/resize.rs#L55), [resize.rs:58](../../src/nodes/resize.rs#L58), [resize.rs:61-63](../../src/nodes/resize.rs#L61)).
- `Resize::render`: nearest uses `Sampling::Point` ([resize.rs:88-91](../../src/nodes/resize.rs#L88)); coarse grids and size-less inputs return the area-filtered input regardless of filter ([resize.rs:92-98](../../src/nodes/resize.rs#L92)); upsampling fetches native pixels and runs `RESAMPLE` with bilinear or bicubic sampling ([resize.rs:99-103](../../src/nodes/resize.rs#L99), [resize.rs:107-108](../../src/nodes/resize.rs#L107), [resize.rs:119-120](../../src/nodes/resize.rs#L119)).
- Fourier: `render` calls `fourier_resample` and resamples its result ([resize.rs:109](../../src/nodes/resize.rs#L109), [resize.rs:111-113](../../src/nodes/resize.rs#L111), [resize.rs:115-118](../../src/nodes/resize.rs#L115)); `fourier_resample` zero-pads or truncates the spectrum, rows then columns ([resize.rs:124-206](../../src/nodes/resize.rs#L124)).
- Bicubic is Catmull-Rom (`catmull_rom`) in both backends ([tile.rs:109-120](../../src/engine/tile.rs#L109)).

**Tests.** `tests/engine.rs::resize_nearest_is_exact_and_fourier_is_close`.

**Notes.** Upscaling also falls back to the area path when the native cover exceeds four times the output, with a floor of 65,536 pixels (`native.pixel_count()`), so very large upscales of large regions ignore the filter too ([resize.rs:104-106](../../src/nodes/resize.rs#L104)). A `mode` choice between a scale factor and an exact `width`/`height` (`MODES`) is not in the spec ([resize.rs:14](../../src/nodes/resize.rs#L14), [resize.rs:57](../../src/nodes/resize.rs#L57), [resize.rs:59-60](../../src/nodes/resize.rs#L59)).

## O9 · Blend with the Wikipedia blend modes {#o9}

> Blend operator - allows combining images with the blend modes specified at https://en.wikipedia.org/wiki/Blend_modes plus the following:

**Status.** Implemented. Every mode the article names is present: normal, dissolve, multiply, screen, overlay, hard light, soft light, colour dodge and burn, linear dodge (addition) and burn, divide, subtract, difference, darken only, lighten only, hue, saturation, colour and luminosity. The code adds the modes the article mentions in passing or Photoshop defines (vivid, linear and pin light, hard mix, exclusion), and an `opacity` parameter.

**Implemented by.**
- `BLEND_MODES` lists `normal` to `linear_burn` ([kernels.rs:341-352](../../src/engine/kernels.rs#L341)), `divide`, `subtract`, `difference` ([kernels.rs:357-359](../../src/engine/kernels.rs#L357)) and `darken` to `luminosity` ([kernels.rs:361-366](../../src/engine/kernels.rs#L361)); `Blend::params` (`mode`) and `render` ([basic.rs:216-218](../../src/nodes/basic.rs#L216), [basic.rs:222-228](../../src/nodes/basic.rs#L222), [basic.rs:230-231](../../src/nodes/basic.rs#L230)).
- Separable modes: `blend_channel` ([kernels.rs:413-423](../../src/engine/kernels.rs#L413), [kernels.rs:432-436](../../src/engine/kernels.rs#L432), [kernels.rs:438-439](../../src/engine/kernels.rs#L438), [kernels.rs:443-445](../../src/engine/kernels.rs#L443)); non-separable modes: `blend_color` with `set_lum` and `set_sat` ([kernels.rs:468-493](../../src/engine/kernels.rs#L468)); the GPU versions of the same modes are WGSL helpers in `BLEND` (`screen1` to `soft_light`, `blend_channel`, `blend_color` with `set_lum` and `set_sat`) ([kernels.rs:501-521](../../src/engine/kernels.rs#L501), [kernels.rs:534-544](../../src/engine/kernels.rs#L534), [kernels.rs:555-560](../../src/engine/kernels.rs#L555), [kernels.rs:562-563](../../src/engine/kernels.rs#L562), [kernels.rs:567](../../src/engine/kernels.rs#L567), [kernels.rs:570-599](../../src/engine/kernels.rs#L570)); compositing with the backdrop's alpha in `BLEND`'s WGSL `kernel` ([kernels.rs:600-617](../../src/engine/kernels.rs#L600)) and `cpu` closure ([kernels.rs:619-646](../../src/engine/kernels.rs#L619)).

**Tests.** `tests/engine.rs::every_blend_mode_renders`, `tests/engine.rs::gpu_matches_cpu`.

**Notes.** Extra modes `vivid_light`, `linear_light`, `pin_light`, `hard_mix` ([kernels.rs:353-356](../../src/engine/kernels.rs#L353)) and `exclusion` ([kernels.rs:360](../../src/engine/kernels.rs#L360)), computed in `blend_channel` ([kernels.rs:424-431](../../src/engine/kernels.rs#L424), [kernels.rs:437](../../src/engine/kernels.rs#L437)) with the `vivid_light` helper ([kernels.rs:397-400](../../src/engine/kernels.rs#L397)), and in the WGSL ([kernels.rs:522-525](../../src/engine/kernels.rs#L522), [kernels.rs:545-554](../../src/engine/kernels.rs#L545), [kernels.rs:561](../../src/engine/kernels.rs#L561), exclusion is `case 18u`). The `opacity` parameter ([basic.rs:217](../../src/nodes/basic.rs#L217), [basic.rs:229](../../src/nodes/basic.rs#L229)). Soft light uses the W3C formula (the article lists several). The mode list was checked against the article on 2026-10-14.

## O10 · Logic-gate blend modes {#o10}

> Logic Gate: applies logic operation (and, or, xor) to both images.
> Note that nand, nor and xnor are not included bc they can be achieved with the invert operator.

**Status.** Implemented.

**Implemented by.**
- `and`, `or` and `xor` are entries 25 to 27 of `BLEND_MODES` ([kernels.rs:367-369](../../src/engine/kernels.rs#L367)); `logic` quantises each straight colour channel to 8 bits and applies the operation ([kernels.rs:402-411](../../src/engine/kernels.rs#L402)); `blend_channel` dispatches the three modes to it ([kernels.rs:440-442](../../src/engine/kernels.rs#L440)), and the WGSL `logic` and `blend_channel` do the same on the GPU ([kernels.rs:526-533](../../src/engine/kernels.rs#L526), [kernels.rs:564-566](../../src/engine/kernels.rs#L564)).

**Tests.** `tests/engine.rs::every_blend_mode_renders`.

**Notes.** The gates apply to colour only; alpha is composited like any other blend mode.

## S1 · Splits produce two complementary outputs {#s1}

> Splitting an image produces two output images, one for each side of a split border.
> One output contains the pixels on one side of the border, with every pixel on the other side transparent.
> The other output contains the complementary pixels, with those present in the first output transparent.
> This makes localized effects possible, as selections do in a regular editor.

**Status.** Implemented.

**Implemented by.**
- `Split` kinds declare two `outputs`; output 0 keeps the `region`, output 1 the rest ([split.rs:21-31](../../src/nodes/split.rs#L21)). `render_output` short-cuts tiles wholly on one side and otherwise runs `SPLIT` with `keep_inside` ([split.rs:110-131](../../src/nodes/split.rs#L110)).
- `SPLIT` keeps the input pixel or returns transparent ([kernels.rs:267-288](../../src/engine/kernels.rs#L267)).

**Tests.** `tests/split.rs::vertical_split_partitions_at_the_border`, `tests/split.rs::localized_effect_on_one_side`.

## S2 · Vertical, horizontal and rectangle splits and their controls {#s2}

> Vertical Split (`left` / `right`), Horizontal Split (`top` / `bottom`) and Rectangle Split (`inside` / `outside`) are provided.
> Each is controlled in the image view by dragging the border line, or by moving and resizing the rectangle, or via form inputs.

**Status.** Implemented.

**Implemented by.**
- `VERTICAL`, `HORIZONTAL` and `RECTANGLE` with their `outputs`, fraction `params` and `overlays` ([split.rs:33-73](../../src/nodes/split.rs#L33)).
- `Overlay::Line` is drawn and dragged (`Drag::Axis`) ([image_view.rs:412-418](../../src/ui/image_view.rs#L412), [image_view.rs:427-441](../../src/ui/image_view.rs#L427)); `Overlay::Rect` is moved by its body (`RectMove`) and resized by its corners (`RectCorner`) ([image_view.rs:467-470](../../src/ui/image_view.rs#L467), [image_view.rs:478-504](../../src/ui/image_view.rs#L478)).

**Tests.** `tests/split.rs::splits_are_registered_with_two_outputs` (registration only; the controls are untested).

**Notes.** The line overlay shows side labels (`b_text`, `a_text`) ([image_view.rs:420-426](../../src/ui/image_view.rs#L420), [image_view.rs:442-444](../../src/ui/image_view.rs#L442)); the rectangle overlay shades the outside and shows its size (`shade`, `label`) ([image_view.rs:471-473](../../src/ui/image_view.rs#L471), [image_view.rs:477](../../src/ui/image_view.rs#L477), [image_view.rs:505](../../src/ui/image_view.rs#L505)).

## S3 · Exact partition by pixel centre {#s3}

> Exact partition: every pixel belongs to exactly one output, decided by its centre.
> Stacking the two outputs therefore reproduces the input exactly, at any zoom level.

**Status.** Implemented.

**Implemented by.**
- The border is rounded to whole node units in each `region` function ([split.rs:40](../../src/nodes/split.rs#L40), [split.rs:50](../../src/nodes/split.rs#L50), [split.rs:67-72](../../src/nodes/split.rs#L67)).
- `render_output` and `SPLIT` test the pixel centre against the half-open rectangle (`contains`, `inside`) ([split.rs:113-119](../../src/nodes/split.rs#L113), [kernels.rs:274-287](../../src/engine/kernels.rs#L274)).

**Tests.** `tests/split.rs::parts_stack_back_into_the_original`, `tests/split.rs::borders_at_the_edges_give_everything_to_one_side`, `tests/split.rs::rectangle_split_partitions_inside_and_outside`, `tests/split.rs::gpu_split_matches_cpu`.

## S4 · Editing view of a split {#s4}

> Editing view: when a split is selected, the image view shows its whole input with the border controls.

**Status.** Implemented.

**Implemented by.**
- `Split::edit_view` returns `EditView::Input(0)` ([split.rs:103-106](../../src/nodes/split.rs#L103)), handled by `displayed` (see [F3.2.1](02-editing-and-views.md#f3-2-1)).

**Tests.** `tests/ui_state.rs::previewing_a_split_shows_its_input_with_the_border`.

## S5 · Specific outputs as inputs, relocation and export targets {#s5}

> Outputs: the inputs of other nodes, the relocated output and exports can each refer to a specific output of a split.

**Status.** Partial. The data model supports it everywhere, but "Add node" with a split selected rewires the consumers of its second output onto its first ([D-ADD-AFTER-SPLIT](findings.md#d-add-after-split)).

**Implemented by.**
- Edges are `Link { node, output }` ([graph.rs:15-27](../../src/graph.rs#L15)); `connect` checks the output exists ([graph.rs:252-254](../../src/graph.rs#L252)).
- The relocated output and export targets are `Link`s: `pinned`, `ExportState::target` ([state.rs:53-54](../../src/ui/state.rs#L53), [state.rs:114-115](../../src/ui/state.rs#L114)).
- The graph view draws one port per output and starts wires from it (`Link::new`) ([graph_view.rs:364-392](../../src/ui/graph_view.rs#L364)); the inspector's input pickers list every output (`candidates`) ([inspector.rs:75-82](../../src/ui/inspector.rs#L75)).
- Structural hashing distinguishes outputs: `link_hash` ([graph.rs:408-416](../../src/graph.rs#L408)).

**Tests.** `tests/split.rs::outputs_are_distinct_links`, `tests/ui_state.rs::a_split_output_can_be_viewed_and_exported`, `tests/ui_state.rs::deleting_a_split_reconnects_both_sides_to_its_input`.

**Notes.** The defect: `App::add_node` reconnects all `consumers` of the anchor and feeds the new node output 0 ([state.rs:470-476](../../src/ui/state.rs#L470)).


## S6 · Split sizes {#s6}

> Size: splits keep their input's size, and splitting a size-less (generative) input splits it at whatever size is requested.

**Status.** Implemented.

**Implemented by.**
- `Split::size` returns the input size ([split.rs:97-99](../../src/nodes/split.rs#L97)); `region` is computed from `req.extent`, which is the requested extent for size-less inputs ([split.rs:111](../../src/nodes/split.rs#L111)).

**Tests.** `tests/split.rs::size_less_inputs_are_split_at_the_requested_extent`.

## S7 · Any number of outputs per node {#s7}

> Extensibility: nodes in general may have any number of outputs.
> Each node type declares its outputs, and every output of a node has the node's size.

**Status.** Implemented.

**Implemented by.**
- `NodeKind::outputs` (default one, "image") and `render_output` ([nodes/mod.rs:204-208](../../src/nodes/mod.rs#L204), [nodes/mod.rs:217-222](../../src/nodes/mod.rs#L217)).
- One size per node: `Graph::size_of` ("all outputs of a node share it") ([graph.rs:284-287](../../src/graph.rs#L284)).

**Tests.** `tests/split.rs::outputs_are_distinct_links`, `tests/project.rs::projects_with_splits_round_trip`.

**Notes.** Output names in titles and expressions: `link_title` ([graph.rs:151-160](../../src/graph.rs#L151)), `Builder::reference` ([expr.rs:540-555](../../src/expr.rs#L540)). Kernels have at most three inputs (`MAX_INPUTS`); that limits inputs, not outputs.

## N1 · Generators take no inputs {#n1}

> These operators are differentiated by not taking any inputs.
> Rather, they should act as generators of data which can be input to other generators.

**Status.** Implemented. Generators have no input ports. The second sentence cannot hold literally, since generators take no inputs; the code reads it as "input to other operators" ([D-SPEC-WORDING](findings.md#d-spec-wording)).

**Implemented by.**
- `Generator` does not override `inputs`, so it has none; its category is `Category::Generator` ([generators.rs:21-46](../../src/nodes/generators.rs#L21)).

**Tests.** `tests/engine.rs::gpu_matches_cpu` (renders each generator kernel).

## N2 · Size-less generators and the output canvas {#n2}

> Note that generative operators do not have output size - if they are connected directly to output, the user must specify the desired output size (this should be possible to do via WYSIWYG).
> As said above, generative operators should generate at the size required of them and should not specify an output size.

**Status.** Implemented.

**Implemented by.**
- `NodeKind::size` returns an `Option`, where `None` means size-less ([nodes/mod.rs:212-214](../../src/nodes/mod.rs#L212)); `Generator::size` returns `None` ([generators.rs:37-39](../../src/nodes/generators.rs#L37)); generator kernels normalise coordinates against the requested `extent` (kernel header) ([kernels.rs:649-652](../../src/engine/kernels.rs#L649)).
- The output's `canvas_w` and `canvas_h` give the size when its input is size-less: `Output::params` and `Output::size` ([basic.rs:46-51](../../src/nodes/basic.rs#L46)), `Graph::canvas` and `extent_of` ([graph.rs:301-308](../../src/graph.rs#L301)).
- WYSIWYG: a corner handle on size-less content (`canvas-handle`) ([image_view.rs:305-322](../../src/ui/image_view.rs#L305)) that sets the canvas while dragged (`Drag::Canvas`) ([image_view.rs:277-286](../../src/ui/image_view.rs#L277)). Form: the Output node's integer fields.

**Tests.** `tests/large_images.rs::export_16k_streams_in_bands` (a generator exported at a set canvas).

## N3 · Default canvas fits the window {#n3}

> The output size which fits naturally into the current Delos window should be selected by default.

**Status.** Implemented.

**Implemented by.**
- `fit_canvas_to_window` sets the canvas to 92% of the image viewport unless the user has set it (`canvas_user_set`) ([state.rs:323-344](../../src/ui/state.rs#L323)).
- Called when a generator is added (`add_node`) ([state.rs:484-486](../../src/ui/state.rs#L484)) and whenever size-less content is shown (`sizeless`) ([image_view.rs:69-76](../../src/ui/image_view.rs#L69)).

**Tests.** None.

**Notes.** "Canvas = window size" forces it with `fit_canvas_to_window(true)` ([inspector.rs:199-208](../../src/ui/inspector.rs#L199)). The automatic fit writes `canvas_w`/`canvas_h` into the graph without an undo step, and in a project that change is autosaved.

## N4 · Generators adapt to what they are combined with {#n4}

> If generative operators are connected to combining operators, they will generate at whatever size is necessary to combine successfully with the other side.

**Status.** Implemented.

**Implemented by.**
- A combining node's size comes from its definite inputs (`union_size`, `Blend::size`) ([basic.rs:20-24](../../src/nodes/basic.rs#L20), [basic.rs:219-221](../../src/nodes/basic.rs#L219)).
- `input_request` passes the consumer's `extent` to inputs without a size of their own ([basic.rs:14-18](../../src/nodes/basic.rs#L14)); `size_conflict` ignores size-less inputs (`size_of`) ([graph.rs:352-353](../../src/graph.rs#L352)).

**Tests.** None checks that a generator adapts to its consumer's size; `tests/project.rs::save_and_load_give_the_same_graph` renders a stack of an image and noise without asserting the size.

## N5 · The initial generators {#n5}

> The initial generators are: solid color, linear gradient, radial gradient, checkerboard, noise (fBm, turbulence, ridged), Voronoi and Mandelbrot.

**Status.** Implemented.

**Implemented by.**
- `SOLID`, `LINEAR`, `RADIAL`, `CHECKER`, `NOISE` (with `NOISE_TYPES` fbm, turbulence, ridged), `VORONOI` and `MANDELBROT` ([generators.rs:61-220](../../src/nodes/generators.rs#L61)).
- Their kernels, `SOLID` to `MANDELBROT` ([kernels.rs:738-976](../../src/engine/kernels.rs#L738)).

**Tests.** `tests/engine.rs::gpu_matches_cpu` (renders every generator; skipped without a GPU).

## N6 · AI image generation (planned) {#n6}

> An AI image-generation operator is planned.

**Status.** Deferred. The spec plans it; nothing implements it.

**Implemented by.** Nothing. The expensive-result `Cache` used by background removal is the README's intended hook ([cache.rs:12-15](../../src/engine/cache.rs#L12)).

## Z1 · Ask which size to use, insert a Resize {#z1}

> However, the outputs produced by imported images and by combining operators will have a definite size.
> In this case, Delos should ask which output size must be used and insert a resizing node after one of the inputs as appropriate.

**Status.** Implemented. A third answer, "Keep both sizes (align top-left)", is not in the spec.

**Implemented by.**
- `Graph::size_conflict` finds two definite input sizes that differ on a node with `combines_inputs` ([graph.rs:347-363](../../src/graph.rs#L347)).
- `after_edit` looks for a conflict after every edit ([state.rs:304-313](../../src/ui/state.rs#L304)).
- `ConflictDialog` asks and calls `insert_resize` on the other input ([dialogs.rs:11-57](../../src/ui/dialogs.rs#L11)); `insert_resize` adds a `Resize` set by `resize_to` ([graph.rs:367-375](../../src/graph.rs#L367)).

**Tests.** `tests/engine.rs::size_inference_and_conflicts`.

**Notes.** The third answer calls `ignore_conflict` ([dialogs.rs:58](../../src/ui/dialogs.rs#L58), [state.rs:315-319](../../src/ui/state.rs#L315)), and `after_edit` then skips ignored conflicts. The two resize buttons call `edit`, whose `after_edit` records the next conflict, and then `conflict.set(None)` erases it ([dialogs.rs:42-55](../../src/ui/dialogs.rs#L42)). A second conflict elsewhere in the graph is therefore not shown until the next edit ([D-CONFLICT](findings.md#d-conflict)).
