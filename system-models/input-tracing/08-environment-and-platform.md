# 08 · Environment, platform and view toggles

The inputs in this file are not pointer gestures on the document. They come
from how and where Delos is launched (environment variables, build
features), from the machine (GPU, display), from the webview that hosts the
UI, and from the toolbar switches that only change what is shown.

## Environment variables

| Variable | Read at | Effect |
|---|---|---|
| `DELOS_NO_GPU` | [gpu.rs:94](../../src/engine/gpu.rs#L94) | Present = no GPU, CPU backend only. Read once, when the `Worker` builds the `Engine`. |
| `DELOS_PROJECTS_DIR` | [project.rs:40](../../src/project.rs#L40) | Where new projects and unpacked archives go. Default `~/.delos/projects`, from `HOME` or `USERPROFILE` ([project.rs:43](../../src/project.rs#L43)). Read once, when `App` is created. |
| `DELOS_REMBG_MODEL` | [bgremove.rs:153](../../src/nodes/bgremove.rs#L153) | Second candidate for the background-removal model, after the node's own path parameter and before `~/.delos/models/*.onnx` ([bgremove.rs:156](../../src/nodes/bgremove.rs#L156)). Read on **every** mask computation. |
| `HOME` (mobile) | [platform.rs:132](../../src/ui/platform.rs#L132) | Exports go to `~/Documents`. |
| `ORT_DYLIB_PATH` | the `ort` crate | Where ONNX Runtime is loaded from. Delos only names it in an error message ([bgremove.rs:130](../../src/nodes/bgremove.rs#L130)). |
| `WGPU_*` | wgpu, through `new_without_display_handle_from_env` ([gpu.rs:112](../../src/engine/gpu.rs#L112)) | wgpu's own backend and adapter overrides. |

Build features choose whole input surfaces at compile time
([Cargo.toml](../../Cargo.toml)). `desktop` brings native pickers, confirm
dialogs, drag-and-drop of projects and background removal. `mobile` brings
projects without pickers. `web` brings quick mode only, downloads and a CPU
backend.

## Trace: the GPU at start-up and the backend badge

1. `Engine::new(true)` → `Gpu::new` ([gpu.rs:93](../../src/engine/gpu.rs#L93)): unless `DELOS_NO_GPU` is set, it asks for a high-performance adapter and refuses a software adapter. A failure or panic means CPU. `use_services` copies the backend name into `App.backend` ([mod.rs:47-48](../../src/ui/mod.rs#L47)).
2. The preview thread compiles every kernel pipeline before its first job (`warm_up`).
3. **The badge** ([mod.rs:432-441](../../src/ui/mod.rs#L432)), when a GPU exists, flips `engine.use_gpu`, rewrites `App.backend` and touches `app.graph.write()` "to re-render". ⇢ effects: the preview key includes the backend, so it renders again on the other backend. The graph write is redundant for that. It also reruns autosave, which saves an unchanged graph (no file write, because the text compares equal).
4. At render time, a GPU that is unhealthy, a tile that is too big, or a kernel without a pipeline falls back to the CPU per kernel ([engine/mod.rs:283-309](../../src/engine/mod.rs#L283)).

## Trace: frames through the webview

1. Device pixel ratio: once, at mount, `document::eval("return window.devicePixelRatio")` ([platform.rs:161](../../src/ui/platform.rs#L161)), clamped to ½–4 → local `dpr` ⇢ the **preview** effect asks for `zoom × dpr` pixels. A later change (moving the window to another display) is **not** picked up.
2. Every finished frame becomes a BMP in a four-frame ring ([platform.rs:18](../../src/ui/platform.rs#L18)). `<img src="/delosframe/<seq>.bmp">` makes the webview request it. The custom protocol handler ([platform.rs:40](../../src/ui/platform.rs#L40)) answers from memory. The URL is the only input it takes, and anything not in the ring gets a 404.

## Trace: background removal's configuration

This node's inputs come from three places at once: its **parameters**
(threshold, flags, resolution and the model path *text field*), the
**environment** and the **file system**.

1. Rendering reaches `BackgroundRemoval::render` ([bgremove.rs:58](../../src/nodes/bgremove.rs#L58)). The cache key is (input's structural hash, threshold, flags, resolution, model path text, extent) ([bgremove.rs:66-70](../../src/nodes/bgremove.rs#L66)).
2. On a cache miss it renders the input at up to the chosen mask resolution, then reports status "Removing background…" ⇢ `Msg::Status` (busy text). It calls `compute_mask` ([bgremove.rs:102](../../src/nodes/bgremove.rs#L102)).
3. `resolve_model` ([bgremove.rs:147](../../src/nodes/bgremove.rs#L147)) uses the first file that exists among: path param, `$DELOS_REMBG_MODEL`, `~/.delos/models/{u2net,…}.onnx`. The model is loaded once per path (a global mutex). If ONNX Runtime cannot be loaded, the panic is caught and becomes an error naming `ORT_DYLIB_PATH`.
4. The mask goes into the 256 MiB LRU. Any error becomes a `NodeError` ⇢ `Msg::RenderError` → red node + message.

The cache key contains the path *text*, not the model file it resolves to.
With an empty path field, putting a different model into
`~/.delos/models`, or replacing the file at the same path, does not
invalidate masks already cached in this session.

## View toggles (no document change)

| Input | Entry | Writes | Reruns |
|---|---|---|---|
| Image / Split / Graph layout | [mod.rs:337](../../src/ui/mod.rs#L337) | `App.layout` | re-render only. Unmounting the image view drops its local state (camera drag). |
| "Preview selection" checkbox | [mod.rs:414](../../src/ui/mod.rs#L414) | `App.follow_selection` | `disp` → fit, canvas, preview |
| "Output relocated to …" **Reset** | [mod.rs:422](../../src/ui/mod.rs#L422) | `App.pinned` | `disp` → fit, canvas, preview |
| View as output / View left / View right | [inspector.rs:146](../../src/ui/inspector.rs#L146) | `App.pinned` | same |
| Click a toast to dismiss it | [mod.rs:456](../../src/ui/mod.rs#L456) | `App.toast` | none |

These change `displayed()` and therefore what renders. They are not saved
and not undoable. Through the canvas effect, though, a view toggle that
makes a size-less node visible can still resize the canvas
([04-image-view.md](04-image-view.md#canvas)).

## Decisions

- **Environment is read at different times**: once per process (GPU), once per `App` (projects root), or on every use (model path). Nothing documents which is which.
- **Configuration that affects pixels is outside the cache key** (which model file is found). This is acceptable as long as model files don't change during a session.
- **Display changes are ignored after start-up** (DPR).
