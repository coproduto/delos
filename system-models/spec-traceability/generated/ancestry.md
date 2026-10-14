# Spec ancestry

Seeds are the lines clause entries cite. *Support* is code reached from them; *unspecified* is code no clause leads to.

| Class | Items | Lines |
|---|---|---|
| specified | 218 | 4395 |
| partly specified | 21 | 947 |
| support | 275 | 1951 |
| unspecified | 162 | 713 |

## Unspecified state

Fields that code writes but that no clause-cited line uses (the field's own declaration does not count). *Paths through writers*: how many clauses reach a function that writes the field. A high number means hidden state woven into specified behaviour.

| Field | Paths through writers | Writers | Noted in |
|---|---|---|---|
| `App.selected_edge` `src/ui/state.rs:113` | 45 | `disconnect_ref` (specified), `GraphView` (partly specified), `App::replace_document` (partly specified), `App::fix_selection` (partly specified), `App::select` (support) | D-OPEN-WRITES, P6.2, U14 |
| `History.last_time` `src/ui/state.rs:105` (declared on a cited line) | 44 | `App::edit_coalesced` (support) | U1 |
| `History.redo` `src/ui/state.rs:103` (declared on a cited line) | 44 | `App::push_history` (support), `App::undo` (unspecified), `App::redo` (unspecified) | U1 |
| `History.undo` `src/ui/state.rs:102` (declared on a cited line) | 44 | `App::push_history` (support), `App::undo` (unspecified), `App::redo` (unspecified) | U1 |
| `ParamDef.visible_if` `src/nodes/mod.rs:84` (declared on a cited line) | 42 | `ParamDef::when` (support) |  |
| `Graph.next_id` `src/graph.rs:71` (declared on a cited line) | 36 | `Graph::fresh_id` (support) | D-STALE |
| `State.idle` `src/engine/pool.rs:42` | 21 | `Pool::take` (unspecified), `Pool::put` (support) |  |
| `App.project_busy` `src/ui/state.rs:137` | 8 | `pump` (specified), `create_project` (partly specified), `on_project_created` (partly specified), `open_project` (specified), `on_project_opened` (specified) … | P3.3, U12, U16 |
| `App.importing` `src/ui/state.rs:130` | 6 | `pump` (specified), `start_imports` (partly specified) | U12 |
| `App.imports` `src/ui/state.rs:129` | 6 | `pump` (specified), `start_imports` (partly specified) | U12 |
| `Registry.slots` `src/engine/resident.rs:54` | 6 | `Registry::evict` (support), `Resident::queue` (support), `upload_loop` (support) |  |
| `App.errors` `src/ui/state.rs:122` | 4 | `pump` (specified), `App::replace_document` (partly specified) | D-OPEN-WRITES, P6.2, U12 |
| `App.global_error` `src/ui/state.rs:123` | 4 | `pump` (specified), `App::replace_document` (partly specified) | D-OPEN-WRITES, P6.2, U12 |
| `Pool.state` `src/engine/pool.rs:48` | 1 | `upload_loop` (support) |  |
| `Registry.tick` `src/engine/resident.rs:57` | 1 | `Resident::get` (unspecified), `upload_loop` (support) |  |
| `Slot.state` `src/engine/resident.rs:47` | 1 | `upload_loop` (support) |  |
| `EncodeOptions.quality` `src/formats.rs:23` | 0 | `ExportDialog` (partly specified) | U10 |
| `Engine.use_gpu` `src/engine/mod.rs:61` | 0 | `Toolbar` (specified) | R4, U1, U13 |
| `ExportState.quality` `src/ui/state.rs:58` | 0 | `ExportDialog` (partly specified) | U10 |
| `State.created` `src/engine/pool.rs:44` | 0 | `Pool::take` (unspecified) |  |

## Uncited branches of cited functions

Runs of 3+ code lines that no clause cites, inside functions that clauses do cite: the behaviour a clause skips over. Spans that change state or call uncited code come first.

| Where | Lines | Starts with | Calls | Noted in |
|---|---|---|---|---|
| `GraphView` `src/ui/graph_view.rs:190-345` | 123 ✎ | `let mut app = use_context::<App>();` | `App::edit_coalesced`, `App::select`, `Builder::node`, `DNode::height` | U14 |
| `ImageView` `src/ui/image_view.rs:174-244` | 58 ✎ | `_ if editing != Some(d.target.node) => format!("Editing {t} · showing its input"),` |  | U14 |
| `Gpu::init` `src/engine/gpu.rs:138-195` | 47 | `let tex_entry = \|binding\| wgpu::BindGroupLayoutEntry {` | `FORMAT`, `Gpu::make_texture`, `Pool::new` |  |
| `ExportDialog` `src/ui/dialogs.rs:70-123` | 46 ✎ | `let mut app = use_context::<App>();` | `App::export_pressed`, `EXPORT_ID`, `ImageCrate::extensions`, `Png::extensions` |  |
| `GraphView` `src/ui/graph_view.rs:393-458` | 43 ✎ | `for (r , row) in n.rows.iter().cloned().enumerate() {` | `HEADER`, `NODE_W`, `ROW` | U14 |
| `ExportDialog` `src/ui/dialogs.rs:127-169` | 31 ✎ | `p { class: "muted", "{st.size.0}×{st.size.1} px ({megapixels:.1} MP), rendered at full resolution in` | `ImageCrate::extensions`, `Png::extensions`, `codec_by_name`, `encoders` | U10 |
| `ProjectDialog` `src/ui/dialogs.rs:239-280` | 30 ✎ | `if from_import {` |  |  |
| `NodePanel` `src/ui/inspector.rs:91-130` | 29 | `let export_label = if multi { format!("Export {}…", outputs[export_link.output]) } else { "Export…".` | `App::edit`, `BackgroundRemoval::slow_hint`, `Graph::node_mut`, `Pool::trim` | U4 |
| `ImageView` `src/ui/image_view.rs:323-361` | 26 ✎ | `if let Some(b) = badge {` | `zoom_by` | U14 |
| `LayerStack` `src/ui/inspector.rs:489-527` | 24 ✎ | `div { class: "actions",` | `Builder::node`, `Env::node`, `Graph::add_node`, `Graph::node` | D-SPEC-WORDING, O1.1, U15 |
| `pump` `src/ui/mod.rs:152-187` | 24 ✎ | `if !f.provisional && f.seq == latest {` | `App::toast` | U12 |
| `start_imports` `src/ui/session.rs:278-312` | 24 ✎ | `for f in files {` | `App::toast`, `IMPORT_TOKEN` |  |
| `NodePanel` `src/ui/inspector.rs:155-188` | 23 | `button { class: "btn danger", onclick: move \|_\| app.delete_node(id), "Delete" }` | `App::edit`, `BackgroundRemoval::inputs`, `Clip::inputs`, `Disable::inputs` | U4 |
| `App::add_node` `src/ui/state.rs:453-483` | 23 | `let Some(k) = nodes::kind(kind) else { return };` | `App::edit`, `BackgroundRemoval::inputs`, `Builder::node`, `Clip::inputs` | D-ADD-AFTER-SPLIT, S5, U3 |
| `ImageView` `src/ui/image_view.rs:126-158` | 22 ✎ | `let refresh_origin = move \|\| async move {` |  |  |
| `App::new` `src/ui/state.rs:153-175` | 21 | `layout: Signal::new(Layout::Split),` | `ViewState::default`, `default_projects_root` |  |
| `App::delete_node` `src/ui/state.rs:506-530` | 18 ✎ | `if id == self.graph.peek().output {` | `App::edit`, `App::stack_chain`, `Builder::node`, `Env::node` | U3 |
| `build` `src/ui/graph_view.rs:88-109` | 17 ✎ | `let rows = k` | `BackgroundRemoval::category`, `BackgroundRemoval::inputs`, `Blend::category`, `Clip::category` |  |
| `Worker::run_preview` `src/worker.rs:183-207` | 17 | `let tx = self.tx.clone();` | `Request::pixel_count`, `Worker::send_frame`, `now_ms`, `render_to_cpu` | U5 |
| `ImageView` `src/ui/image_view.rs:42-68` | 16 ✎ | `let disp = use_memo(move \|\| app.displayed());` | `device_pixel_ratio` |  |
| `Toolbar` `src/ui/mod.rs:361-383` | 16 ✎ | `div { class: "menu-wrap",` | `Graph::add_node` |  |
| `Toolbar` `src/ui/mod.rs:423-446` | 14 ✎ | `div { class: "spacer" }` | `Engine::backend_name`, `Worker::backend_name`, `store` | R4, U13 |
| `BackgroundRemoval::render` `src/nodes/bgremove.rs:63-76` | 12 | `let p = &node.params;` | `Ctx::link_hash`, `ParamDef::choice`, `ParamDef::text`, `Params::b` | D-STALE, O5 |
| `ImageView` `src/ui/image_view.rs:287-304` | 12 ✎ | `onpointerup: move \|_\| drag.set(Drag::None),` |  |  |
| `LayerStack` `src/ui/inspector.rs:469-482` | 12 ✎ | `button {` |  | O1.1, U15 |

73 more spans (`--json` for all). ✎: the span changes state.

## Unspecified code

Groups of functions no clause leads to (shown: those reached from uncited lines of cited code, or of 10+ lines). *Triggered from* names the uncited lines that call into the group: how a user reaches it.

### 1. Inspector, DocumentPanel

38 lines in `src/ui/inspector.rs` · **not noted anywhere**

- triggered from `Root` at `src/ui/mod.rs:116`

### 2. disconnect_port, decode_edge, App::delete_selection

23 lines in `src/ui/graph_view.rs`, `src/ui/state.rs` · **not noted anywhere**

- triggered from `on_key` at `src/ui/mod.rs:295`

### 3. use_frame_server

18 lines in `src/ui/platform.rs` · **not noted anywhere**

- triggered from `Root` at `src/ui/mod.rs:61`

### 4. deliver_bytes

14 lines in `src/ui/platform.rs` · changes state · noted in U11

- triggered from `pump` at `src/ui/mod.rs:202`

### 5. App::undo

11 lines in `src/ui/state.rs` · changes state · noted in U1

- triggered from `on_key` at `src/ui/mod.rs:291`
- triggered from `Toolbar` at `src/ui/mod.rs:395`

### 6. App::redo

11 lines in `src/ui/state.rs` · changes state · noted in U1

- triggered from `on_key` at `src/ui/mod.rs:291`
- triggered from `on_key` at `src/ui/mod.rs:293`
- triggered from `Toolbar` at `src/ui/mod.rs:396`

### 7. extensions, ImageCrate::extensions, Png::extensions, accept_attr

10 lines in `src/formats.rs` · **not noted anywhere**

- triggered from `ExportDialog` at `src/ui/dialogs.rs:85`
- triggered from `ExportDialog` at `src/ui/dialogs.rs:138`
- triggered from `LayerStack` at `src/ui/inspector.rs:424`
- triggered from `Toolbar` at `src/ui/mod.rs:323`

### 8. can_encode, ImageCrate::can_encode, Png::can_encode, encoders

10 lines in `src/formats.rs` · **not noted anywhere**

- triggered from `ExportDialog` at `src/ui/dialogs.rs:73`
- triggered from `ExportDialog` at `src/ui/dialogs.rs:143`

### 9. Toast

10 lines in `src/ui/mod.rs` · changes state · **not noted anywhere**

- triggered from `Root` at `src/ui/mod.rs:122`

### 10. DEFAULT_CAP, DEFAULT_CAP, Pool::new, Pool::default

8 lines in `src/engine/pool.rs` · noted in U6

- triggered from `Gpu::init` at `src/engine/gpu.rs:191`

### 11. default_projects_root

8 lines in `src/ui/session.rs` · **not noted anywhere**

- triggered from `App::new` at `src/ui/state.rs:173`

### 12. App::export_pressed

8 lines in `src/ui/state.rs` · changes state · **not noted anywhere**

- triggered from `ExportDialog` at `src/ui/dialogs.rs:114`
- triggered from `ExportDialog` at `src/ui/dialogs.rs:119`

### 13. Graph::disconnect

7 lines in `src/graph.rs` · **not noted anywhere**

- triggered from `disconnect_ref` at `src/ui/graph_view.rs:165`
- triggered from `NodePanel` at `src/ui/inspector.rs:172`

### 14. encode_edge

7 lines in `src/ui/graph_view.rs` · **not noted anywhere**

- triggered from `GraphView` at `src/ui/graph_view.rs:233`
- triggered from `GraphView` at `src/ui/graph_view.rs:309`

### 15. BackgroundRemoval::slow_hint, slow_hint

6 lines in `src/nodes/bgremove.rs`, `src/nodes/mod.rs` · **not noted anywhere**

- triggered from `NodePanel` at `src/ui/inspector.rs:124`

### 16. zoom_by

5 lines in `src/ui/image_view.rs` · changes state · **not noted anywhere**

- triggered from `ImageView` at `src/ui/image_view.rs:356`
- triggered from `ImageView` at `src/ui/image_view.rs:357`

### 17. App::ignore_conflict

5 lines in `src/ui/state.rs` · changes state · noted in U9, Z1

- triggered from `ConflictDialog` at `src/ui/dialogs.rs:58`

### 18. NODE_W, DNode::out_port

4 lines in `src/ui/graph_view.rs` · **not noted anywhere**

- triggered from `GraphView` at `src/ui/graph_view.rs:442`
- triggered from `GraphView` at `src/ui/graph_view.rs:229`
- triggered from `GraphView` at `src/ui/graph_view.rs:240`

### 19. Request::pixel_count

3 lines in `src/engine/geom.rs` · **not noted anywhere**

- triggered from `Resize::render` at `src/nodes/resize.rs:104`
- triggered from `Worker::run_preview` at `src/worker.rs:197`

### 20. vivid_light

3 lines in `src/engine/kernels.rs` · noted in O9

- triggered from `blend_channel` at `src/engine/kernels.rs:424`
- triggered from `blend_channel` at `src/engine/kernels.rs:430`

### 21. is_cancelled

3 lines in `src/engine/mod.rs` · **not noted anywhere**

- triggered from `Worker::run_preview` at `src/worker.rs:218`
- triggered from `Worker::export` at `src/worker.rs:318`

### 22. codec_by_name

3 lines in `src/formats.rs` · **not noted anywhere**

- triggered from `ExportDialog` at `src/ui/dialogs.rs:73`
- triggered from `ExportDialog` at `src/ui/dialogs.rs:84`
- triggered from `ExportDialog` at `src/ui/dialogs.rs:136`
- triggered from `Worker::export` at `src/worker.rs:297`

### 23. DNode::in_port

3 lines in `src/ui/graph_view.rs` · **not noted anywhere**

- triggered from `GraphView` at `src/ui/graph_view.rs:230`

### 24. device_pixel_ratio

3 lines in `src/ui/platform.rs` · **not noted anywhere**

- triggered from `ImageView` at `src/ui/image_view.rs:46`

### 25. cancel_prompt

3 lines in `src/ui/session.rs` · changes state · **not noted anywhere**

- triggered from `ProjectDialog` at `src/ui/dialogs.rs:226`
- triggered from `ProjectDialog` at `src/ui/dialogs.rs:290`
- triggered from `on_key` at `src/ui/mod.rs:296`

116 more groups, untriggered and under 10 lines or beyond the top 25 (`--json` for all).
