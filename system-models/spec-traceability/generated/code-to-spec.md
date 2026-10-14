## `src/engine/cache.rs`

- `Entry` (10-10): D1, U7
- `Cache` (12-15): D1, N6, U7
- `Cache::new` (18-20): U7
- `Cache::get` (22-29): U7
- `Cache::insert` (31-44): U7
- `Cache::bytes` (46-48): U7

## `src/engine/geom.rs`

- `Map2` (99-102): D-OVERLAY, F3.3.1

## `src/engine/gpu.rs`

- `Gpu::new` (93-109): R4, U13
- `Gpu::init` (111-195): R4
- `Gpu::fits` (217-219): R4
- `Gpu::pipeline` (223-250): R4

## `src/engine/kernels.rs`

- `Kernel` (18-25): R3
- `WGSL_PRELUDE` (105-144): R1
- `INVERT` (215-226): O6
- `OVER` (229-242): O1
- `SPLIT` (271-288): S1, S3
- `REFLECT` (295-335): O3
- `BLEND_MODES` (341-370): O10, O9
- `vivid_light` (398-400): O9
- `logic` (402-411): O10
- `blend_channel` (413-445): O10, O9
- `set_lum` (468-471): O9
- `sat` (472-474): O9
- `set_sat` (475-483): O9
- `blend_color` (485-493): O9
- `BLEND` (497-647): O10, O9
- `SOLID` (739-744): N5
- `LINEAR_GRADIENT` (747-765): N5
- `RADIAL_GRADIENT` (769-786): N5
- `CHECKER` (789-806): N5
- `NOISE` (812-861): N5
- `VORONOI` (866-927): N5
- `MANDELBROT` (932-976): N5
- `ALL` (978-980): R3

## `src/engine/mod.rs`

- `NodeError` (33-36): U12
- `NodeError::fmt` (39-41): U12
- `Engine::new` (68-84): U7
- `Engine::warm_up` (108-115): U6
- `Engine::active_gpu` (142-147): R4
- `Ctx::cancelled` (179-181): F4.2
- `Ctx::status` (184-186): U12
- `Ctx::render` (198-215): F2, F4.2, U12
- `Ctx::render_input` (219-224): F2
- `Ctx::source_on_gpu` (258-271): R3
- `Ctx::kernel` (274-324): F4.2, R3, R4

## `src/engine/pool.rs`

- `DEFAULT_CAP` (18-18): U6
- `DEFAULT_CAP` (20-20): U6

## `src/engine/resident.rs`

- `DEFAULT_BUDGET` (18-18): U6
- `DEFAULT_BUDGET` (20-20): U6
- `Resident::new` (109-118): R3

## `src/engine/source.rs`

- `SourceImage::from_rgba8` (43-65): F1
- `SourceImage::level_for` (114-121): D-RESIZE, O4
- `SourceImage::texel_map` (126-133): D-RESIZE, O4

## `src/engine/tile.rs`

- `catmull_rom` (111-120): O8

## `src/expr.rs`

- `IMAGE_DERIVED` (36-36): D-DERIVED-PARAMS, E8
- `image_file` (39-41): E5
- `ExprError` (49-53): E11
- `ExprError::fmt` (56-58): E11
- `err` (63-65): E11
- `Pos` (71-74): E11
- `Sexp` (77-83): E11
- `Sexp::pos` (86-90): E11
- `parse` (102-112): E1
- `Parser::skip_space` (139-151): E7
- `Parser::form` (152-202): E1
- `Doc::width` (223-237): E9
- `value_text` (293-302): E7
- `Printer::reference` (315-330): E2, E3
- `Printer::node` (332-372): D-DERIVED-PARAMS, E10, E2, E4, E5, E6, E8
- `to_text` (376-442): E1, E3, E4, E5, P11
- `visit` (396-404): E1
- `image_files` (448-466): E5
- `walk` (449-460): E5
- `build` (470-506): E1
- `from_text` (509-511): E1
- `Builder::input` (524-538): E2, E3
- `Builder::reference` (540-555): E3, S7
- `Builder::fill` (579-639): E2, E4, E5
- `Builder::attribute` (641-665): E6
- `Builder::layout` (668-695): E8
- `parse_value` (698-745): E7

## `src/formats.rs`

- `ImageCrate::encode` (83-110): D-EXPORT-MEMORY, F4.2, U10
- `Png::encode` (135-149): D-EXPORT-MEMORY, F4.2
- `CODECS` (152-164): F1, U10
- `decode` (183-186): F1
- `GraphBands::new` (222-234): F4.2
- `GraphBands::next_band` (244-256): F4.2
- `export` (259-300): F4.2
- `Prefetched::next_band` (320-327): F4.2

## `src/graph.rs`

- `Link` (18-21): S5
- `Link::new` (24-26): S5
- `Node` (37-44): F2
- `Graph` (67-72): F2
- `Graph::fresh_id` (109-113): D-STALE
- `Graph::add_node_at` (133-139): E8
- `Graph::link_title` (153-160): S7
- `Graph::depends_on` (224-239): F2
- `Graph::connect` (243-264): F1, F2, S5
- `Graph::size_of` (285-287): S7
- `Graph::canvas` (301-304): N2
- `Graph::extent_of` (306-308): N2
- `Graph::space_map` (313-316): F3.3.1
- `Graph::space_map_rec` (318-343): D-OVERLAY, F3.3.1
- `Graph::size_conflict` (347-363): N4, Z1
- `Graph::insert_resize` (367-375): Z1
- `Graph::link_hash` (408-416): S5
- `Graph::is_interior_stack` (426-432): F5
- `Graph::chain_head` (434-439): F5
- `Graph::stack_chain` (441-465): F5, O1
- `Graph::set_stack_layers` (470-491): F5, O1.1

## `src/nodes/basic.rs`

- `kinds` (10-12): O0
- `input_request` (16-18): N4
- `union_size` (22-24): N4, O1
- `Output::params` (46-48): N2
- `Output::size` (49-51): N2
- `StaticImage` (62-62): D1, F1
- `StaticImage::id` (65-67): D1
- `StaticImage::name` (68-70): D1, F1.1
- `StaticImage::category` (71-73): D1
- `StaticImage::description` (74-76): D1
- `StaticImage::params` (77-83): D1, F1
- `StaticImage::size` (84-86): D1, F1
- `StaticImage::render` (87-91): D1, F1
- `StaticImage::user_creatable` (92-94): D1
- `Stack::inputs` (114-116): F5, O1
- `Stack::size` (117-119): F5, O1
- `Stack::render` (120-130): F5, O1
- `Stack::combines_inputs` (131-133): F5
- `Disable::size` (156-158): O2
- `Disable::render` (159-161): O2
- `Invert::render` (187-193): O6
- `Blend::inputs` (213-215): F5
- `Blend::params` (216-218): F5, O9, U8
- `Blend::size` (219-221): F5, N4
- `Blend::render` (222-231): F5, O9
- `Blend::combines_inputs` (232-234): F5
- `ORIENTATIONS` (241-241): O3
- `SIDES` (242-242): O3
- `REFLECT_MODES` (243-243): O3
- `Reflect::params` (261-268): O3, U8
- `Reflect::overlays` (272-274): O3
- `Reflect::render` (275-301): O3
- `Clip::params` (324-331): O7
- `Clip::size` (332-336): O7
- `Clip::overlays` (337-339): O7
- `Clip::edit_view` (340-342): F3.2.1, O7
- `Clip::input_map` (343-348): F3.3.1, O7
- `Clip::render` (349-352): O7

## `src/nodes/bgremove.rs`

- `kinds` (18-20): O0
- `BackgroundRemoval::params` (42-51): O5, U8
- `BackgroundRemoval::render` (58-98): D-STALE, O5
- `compute_mask` (102-139): O5, R3
- `MANAGER` (108-108): O5, R3
- `resolve_model` (147-167): O5
- `compute_mask` (170-172): D-PLAT, O5

## `src/nodes/generators.rs`

- `Generator::id` (22-24): N1
- `Generator::name` (25-27): N1
- `Generator::category` (28-30): N1
- `Generator::description` (31-33): N1
- `Generator::params` (34-36): N1
- `Generator::size` (37-39): N1, N2
- `Generator::overlays` (40-42): N1
- `Generator::render` (43-45): N1
- `NOISE_TYPES` (61-61): N5
- `VORONOI_MODES` (62-62): N5
- `SOLID` (64-72): N5, U8
- `LINEAR` (74-95): N5, U8
- `RADIAL` (97-117): N5, U8
- `CHECKER` (119-135): N5, U8
- `NOISE` (137-170): N5, U8
- `VORONOI` (172-193): N5, U8
- `MANDELBROT` (195-216): N5, U8
- `kinds` (218-220): D1, N5

## `src/nodes/mod.rs`

- `ParamKind` (54-66): F1.2
- `ParamDef` (77-85): F1.2
- `Overlay` (171-182): F1.2
- `EditView` (186-192): F3.2.1
- `NodeKind` (194-249): D1, F3.2.1, F3.3.1, N2, S7
- `outputs` (206-208): S7
- `size` (214-214): N2
- `render_output` (219-222): S7
- `edit_view` (226-228): F3.2.1
- `input_map` (233-235): F3.3.1
- `combines_inputs` (238-240): D1
- `REGISTRY` (251-259): D1, O0

## `src/nodes/resize.rs`

- `kinds` (10-12): O0
- `MODES` (14-14): O8
- `FILTERS` (15-15): O8
- `Resize::params` (55-63): O8, U8
- `Resize::input_map` (73-81): F3.3.1
- `Resize::render` (82-121): D-RESIZE, O8
- `fourier_resample` (126-183): O8, R3
- `resize_spectrum` (187-206): O8
- `DOWN_METHODS` (210-210): O4
- `Downscale::params` (230-232): O4, U8
- `Downscale::size` (233-237): O4
- `Downscale::input_map` (238-245): F3.3.1, O4
- `Downscale::render` (246-253): O4

## `src/nodes/split.rs`

- `Split` (21-31): S1
- `VERTICAL` (33-41): S2, S3
- `HORIZONTAL` (43-51): S2, S3
- `RECTANGLE` (53-73): S2, S3
- `Split::size` (97-99): S6
- `Split::edit_view` (104-106): F3.2.1, S4
- `Split::render_output` (110-131): S1, S3, S6

## `src/project.rs`

- `PROJECT_FILE` (29-29): P8
- `INPUTS_DIR` (30-30): P8
- `Project` (34-36): P2
- `default_root` (39-45): P3.3
- `unique_dir` (55-64): P13.1
- `Project::create` (69-77): P3.3, P8
- `Project::add_input` (90-113): P6.1, P8, P9
- `Project::adopt_image` (117-133): P11.2, P9
- `Project::sync_inputs` (141-182): P10, P11.1, P11.2, P4
- `Project::save` (186-189): E1
- `Project::save_text` (191-193): P6.1
- `Project::resolve` (196-204): P15
- `Project::verify` (208-225): D-ERROR-COLUMN, E11, P14
- `Project::load` (229-251): E1, E9, P14
- `Project::share` (255-284): P12, P6.1
- `open` (289-301): D-IMPORT-LEFTOVER, P13, P13.2, P15
- `unpack` (305-364): D-IMPORT-LEFTOVER, P13.1, P14, P15
- `write_atomic` (392-399): P6.1

## `src/ui/dialogs.rs`

- `ConflictDialog` (12-64): D-CONFLICT, U9, Z1
- `ExportDialog` (69-200): F4.2, U10
- `ProjectDialog` (204-307): P3, P3.1

## `src/ui/graph_view.rs`

- `build` (62-109): D-MODALITIES, F5
- `connect_port` (112-138): F2, F5, O1.1
- `disconnect_ref` (163-178): O1.1
- `GraphView` (189-458): F3.1.1, S5, U14

## `src/ui/image_view.rs`

- `MAX_PREVIEW_PIXELS` (16-16): U5
- `ImageView` (31-361): D-OPEN-WRITES, D-STALE, F1.2, F2.1, F3.2, F3.3, F3.3.2, N2, N3, P6.2, U14, U5
- `render_overlay` (370-510): F1.2, F3.2, O3, S2

## `src/ui/inspector.rs`

- `NodePanel` (56-214): F1.2, F3.1, F3.2.1, F4, N3, S5, U4
- `ParamField` (249-401): F1.2
- `LayerStack` (404-527): D-SPEC-WORDING, F5, O1.1, U15

## `src/ui/mod.rs`

- `launch` (26-38): D-QUICK-EXIT, R2
- `use_services` (43-56): Q2
- `Root` (59-125): F1, F1.1, F3, P13.3, P7, U16
- `pump` (128-242): F2.1, F4.2, P13, P3.3, P5, U12
- `is_project_path` (246-249): P13.3
- `finish_import` (251-284): D-IMPORT, D1, F1.1
- `on_key` (286-308): F4, F4.2, U2
- `Toolbar` (311-446): F1, F3, F3.1, F3.2, F3.3, F4, O0, R4, U1, U13, X2
- `ProjectMenu` (461-553): P12, P13, P4, P7, X1
- `Footer` (558-589): P5

## `src/ui/platform.rs`

- `frame_url` (53-65): R2
- `choose_save_target` (120-141): U11, X1
- `deliver_bytes` (144-157): U11
- `HAS_PICKERS` (165-165): X1
- `pick_project_file` (169-177): P13
- `pick_folder` (180-190): P13
- `confirm` (219-236): D-QUICK-EXIT, P7

## `src/ui/session.rs`

- `Session` (15-24): P1
- `PROJECTS_SUPPORTED` (87-87): P3, X2
- `import` (100-120): F1, P3, P3.2, P3.3, X2
- `choose_quick` (123-131): P3.2
- `prompt_new_project` (134-137): P4
- `set_prompt_name` (140-145): P3.3
- `create_project` (154-165): P3.3, U16
- `on_project_created` (168-190): P3.3
- `open_project` (194-199): P13.1
- `on_project_opened` (202-216): P13, P14, P6.2
- `close_session` (219-223): U16
- `has_unsaved_work` (243-245): P7
- `use_autosave` (248-270): P1, P4, P6, P6.2
- `start_imports` (276-312): P1

## `src/ui/state.rs`

- `Layout` (12-16): F3
- `ExportState` (53-66): S5
- `ImportTarget` (70-75): F1.1
- `Displayed` (78-98): D-EXPORT-TARGET
- `History` (101-106): P11
- `App` (109-140): F3.1, S5
- `HISTORY_LIMIT` (142-142): U1
- `App::new` (145-175): F3.2
- `App::push_history` (179-187): U1
- `App::edit` (190-195): U1
- `App::edit_coalesced` (199-215): U1
- `App::replace_document` (247-260): D-OPEN-WRITES, P11, P6.2
- `App::undo` (262-272): U1
- `App::redo` (274-284): U1
- `App::fix_selection` (286-300): F3.1
- `App::after_edit` (304-313): Z1
- `App::ignore_conflict` (315-319): U9, Z1
- `App::fit_canvas_to_window` (323-344): D-OPEN-WRITES, N3, P6.2
- `App::displayed` (351-386): D-EXPORT-TARGET, F2.1, F3.1, F3.2, F3.2.1, F3.3, F3.3.2, F4, F4.1
- `App::open_export` (392-420): F4
- `App::export_backdrop_clicked` (436-440): F4.2
- `App::add_node` (452-488): D-ADD-AFTER-SPLIT, F1.2, N3, S5, U3
- `App::delete_node` (505-534): F3.1, U3

## `src/worker.rs`

- `ExportJob` (28-36): F4.1
- `Worker::run_preview` (179-225): F2.1, U5
- `Worker::import` (242-278): F1, P1, P9
- `Worker::export` (280-322): D-EXPORT-MEMORY, F4.2
- `Worker::create_project` (327-333): P3.3
- `Worker::save_project` (356-378): P10, P5, P6
- `Worker::share_project` (381-390): P12
- `Worker::cancel_export` (392-396): F4.2
- `new_stash_dir` (402-415): P11.1, U16
- `N` (413-413): P11.1, U16
- `run_background` (417-425): D-PLAT, R2
- `encode_display_bmp` (452-485): U5

