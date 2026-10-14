| File | Functions | Cited | Cited share of fn lines |
|---|---|---|---|
| `src/engine/geom.rs` | 27 | 0 | 0% |
| `src/engine/pool.rs` | 14 | 0 | 0% |
| `src/engine/profile.rs` | 8 | 0 | 0% |
| `src/main.rs` | 1 | 0 | 0% |
| `src/engine/resident.rs` | 14 | 1 | 2% |
| `src/engine/tile.rs` | 12 | 1 | 9% |
| `src/nodes/mod.rs` | 38 | 6 | 13% |
| `src/engine/gpu.rs` | 16 | 4 | 16% |
| `src/engine/source.rs` | 14 | 3 | 25% |
| `src/engine/kernels.rs` | 28 | 7 | 32% |
| `src/ui/graph_view.rs` | 11 | 4 | 37% |
| `src/ui/session.rs` | 20 | 12 | 40% |
| `src/ui/platform.rs` | 14 | 6 | 42% |
| `src/ui/dialogs.rs` | 3 | 3 | 43% |
| `src/engine/mod.rs` | 26 | 10 | 44% |
| `src/nodes/split.rs` | 13 | 3 | 48% |
| `src/worker.rs` | 18 | 10 | 52% |
| `src/formats.rs` | 40 | 7 | 53% |
| `src/graph.rs` | 44 | 18 | 53% |
| `src/ui/mod.rs` | 11 | 10 | 56% |
| `src/ui/state.rs` | 28 | 16 | 59% |
| `src/nodes/basic.rs` | 71 | 34 | 60% |
| `src/ui/image_view.rs` | 3 | 2 | 68% |
| `src/nodes/resize.rs` | 23 | 10 | 71% |
| `src/expr.rs` | 34 | 23 | 72% |
| `src/nodes/bgremove.rs` | 14 | 6 | 76% |
| `src/ui/inspector.rs` | 9 | 3 | 77% |
| `src/nodes/generators.rs` | 11 | 9 | 77% |
| `src/project.rs` | 20 | 15 | 80% |
| `src/engine/cache.rs` | 4 | 4 | 100% |

- `src/engine/geom.rs`: `Size::new (16-18)`, `Size::rect (20-22)`, `Size::min_dim (24-26)`, `Size::pixels (28-30)`, `Rect::new (42-44)`, `Rect::right (46-48)`, `Rect::bottom (50-52)`, `Rect::is_empty (54-56)`, `Rect::intersect (58-64)`, `Rect::translate (66-68)`, `Map1::apply (81-83)`, `Map1::after (86-88)`, `Map1::inverse (90-92)`, `Map2::new (107-109)`, `Map2::after (111-113)`, `Map2::inverse (115-117)`, `Map2::apply_rect (120-126)`, `Map2::as_f32 (128-130)`, `Request::new (160-162)`, `Request::native (165-168)`, `Request::step_x (170-172)`, `Request::step_y (174-176)`, `Request::pixel_to_node (180-182)`, `Request::pixel_count (184-186)`, `Request::mapped (190-192)`, `Request::native_cover (196-208)`, `Request::texel_map_to (212-215)`
- `src/engine/gpu.rs`: `Texture::drop (35-39)`, `GpuTile::view (43-45)`, `GpuTile::from (49-51)`, `Gpu::make_texture (197-211)`, `Gpu::healthy (213-215)`, `Gpu::tile (253-256)`, `Gpu::upload (258-271)`, `Gpu::download (273-326)`, `Gpu::run (330-397)`, `Gpu::timer (400-411)`, `Gpu::collect (415-430)`, `Gpu::finish (434-436)`
- `src/engine/kernels.rs`: `Row::fill (41-46)`, `EnvInput::aligned_row (65-69)`, `EnvInput::sample_row (72-87)`, `EnvInput::row (76-80)`, `Env::node (92-95)`, `Env::color (98-100)`, `wgsl_source (193-202)`, `screen (373-375)`, `color_dodge (377-379)`, `color_burn (381-383)`, `hard_light (385-387)`, `soft_light (389-396)`, `lum (448-450)`, `clip_color (451-467)`, `hash_u (693-697)`, `hash2 (699-701)`, `hashf (703-705)`, `fade (709-711)`, `perlin (713-730)`, `mix_px (733-736)`, `run_cpu (983-1010)`
- `src/engine/mod.rs`: `Cancelled::fmt (50-52)`, `Engine::profile_samples (88-94)`, `Engine::backend_name (96-104)`, `Engine::make_resident (119-126)`, `Engine::resident (130-132)`, `Engine::has_gpu (134-139)`, `KernelInput::aligned (160-162)`, `Ctx::new (175-177)`, `Ctx::node_hash (188-190)`, `Ctx::link_hash (193-195)`, `Ctx::input_size (226-228)`, `Ctx::has_input (230-232)`, `Ctx::to_cpu (234-241)`, `Ctx::source (245-252)`, `render_to_cpu (328-339)`, `is_cancelled (341-343)`
- `src/engine/pool.rs`: `texture_bytes (53-55)`, `Pool::new (58-60)`, `Pool::texture (64-73)`, `Pool::buffer (77-82)`, `Pool::put_texture (84-87)`, `Pool::put_buffer (90-92)`, `Pool::take (94-108)`, `Pool::put (110-119)`, `Pool::trim (121-128)`, `Pool::cap (130-132)`, `Pool::set_cap (135-138)`, `Pool::idle_bytes (141-143)`, `Pool::created (147-149)`, `Pool::default (153-155)`
- `src/engine/profile.rs`: `Profile::enable (45-47)`, `Profile::enabled (49-51)`, `Profile::record (53-55)`, `Profile::time (58-66)`, `Profile::take (69-71)`, `Summary::mpx_per_s (87-89)`, `summarize (93-115)`, `report (118-134)`
- `src/engine/resident.rs`: `Registry::drop_dead (62-72)`, `Registry::make_room (76-91)`, `Registry::evict (93-99)`, `Resident::prefetch (122-126)`, `Resident::get (130-142)`, `Resident::is_resident (144-146)`, `Resident::bytes (149-153)`, `Resident::budget (155-157)`, `Resident::set_budget (162-168)`, `Resident::queue (170-184)`, `level_bytes (187-189)`, `upload_loop (191-232)`, `upload (236-276)`
- `src/engine/source.rs`: `SourceImage::file (67-69)`, `SourceImage::set_project_file (71-73)`, `SourceImage::clear_project_file (75-77)`, `SourceImage::set_origin (79-81)`, `SourceImage::level_count (83-85)`, `SourceImage::level (89-92)`, `SourceImage::bytes (94-96)`, `SourceImage::to_rgba8 (99-110)`, `SourceImage::render (135-183)`, `Tap::new (198-211)`, `downsample (214-235)`
- `src/engine/tile.rs`: `CpuTile::new (21-23)`, `CpuTile::from_fn_par (25-33)`, `CpuTile::get (36-42)`, `CpuTile::get_clamped (45-49)`, `CpuTile::bytes (51-53)`, `CpuTile::sample (59-106)`, `clamp_premul (123-126)`, `unpremul (137-139)`, `premul (142-144)`, `Tile::dims (156-163)`, `Tile::is_clear (165-167)`
- `src/expr.rs`: `Sexp::describe (91-98)`, `Parser::peek (122-124)`, `Parser::pos (125-127)`, `Parser::bump (128-138)`, `Doc::flat (215-220)`, `Doc::write (238-262)`, `quote (265-279)`, `is_symbol (281-286)`, `is_name (289-291)`, `file_name (304-306)`, `Builder::node (558-576)`
- `src/formats.rs`: `EncodeOptions::default (27-29)`, `width (34-34)`, `height (35-35)`, `next_band (37-37)`, `name (41-41)`, `extensions (42-42)`, `mime (43-43)`, `can_decode (44-44)`, `decode (45-45)`, `can_encode (46-46)`, `encode (47-47)`, `ImageCrate::name (60-62)`, `ImageCrate::extensions (63-65)`, `ImageCrate::mime (66-68)`, `ImageCrate::can_decode (69-71)`, `ImageCrate::decode (72-79)`, `ImageCrate::can_encode (80-82)`, `Png::name (117-119)`, `Png::extensions (120-122)`, `Png::mime (123-125)`, `Png::can_decode (126-128)`, `Png::decode (129-131)`, `Png::can_encode (132-134)`, `codecs (166-168)`, `encoders (170-172)`, `codec_by_name (174-176)`, `accept_attr (179-181)`, `tile_to_rgba8 (189-200)`, `to8 (203-205)`, `GraphBands::width (238-240)`, `GraphBands::height (241-243)`, `Prefetched::width (314-316)`, `Prefetched::height (317-319)`
- `src/graph.rs`: `Link::from (31-33)`, `Node::kind (47-49)`, `Node::title (51-53)`, `Node::input_nodes (56-58)`, `Node::input_node (61-63)`, `Graph::fmt (91-93)`, `Graph::default (97-99)`, `Graph::new (103-107)`, `Graph::free_position (116-125)`, `Graph::add_node (127-130)`, `Graph::add_image (141-145)`, `Graph::node (147-149)`, `Graph::has_link (163-165)`, `Graph::node_mut (167-169)`, `Graph::set_param (171-175)`, `Graph::remove_node (179-186)`, `Graph::unlink (189-197)`, `Graph::gc_images (199-203)`, `Graph::consumers (206-208)`, `Graph::links_from (211-221)`, `Graph::disconnect (266-272)`, `Graph::sizes (276-282)`, `Graph::size_memo (289-298)`, `Graph::hash_of (381-405)`, `Graph::is_stack (420-422)`, `hash_value (494-506)`
- `src/main.rs`: `main (1-3)`
- `src/nodes/basic.rs`: `Output::id (31-33)`, `Output::name (34-36)`, `Output::category (37-39)`, `Output::description (40-42)`, `Output::inputs (43-45)`, `Output::render (52-54)`, `Output::user_creatable (55-57)`, `Stack::id (102-104)`, `Stack::name (105-107)`, `Stack::category (108-110)`, `Stack::description (111-113)`, `Disable::id (141-143)`, `Disable::name (144-146)`, `Disable::category (147-149)`, `Disable::description (150-152)`, `Disable::inputs (153-155)`, `Invert::id (169-171)`, `Invert::name (172-174)`, `Invert::category (175-177)`, `Invert::description (178-180)`, `Invert::inputs (181-183)`, `Invert::size (184-186)`, `Blend::id (201-203)`, `Blend::name (204-206)`, `Blend::category (207-209)`, `Blend::description (210-212)`, `Reflect::id (246-248)`, `Reflect::name (249-251)`, `Reflect::category (252-254)`, `Reflect::description (255-257)`, `Reflect::inputs (258-260)`, `Reflect::size (269-271)`, `Clip::id (309-311)`, `Clip::name (312-314)`, `Clip::category (315-317)`, `Clip::description (318-320)`, `Clip::inputs (321-323)`
- `src/nodes/bgremove.rs`: `BackgroundRemoval::id (27-29)`, `BackgroundRemoval::name (30-32)`, `BackgroundRemoval::category (33-35)`, `BackgroundRemoval::description (36-38)`, `BackgroundRemoval::inputs (39-41)`, `BackgroundRemoval::size (52-54)`, `BackgroundRemoval::slow_hint (55-57)`, `to8 (142-144)`
- `src/nodes/generators.rs`: `colors (48-52)`, `color_pair (57-59)`
- `src/nodes/mod.rs`: `Category::label (31-40)`, `ParamDef::new (88-90)`, `ParamDef::float (91-93)`, `ParamDef::fraction (94-96)`, `ParamDef::int (97-99)`, `ParamDef::bool (100-102)`, `ParamDef::choice (103-105)`, `ParamDef::color (106-108)`, `ParamDef::text (109-111)`, `ParamDef::hidden (112-114)`, `ParamDef::when (115-118)`, `Params::f (125-132)`, `Params::i (133-139)`, `Params::b (140-142)`, `Params::choice (143-148)`, `Params::color (149-154)`, `Params::text (155-157)`, `Params::set (158-160)`, `Params::choice_index (162-165)`, `id (196-196)`, `name (197-197)`, `category (198-198)`, `description (199-199)`, `inputs (201-203)`, `params (209-211)`, `render (216-216)`, `overlays (223-225)`, `slow_hint (242-244)`, `user_creatable (246-248)`, `registry (261-263)`, `kind (265-267)`, `default_params (269-275)`
- `src/nodes/resize.rs`: `resize_to (17-21)`, `Resize::factors (26-36)`, `Resize::id (40-42)`, `Resize::name (43-45)`, `Resize::category (46-48)`, `Resize::description (49-51)`, `Resize::inputs (52-54)`, `Resize::size (64-72)`, `Downscale::id (215-217)`, `Downscale::name (218-220)`, `Downscale::category (221-223)`, `Downscale::description (224-226)`, `Downscale::inputs (227-229)`
- `src/nodes/split.rs`: `kinds (14-16)`, `Split::id (76-78)`, `Split::name (79-81)`, `Split::category (82-84)`, `Split::description (85-87)`, `Split::inputs (88-90)`, `Split::outputs (91-93)`, `Split::params (94-96)`, `Split::overlays (100-102)`, `Split::render (107-109)`
- `src/project.rs`: `sanitize_name (48-52)`, `Project::name (79-81)`, `Project::file (83-85)`, `collect_files (366-382)`, `tmp_path (384-388)`
- `src/ui/graph_view.rs`: `DNode::height (46-49)`, `DNode::out_top (51-53)`, `DNode::out_port (54-56)`, `DNode::in_port (57-59)`, `disconnect_port (141-145)`, `decode_edge (147-153)`, `encode_edge (155-161)`
- `src/ui/image_view.rs`: `zoom_by (363-367)`
- `src/ui/inspector.rs`: `Inspector (14-27)`, `DocumentPanel (30-53)`, `visible (216-221)`, `ParamDefProp::eq (226-228)`, `hex (234-237)`, `parse_hex (239-246)`
- `src/ui/mod.rs`: `Toast (449-458)`
- `src/ui/platform.rs`: `store (18-24)`, `get (26-28)`, `use_frame_server (33-50)`, `blob_url (78-96)`, `download (98-107)`, `device_pixel_ratio (160-162)`, `pick_save_path (194-199)`, `choose_save_target_titled (202-215)`
- `src/ui/session.rs`: `Session::project (28-33)`, `Session::project (35-37)`, `ImportSource::name (60-66)`, `ProjectPrompt::eq (81-83)`, `default_projects_root (89-96)`, `cancel_prompt (147-149)`, `share_project (226-230)`, `share_file_name (233-240)`
- `src/ui/state.rs`: `ViewState::default (29-31)`, `App::set_param (217-219)`, `App::can_undo (221-223)`, `App::can_redo (225-227)`, `App::all_images (230-243)`, `App::export_pressed (423-430)`, `App::select (444-447)`, `App::delete_selection (491-501)`, `App::stack_chain (536-538)`, `App::toast (540-542)`, `conflict_key (545-548)`, `fmt_size (550-552)`
- `src/worker.rs`: `now_ms (102-112)`, `Worker::new (115-153)`, `Worker::backend_name (155-157)`, `Worker::preview (159-177)`, `Worker::send_frame (227-239)`, `Worker::open_project (338-350)`, `spawn_local (428-433)`, `yield_now (437-447)`
