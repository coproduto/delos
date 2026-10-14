# Input inventory

140 entry points found by `inputtrace inventory`.

| Kind | Count |
|---|---|
| handler | 105 |
| effect | 6 |
| env | 8 |
| fs | 14 |
| dialog | 5 |
| webview | 2 |

## handler

| Where | Component / fn | Input | Calls | Writes |
|---|---|---|---|---|
| `src/ui/dialogs.rs:42` | `ConflictDialog` | onclick on <button class="btn primary"> “Use {fmt_size(sa)} (resize {nb})” | `App::edit`, `Graph::insert_resize` | `App.conflict` |
| `src/ui/dialogs.rs:51` | `ConflictDialog` | onclick on <button class="btn primary"> “Use {fmt_size(sb)} (resize {na})” | `App::edit`, `Graph::insert_resize` | `App.conflict` |
| `src/ui/dialogs.rs:58` | `ConflictDialog` | onclick on <button class="btn"> “Keep both sizes (align top-left)” | `App::ignore_conflict` | — |
| `src/ui/dialogs.rs:114` | `ExportDialog` | onmousedown on <div class="modal-backdrop"> | `App::export_pressed` | — |
| `src/ui/dialogs.rs:115` | `ExportDialog` | onclick on <div class="modal-backdrop"> | `App::export_backdrop_clicked` | — |
| `src/ui/dialogs.rs:117` | `ExportDialog` | onmousedown on <div class="modal"> | `App::export_pressed` | — |
| `src/ui/dialogs.rs:121` | `ExportDialog` | onclick on <div class="modal"> | — | — |
| `src/ui/dialogs.rs:125` | `ExportDialog` | onclick on <button class="modal-close"> “×” | — | `App.export` |
| `src/ui/dialogs.rs:133` | `ExportDialog` | onchange on <select> | `codec_by_name` | `App.export` |
| `src/ui/dialogs.rs:154` | `ExportDialog` | onchange on <input type="text"> | — | `App.export` |
| `src/ui/dialogs.rs:166` | `ExportDialog` | oninput on <input type="range"> | — | `App.export` |
| `src/ui/dialogs.rs:184` | `ExportDialog` | onclick on <button class="btn danger"> “Cancel export” | `Worker::cancel_export` | — |
| `src/ui/dialogs.rs:193` | `ExportDialog` | onclick on <button class="btn primary"> “Export…” (via `start`) | `codec_by_name`, `choose_save_target`, `Worker::export` | `App.export` |
| `src/ui/dialogs.rs:194` | `ExportDialog` | onclick on <button class="btn"> “Close” | — | `App.export` |
| `src/ui/dialogs.rs:226` | `ProjectDialog` | onclick on <button class="modal-close"> “×” | `cancel_prompt` | — |
| `src/ui/dialogs.rs:250` | `ProjectDialog` | oninput on <input type="text"> | `set_prompt_name` | — |
| `src/ui/dialogs.rs:261` | `ProjectDialog` | onclick on <button class="btn small"> “Change…” | `pick_folder` | `App.project_prompt` |
| `src/ui/dialogs.rs:286` | `ProjectDialog` | onclick on <button class="btn"> “Quick mode” | `choose_quick` | — |
| `src/ui/dialogs.rs:290` | `ProjectDialog` | onclick on <button class="btn"> “Cancel” | `cancel_prompt` | — |
| `src/ui/dialogs.rs:295` | `ProjectDialog` | onclick on <button class="btn primary"> | `create_project` | — |
| `src/ui/graph_view.rs:251` | `GraphView` | onmounted on <div class="graph wiring"> | — | `GraphView.origin`, `GraphView.mounted` |
| `src/ui/graph_view.rs:255` | `GraphView` | onresize on <div class="graph wiring"> | — | `GraphView.origin` |
| `src/ui/graph_view.rs:256` | `GraphView` | onwheel on <div class="graph wiring"> | — | `GraphView.pan`, `GraphView.zoom` |
| `src/ui/graph_view.rs:266` | `GraphView` | onpointerdown on <div class="graph wiring"> | `App::select` | `GraphView.drag` |
| `src/ui/graph_view.rs:272` | `GraphView` | onpointermove on <div class="graph wiring"> | `Graph::node_mut` | `App.graph`, `GraphView.pan`, `GraphView.drag` |
| `src/ui/graph_view.rs:293` | `GraphView` | onpointerup on <div class="graph wiring"> | — | `GraphView.drag` |
| `src/ui/graph_view.rs:294` | `GraphView` | onpointerleave on <div class="graph wiring"> | — | `GraphView.drag` |
| `src/ui/graph_view.rs:306` | `GraphView` | onpointerdown on <path class="edge-hit"> | `encode_edge` | `App.selected`, `App.selected_edge` |
| `src/ui/graph_view.rs:337` | `GraphView` | onpointerdown on <div class="{cls}"> | `App::select`, `App::edit_coalesced` | `GraphView.drag` |
| `src/ui/graph_view.rs:346` | `GraphView` | ondoubleclick on <div class="{cls}"> | `App::select` | `App.pinned` |
| `src/ui/graph_view.rs:375` | `GraphView` | onpointerdown on <div class="port out viewing-port"> ““{name}” · drag to an input to connect · double-click to view” | — | `GraphView.drag` |
| `src/ui/graph_view.rs:381` | `GraphView` | ondoubleclick on <div class="port out viewing-port"> ““{name}” · drag to an input to connect · double-click to view” | `App::select` | `App.pinned` |
| `src/ui/graph_view.rs:404` | `GraphView` | onpointerdown on <div class="{pcls}"> | `disconnect_ref` | `GraphView.drag` |
| `src/ui/graph_view.rs:414` | `GraphView` | onpointerup on <div class="{pcls}"> | `connect_port` | `GraphView.drag` |
| `src/ui/graph_view.rs:433` | `GraphView` | onpointerdown on <div class="graph-tools"> | — | — |
| `src/ui/graph_view.rs:436` | `GraphView` | onclick on <button class="btn small"> “Home” | — | `GraphView.pan`, `GraphView.zoom` |
| `src/ui/graph_view.rs:452` | `GraphView` | onclick on <button class="btn small"> “+” | — | `GraphView.zoom` |
| `src/ui/graph_view.rs:453` | `GraphView` | onclick on <button class="btn small"> “−” | — | `GraphView.zoom` |
| `src/ui/image_view.rs:197` | `ImageView` | onmounted on <div class="viewport"> | — | `ImageView.origin`, `ImageView.mounted` |
| `src/ui/image_view.rs:201` | `ImageView` | onresize on <div class="viewport"> | — | `ImageView.origin`, `App.viewport` |
| `src/ui/image_view.rs:207` | `ImageView` | onwheel on <div class="viewport"> | — | `App.view` |
| `src/ui/image_view.rs:222` | `ImageView` | onpointerdown on <div class="viewport"> | — | `ImageView.drag` |
| `src/ui/image_view.rs:227` | `ImageView` | onpointermove on <div class="viewport"> | `App::set_param`, `App::edit_coalesced`, `Graph::set_param` | `App.view`, `App.canvas_user_set`, `ImageView.drag` |
| `src/ui/image_view.rs:289` | `ImageView` | onpointerup on <div class="viewport"> | — | `ImageView.drag` |
| `src/ui/image_view.rs:290` | `ImageView` | onpointerleave on <div class="viewport"> | — | `ImageView.drag` |
| `src/ui/image_view.rs:291` | `ImageView` | ondoubleclick on <div class="viewport"> | — | `App.view` |
| `src/ui/image_view.rs:313` | `ImageView` | onpointerdown on <rect class="handle canvas-handle"> | — | `ImageView.drag` |
| `src/ui/image_view.rs:341` | `ImageView` | onpointerdown on <div class="statusbar"> | — | — |
| `src/ui/image_view.rs:346` | `ImageView` | onclick on <button class="btn small"> “Fit” | — | `App.view` |
| `src/ui/image_view.rs:349` | `ImageView` | onclick on <button class="btn small"> “1:1” | — | `App.view` |
| `src/ui/image_view.rs:356` | `ImageView` | onclick on <button class="btn small"> “+” | `zoom_by` | — |
| `src/ui/image_view.rs:357` | `ImageView` | onclick on <button class="btn small"> “−” | `zoom_by` | — |
| `src/ui/image_view.rs:402` | `render_overlay` | onpointerdown on <line class="axis-hit h"> | — | `ImageView.drag`, `ImageView.frac_map` |
| `src/ui/image_view.rs:436` | `render_overlay` | onpointerdown on <line class="axis-hit h"> | — | `ImageView.drag`, `ImageView.frac_map` |
| `src/ui/image_view.rs:457` | `render_overlay` | onpointerdown on <circle class="handle point"> | — | `ImageView.drag`, `ImageView.frac_map` |
| `src/ui/image_view.rs:484` | `render_overlay` | onpointerdown on <rect class="clip-rect"> | — | `ImageView.drag`, `ImageView.frac_map` |
| `src/ui/image_view.rs:498` | `render_overlay` | onpointerdown on <rect class="handle"> | — | `ImageView.drag`, `ImageView.frac_map` |
| `src/ui/inspector.rs:19` | `Inspector` | onkeydown on <div class="inspector"> | — | — |
| `src/ui/inspector.rs:108` | `NodePanel` | onchange on <input class="title-input"> | `App::edit`, `Graph::node_mut` | — |
| `src/ui/inspector.rs:146` | `NodePanel` | onclick on <button class="btn active"> “{label}” | `Link::new` | `App.pinned` |
| `src/ui/inspector.rs:154` | `NodePanel` | onclick on <button class="btn"> “{export_label}” | `App::open_export` | — |
| `src/ui/inspector.rs:155` | `NodePanel` | onclick on <button class="btn danger"> “Delete” | `App::delete_node` | — |
| `src/ui/inspector.rs:167` | `NodePanel` | onchange on <select> | `Link::new`, `connect_port`, `App::edit`, `Graph::disconnect` | — |
| `src/ui/inspector.rs:204` | `NodePanel` | onclick on <button class="btn"> “Canvas = window size” | `App::fit_canvas_to_window` | — |
| `src/ui/inspector.rs:269` | `ParamField` | oninput on <input type="range"> | `App::set_param` | — |
| `src/ui/inspector.rs:276` | `ParamField` | onchange on <input type="number" class="num"> | `App::set_param` | — |
| `src/ui/inspector.rs:300` | `ParamField` | oninput on <input type="range"> | `App::set_param` | — |
| `src/ui/inspector.rs:307` | `ParamField` | onchange on <input type="number" class="num"> “pixels” | `App::set_param` | — |
| `src/ui/inspector.rs:328` | `ParamField` | oninput on <input type="range"> | `App::set_param` | — |
| `src/ui/inspector.rs:337` | `ParamField` | onchange on <input type="number" class="num"> | `App::set_param` | — |
| `src/ui/inspector.rs:347` | `ParamField` | onchange on <input type="checkbox"> | `App::set_param` | — |
| `src/ui/inspector.rs:357` | `ParamField` | onchange on <select> | `App::set_param` | — |
| `src/ui/inspector.rs:375` | `ParamField` | oninput on <input type="color"> | `parse_hex`, `App::set_param` | — |
| `src/ui/inspector.rs:384` | `ParamField` | oninput on <input type="range"> “opacity” | `App::set_param` | — |
| `src/ui/inspector.rs:395` | `ParamField` | onchange on <input type="text"> | `App::set_param` | — |
| `src/ui/inspector.rs:448` | `LayerStack` | onclick on <button class="icon-btn"> “Disable layer” | `App::edit`, `Graph::stack_chain`, `Graph::set_stack_layers`, `Graph::node`, `Graph::remove_node`, `Graph::add_node`, `Graph::connect` | — |
| `src/ui/inspector.rs:466` | `LayerStack` | onclick on <span class="layer-name"> “{name}” | `App::select` | — |
| `src/ui/inspector.rs:473` | `LayerStack` | onclick on <button class="icon-btn"> “▲” | `App::edit`, `Graph::stack_chain`, `Graph::set_stack_layers` | — |
| `src/ui/inspector.rs:480` | `LayerStack` | onclick on <button class="icon-btn"> “▼” | `App::edit`, `Graph::stack_chain`, `Graph::set_stack_layers` | — |
| `src/ui/inspector.rs:486` | `LayerStack` | onclick on <button class="icon-btn"> “✕” | `App::edit`, `Graph::stack_chain`, `Graph::set_stack_layers` | — |
| `src/ui/inspector.rs:499` | `LayerStack` | onchange on <input type="file"> | `import` | — |
| `src/ui/inspector.rs:508` | `LayerStack` | onchange on <select class="small"> | `kind`, `App::edit`, `Graph::stack_chain`, `Graph::set_stack_layers`, `Graph::node`, `Graph::add_node` | — |
| `src/ui/inspector.rs:523` | `LayerStack` | onclick on <button class="btn small"> “+ Empty slot” | `App::edit`, `Graph::stack_chain`, `Graph::set_stack_layers` | — |
| `src/ui/mod.rs:84` | `Root` | onkeydown on <div class="app"> | `on_key` | — |
| `src/ui/mod.rs:85` | `Root` | ondragover on <div class="app"> | — | — |
| `src/ui/mod.rs:86` | `Root` | ondrop on <div class="app"> | `is_project_path`, `has_unsaved_work`, `confirm`, `open_project`, `import` | — |
| `src/ui/mod.rs:337` | `Toolbar` | onclick on <button class="{cls}"> “{label}” | — | `App.layout` |
| `src/ui/mod.rs:351` | `Toolbar` | onchange on <input type="file"> | `import` | — |
| `src/ui/mod.rs:362` | `Toolbar` | onclick on <button class="btn"> “Add node ▾” | — | `Toolbar.menu_open` |
| `src/ui/mod.rs:364` | `Toolbar` | onclick on <div class="menu-backdrop"> | — | `Toolbar.menu_open` |
| `src/ui/mod.rs:372` | `Toolbar` | onclick on <button class="menu-item"> “{k.name()}” | `App::add_node` | `Toolbar.menu_open` |
| `src/ui/mod.rs:391` | `Toolbar` | onclick on <button class="btn"> “Export” | `App::open_export` | — |
| `src/ui/mod.rs:395` | `Toolbar` | onclick on <button class="btn icon"> “↶” | `App::undo` | — |
| `src/ui/mod.rs:396` | `Toolbar` | onclick on <button class="btn icon"> “↷” | `App::redo` | — |
| `src/ui/mod.rs:414` | `Toolbar` | onchange on <input type="checkbox"> | — | `App.follow_selection` |
| `src/ui/mod.rs:422` | `Toolbar` | onclick on <button class="link"> “Reset” | — | `App.pinned` |
| `src/ui/mod.rs:432` | `Toolbar` | onclick on <button class="badge"> “{backend}” | — | `App.backend`, `App.graph` |
| `src/ui/mod.rs:456` | `Toast` | onclick on <div class="{cls}"> “{msg}” | — | `App.toast` |
| `src/ui/mod.rs:521` | `ProjectMenu` | onclick on <button class="btn"> “Project ▾” | — | `ProjectMenu.open` |
| `src/ui/mod.rs:523` | `ProjectMenu` | onclick on <div class="menu-backdrop"> | — | `ProjectMenu.open` |
| `src/ui/mod.rs:528` | `ProjectMenu` | onclick on <button class="menu-item"> “Save the current work as a project folder (images are copied into it)” | `prompt_new_project` | `ProjectMenu.open` |
| `src/ui/mod.rs:535` | `ProjectMenu` | onclick on <button class="menu-item"> “Import project…” (via `import_file`) | `has_unsaved_work`, `confirm`, `open_project`, `pick_project_file` | `ProjectMenu.open` |
| `src/ui/mod.rs:536` | `ProjectMenu` | onclick on <button class="menu-item"> “Open project folder…” (via `import_folder`) | `has_unsaved_work`, `confirm`, `open_project`, `pick_folder` | `ProjectMenu.open` |
| `src/ui/mod.rs:541` | `ProjectMenu` | onclick on <button class="menu-item"> “Share project…” (via `share`) | `share_file_name`, `pick_save_path`, `share_project` | `ProjectMenu.open` |
| `src/ui/mod.rs:547` | `ProjectMenu` | onclick on <button class="menu-item"> (via `close`) | `has_unsaved_work`, `confirm`, `close_session` | `ProjectMenu.open` |

## effect

| Where | Component / fn | Input | Calls | Writes |
|---|---|---|---|---|
| `src/ui/image_view.rs:42` | `ImageView` | use_memo in ImageView → `disp` | `App::displayed` | — |
| `src/ui/image_view.rs:51` | `ImageView` | use_effect in ImageView | — | `App.view` |
| `src/ui/image_view.rs:70` | `ImageView` | use_effect in ImageView | `App::fit_canvas_to_window` | — |
| `src/ui/image_view.rs:83` | `ImageView` | use_effect in ImageView | `Rect::new`, `Rect::intersect`, `Request::new`, `Graph::link_hash`, `Worker::preview` | `App.seq`, `App.busy` |
| `src/ui/mod.rs:63` | `Root` | use_effect in Root | — | — |
| `src/ui/session.rs:251` | `use_autosave` | use_effect in use_autosave | `Worker::save_project`, `App::all_images` | `App.save_state` |

## env

| Where | fn | Input |
|---|---|---|
| `src/engine/gpu.rs:94` | `Gpu::new` | env DELOS_NO_GPU |
| `src/nodes/bgremove.rs:153` | `resolve_model` | env DELOS_REMBG_MODEL |
| `src/nodes/bgremove.rs:156` | `resolve_model` | env HOME |
| `src/nodes/bgremove.rs:156` | `resolve_model` | env USERPROFILE |
| `src/project.rs:40` | `default_root` | env DELOS_PROJECTS_DIR |
| `src/project.rs:43` | `default_root` | env HOME |
| `src/project.rs:43` | `default_root` | env USERPROFILE |
| `src/ui/platform.rs:132` | `choose_save_target` | env HOME |

## fs

| Where | fn | Input |
|---|---|---|
| `src/project.rs:70` | `Project::create` | fs::read_dir in Project::create |
| `src/project.rs:106` | `Project::add_input` | fs::read in Project::add_input |
| `src/project.rs:156` | `Project::sync_inputs` | fs::read_dir in Project::sync_inputs |
| `src/project.rs:216` | `Project::verify` | fs::read_to_string in Project::verify |
| `src/project.rs:239` | `Project::load` | fs::read in Project::load |
| `src/project.rs:272` | `Project::share` | File::open in Project::share |
| `src/project.rs:306` | `unpack` | File::open in unpack |
| `src/project.rs:307` | `unpack` | ZipArchive::new in unpack |
| `src/project.rs:367` | `collect_files` | fs::read_dir in collect_files |
| `src/ui/session.rs:157` | `create_project` | fs::read_dir in create_project |
| `src/ui/session.rs:300` | `start_imports` | read_bytes in start_imports |
| `src/worker.rs:253` | `Worker::import` | fs::read in Worker::import |
| `src/worker.rs:368` | `Worker::save_project` | fs::read_to_string in Worker::save_project |
| `src/worker.rs:404` | `new_stash_dir` | fs::read_dir in new_stash_dir |

## dialog

| Where | fn | Input |
|---|---|---|
| `src/ui/platform.rs:123` | `choose_save_target` | AsyncFileDialog in choose_save_target |
| `src/ui/platform.rs:172` | `pick_project_file` | AsyncFileDialog in pick_project_file |
| `src/ui/platform.rs:183` | `pick_folder` | AsyncFileDialog in pick_folder |
| `src/ui/platform.rs:205` | `choose_save_target_titled` | AsyncFileDialog in choose_save_target_titled |
| `src/ui/platform.rs:222` | `confirm` | AsyncMessageDialog in confirm |

## webview

| Where | fn | Input |
|---|---|---|
| `src/ui/platform.rs:40` | `use_frame_server` | use_asset_handler in use_frame_server |
| `src/ui/platform.rs:161` | `device_pixel_ratio` | document::eval in device_pixel_ratio |
