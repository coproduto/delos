# Unspecified behaviour: code without a spec clause

The clause files go from the spec to the code. This file goes the other
way: behaviour a user can see, or a resource policy a maintainer must
know about, that no spec sentence asks for. Each item was found by
reading the code, then checked with `spectrace uncited`, which lists the
functions no entry cites. The items below account for the user-visible
part of that list; what remains uncited is plumbing (accessors, layout
helpers, the WGSL prelude, test harness code).

This list was written by hand. Its automated counterpart is
[generated/ancestry.md](generated/ancestry.md) (`spectrace ancestry`). That
report flags 51% of the code lines the items below cite, against 32% of
the same files overall (×1.57 over chance), and adds user-reachable
groups that are not listed here. See [tightening.md](tightening.md) for
how this was measured.

Unspecified does not mean unwanted. Most of these are reasonable
defaults. They are listed because the spec is silent on them, so nothing
protects them from being changed or removed, and because some of them
are real product decisions (undo depth, memory budgets) that nobody has
written down.

## U1 · Undo and redo {#u1}

**What.** Every edit is an undo step holding a full copy of the graph; slider and handle drags within one second share a step; the history keeps 60 steps. ⌘Z, ⇧⌘Z and ⌘Y undo and redo.

**Where.** `HISTORY_LIMIT` and `push_history` ([state.rs:142](../../src/ui/state.rs#L142), [state.rs:179-187](../../src/ui/state.rs#L179)); `edit` and `edit_coalesced` ([state.rs:190-215](../../src/ui/state.rs#L190)); `undo` and `redo` ([state.rs:262-284](../../src/ui/state.rs#L262)); the toolbar buttons ([mod.rs:395-396](../../src/ui/mod.rs#L395)).

**Why it matters.** The spec mentions undo only to say it is not part of a project ([P11](04-sessions-and-projects.md#p11)). Its depth and memory cost (each step may keep decoded images alive) are product decisions. Undo also caused the identity defect [D-STALE](findings.md#d-stale).

## U2 · Keyboard shortcuts {#u2}

**What.** ⌘E exports, Delete or Backspace deletes the selection, Escape closes the export dialog or prompt or clears the selection, F fits the image.

**Where.** `on_key` ([mod.rs:286-308](../../src/ui/mod.rs#L286)).

## U3 · Adding and deleting nodes rewire the graph {#u3}

**What.** "Add node" inserts the new node after the selected one, taking over its consumers; with nothing selected it feeds an empty output. Deleting a node with one input bridges its input to its consumers; deleting a layer stack deletes the whole chain.

**Where.** `App::add_node` ([state.rs:452-488](../../src/ui/state.rs#L452)), `App::delete_node` ([state.rs:505-534](../../src/ui/state.rs#L505)).

**Why it matters.** These rules decide what a user's graph looks like after the two most common edits, and they caused [D-ADD-AFTER-SPLIT](findings.md#d-add-after-split).

## U4 · The inspector beyond parameters {#u4}

**What.** Nodes can be renamed (the title becomes `:title`); every input port has a picker listing all outputs that would not form a cycle; each node shows its size, description, "slow" hint and last error.

**Where.** The title field, which sets `label` ([inspector.rs:104-118](../../src/ui/inspector.rs#L104)), input pickers built from `candidates` ([inspector.rs:160-188](../../src/ui/inspector.rs#L160)), `slow_hint` ([inspector.rs:124-129](../../src/ui/inspector.rs#L124)).

## U5 · How previews are produced {#u5}

**What.** Previews render only the visible region at screen resolution, capped at 10 Mpx (`MAX_PREVIEW_PIXELS`); a newer request cancels an older one; when renders are slow a quarter-resolution frame is shown first; frames are composited over a checkerboard and sent to the webview as BMP.

**Where.** The request effect ([image_view.rs:83-125](../../src/ui/image_view.rs#L83)) and `MAX_PREVIEW_PIXELS` ([image_view.rs:15-16](../../src/ui/image_view.rs#L15)); latest-wins and progressive refinement in `run_preview` ([worker.rs:179-225](../../src/worker.rs#L179)); `encode_display_bmp` ([worker.rs:449-485](../../src/worker.rs#L449)).

**Why it matters.** This is how Delos meets the INSTRUCTIONS' "effectively instant" principle, which the spec turns into a single test clause ([Q3](06-quality.md#q3)). The 80 ms threshold and the 10 Mpx cap are unrecorded tuning decisions.

## U6 · GPU memory policy {#u6}

**What.** Source image levels are kept on the GPU up to 4 GiB (1 GiB on mobile) and evicted least recently used first; idle textures and readback buffers are pooled up to 512 MiB (128 MiB on mobile); every kernel is compiled at start-up; opt-in profiling times every operation.

**Where.** `DEFAULT_BUDGET` ([resident.rs:16-20](../../src/engine/resident.rs#L16)), the `Pool` and its `DEFAULT_CAP` ([pool.rs:1-20](../../src/engine/pool.rs#L1)), `warm_up` ([engine/mod.rs:106-115](../../src/engine/mod.rs#L106)), `Profile` ([profile.rs:1-14](../../src/engine/profile.rs#L1)).

**Why it matters.** The INSTRUCTIONS ask for minimal memory use; these budgets are where that principle became numbers. `NEXT_FEATURES.md` plans to expose them in a settings menu.

## U7 · The expensive-result cache {#u7}

**What.** Background-removal masks are cached in a 256 MiB LRU keyed by structural hash; nothing else is cached.

**Where.** `Cache` ([cache.rs:1-49](../../src/engine/cache.rs#L1)), created in `Engine::new` with `256 << 20` ([engine/mod.rs:81](../../src/engine/mod.rs#L81)).

## U8 · Operator parameters the spec does not name {#u8}

**What.** Reflection's direction (`side`); Blend's `opacity` and five extra modes; Resize's exact-size mode; Downscale's `method`; Background Removal's `invert`, `resolution` and `model`; every generator's parameters (colours, seeds, octaves, zoom, palette).

**Where.** `Reflect::params` ([basic.rs:261-268](../../src/nodes/basic.rs#L261)), `Blend::params` ([basic.rs:216-218](../../src/nodes/basic.rs#L216)), `Resize::params` ([resize.rs:55-63](../../src/nodes/resize.rs#L55)), `Downscale::params` ([resize.rs:230-232](../../src/nodes/resize.rs#L230)), `BackgroundRemoval::params` ([bgremove.rs:42-51](../../src/nodes/bgremove.rs#L42)), the generator definitions from `SOLID` on ([generators.rs:64-216](../../src/nodes/generators.rs#L64)).

**Why it matters.** Saved projects write every parameter ([E8](05-expressions.md#e8)), so each of these names and defaults is now part of the file format.

## U9 · A third answer to a size conflict {#u9}

**What.** Besides resizing one input ([Z1](03-operators.md#z1)), the user can keep both sizes, aligned top-left; the answer is remembered per node and pair of sizes.

**Where.** The "Keep both sizes" button calling `ignore_conflict` ([dialogs.rs:58](../../src/ui/dialogs.rs#L58), [state.rs:315-319](../../src/ui/state.rs#L315)).

## U10 · Image formats {#u10}

**What.** PNG, JPEG, WebP (lossless), GIF, BMP and TIFF are read and written; ICO is read only; JPEG exports are composited over white; JPEG quality is adjustable.

**Where.** `CODECS` ([formats.rs:152-164](../../src/formats.rs#L152)); the JPEG branch of `ImageCrate::encode` ([formats.rs:91-102](../../src/formats.rs#L91)); the export dialog's format and `quality` fields ([dialogs.rs:129-169](../../src/ui/dialogs.rs#L129)).

**Why it matters.** The INSTRUCTIONS ask for "most common web formats" with an extensible design; the spec says nothing about formats. AVIF is missing (README).

## U11 · Where exports go {#u11}

**What.** Desktop asks with a save dialog; mobile writes to `~/Documents` without asking; the web downloads the file.

**Where.** `choose_save_target` and `deliver_bytes` ([platform.rs:119-157](../../src/ui/platform.rs#L119)).

## U12 · Errors and progress shown in place {#u12}

**What.** A failing node is marked in the graph view and its error shown in the image view and the inspector; long steps (Fourier resampling, the neural network) show a status line; imports show a spinner.

**Where.** `NodeError` ([engine/mod.rs:31-44](../../src/engine/mod.rs#L31)) attached by `Ctx::render` ([engine/mod.rs:208-214](../../src/engine/mod.rs#L208)); `RenderError` and `Status` in `pump` ([mod.rs:162-179](../../src/ui/mod.rs#L162)); `Ctx::status` ([engine/mod.rs:183-186](../../src/engine/mod.rs#L183)).

## U13 · Choosing the backend at run time {#u13}

**What.** `DELOS_NO_GPU` forces the CPU; the toolbar badge switches between GPU and CPU for the session.

**Where.** `Gpu::new` ([gpu.rs:93-96](../../src/engine/gpu.rs#L93)); the badge (`use_gpu`) ([mod.rs:429-443](../../src/ui/mod.rs#L429)).

## U14 · Navigating the views {#u14}

**What.** The image view zooms with the wheel, pans by dragging, and has Fit (also double-click) and 1:1; the graph view pans, zooms, has Home, and lets wires be dragged, picked up and selected.

**Where.** Image view handlers (`onwheel`, `Drag::Pan`) ([image_view.rs:207-244](../../src/ui/image_view.rs#L207)) and its status bar buttons, Fit, 1:1 and `zoom_by` ([image_view.rs:340-358](../../src/ui/image_view.rs#L340)); graph view handlers (`GDrag`) ([graph_view.rs:256-295](../../src/ui/graph_view.rs#L256)) and tools ([graph_view.rs:432-455](../../src/ui/graph_view.rs#L432)).

## U15 · Layer stacks grow from the inspector {#u15}

**What.** Besides disabling and removing ([O1.1](03-operators.md#o1-1)), layers can be reordered, and the inspector adds image layers, generator layers and empty slots.

**Where.** `LayerStack` actions ([inspector.rs:469-523](../../src/ui/inspector.rs#L469)).

## U16 · Session housekeeping {#u16}

**What.** Session stashes older than a day are deleted at start-up; a new project's name must not collide with a non-empty folder; the window title shows the project or mode; "Close project" starts over.

**Where.** `new_stash_dir` ([worker.rs:399-415](../../src/worker.rs#L399)); the collision check in `create_project` ([session.rs:156-161](../../src/ui/session.rs#L156)); the title effect (`set_title`) ([mod.rs:62-70](../../src/ui/mod.rs#L62)); `close_session` ([session.rs:218-223](../../src/ui/session.rs#L218)).

**Why it matters.** Pruning at a day is what makes [P11.2](04-sessions-and-projects.md#p11-2) fall back to PNG in long sessions with two instances running.
