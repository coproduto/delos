# Delos

A generative image editor built around a node graph (Rust + Dioxus).

## Running

```sh
cargo run --release                 # desktop (default; GPU via wgpu, CPU fallback)
dx serve --platform web             # web (CPU, single-threaded)
dx serve --platform ios|android     # mobile (feature `mobile`; untested)
```

- `DELOS_NO_GPU=1` forces the CPU backend. The toolbar badge shows the backend; click it to switch.
- **Background removal** needs a U²-Net ONNX model plus the ONNX Runtime library, because `rembg-rs` loads
  ORT dynamically. Put `u2net.onnx` in `~/.delos/models/`, set `DELOS_REMBG_MODEL`, or enter a path in the node's settings.
  Install ORT with `brew install onnxruntime`, or point `ORT_DYLIB_PATH` at the library. It is desktop-only.

## Projects and quick mode

The first time you import an image, Delos asks whether to start a **project**:

- **Quick mode** is for one-off edits. Images are used where they are and nothing is saved.
- **A project** is a folder, created in `~/.delos/projects/<name>` by default (`DELOS_PROJECTS_DIR` overrides this):

  ```text
  <name>/
    project.delos   the node graph as a Delos expression
    inputs/         a copy of every imported image
  ```

  Every change is saved automatically. Because images are copied, projects take more disk space than the originals.

  `inputs/` holds exactly the images the graph uses. Each save copies in new images and moves unused ones out.
  Undo history belongs to the editing session, not the project. Files it may still need are kept in a session stash in the system temp folder.
  If an undo brings an image back, its original bytes are copied back into `inputs/`.
  Opening a project changes nothing on disk until you edit it.

The footer shows the current mode and the save state. The **Project** menu has these commands:

- **New project from current work**: copies the images the graph uses into a new project.
- **Import project**: accepts a `.delosproject` file or a `project.delos` file. Archives are unpacked into the projects folder.
- **Open project folder**: opens a project in place.
- **Share project**: writes a `.delosproject` file, which is a zip of the project folder.
- **Close / End quick session**: starts over.

You can also open a project by dropping it onto the window.

**Platform status (temporary):** projects work fully on desktop. Mobile supports projects but has no file pickers, so Import and Share are unavailable there. The web build has no file system and always uses quick mode. Full functionality on every platform is planned.

### Delos expressions

`project.delos` is plain text in a small Lisp-like syntax (`src/expr.rs`):

```lisp
; Nodes used more than once are bound with def; everything else nests.
(def photo (image "inputs/photo.png" :at (60 40)))
(output
  (stack
    (invert photo)
    (blend photo (noise :scale 0.3) :mode screen))
  :canvas_w 1920 :canvas_h 1080)
(noise :seed 4)   ; a node that is not connected to anything
```

- A node is written `(kind input… :param value…)`. Inputs come in port order, and `_` leaves a port empty.
- Every parameter is written, defaults included, so a saved project keeps its meaning if a default changes. When reading, omitted parameters take their defaults, which keeps hand-written expressions short.
- `(stack front … back)` is a layer stack.
- `:at (x y)` is the node's position in the graph view, and `:title` sets a custom title. Nodes without `:at` are laid out automatically, so expressions can be written by hand.
- Saving and loading reproduce the same graph, which renders the same pixels. A 3,400-node graph serializes in about 5 ms and loads in about 16 ms.

## Tests

```sh
cargo test                                                  # correctness + GPU/CPU parity
cargo test --release --test large_images -- --nocapture     # 16K (16384²) timings
```

| Suite | What it covers |
|---|---|
| `src/expr.rs` (unit) | Expression parsing and printing, round trips, error messages with line:col |
| `tests/project.rs` | Project layout, input copies, save/load (compared pixel by pixel), verification, unsafe paths, share/import |
| `tests/project_e2e.rs` | End to end, headless: the app's real state, worker and autosave driven like the UI (prompt, quick mode, autosave, reopen, share, import, errors) |
| `tests/ui_state.rs` | UI state operations (regression tests) |
| `tests/engine.rs`, `tests/large_images.rs` | Rendering correctness and 16K performance |

On an M1 Pro, a 16K image goes through invert → reflect → overlay-blend with noise → stack with a gradient.
The whole image previews fitted in ~40 ms and a 1:1 view takes ~70 ms (CPU or GPU).
A 16K generator renders for export in ~1.7 s using about 64 MiB of working memory.

## Architecture

| Path | Role |
|---|---|
| `src/engine/geom.rs` | `Request`: "render this region of node space as a W×H grid". Every node maps requests to its inputs. |
| `src/engine/mod.rs` | `Ctx`: evaluation, kernel dispatch (GPU when possible, CPU otherwise), per-node errors, cancellation. |
| `src/engine/kernels.rs` | Per-pixel kernels written twice: WGSL (GPU) and Rust (CPU), with the same parameter layout. |
| `src/engine/gpu.rs` | wgpu compute backend. Tiles stay on the GPU between consecutive GPU kernels. |
| `src/engine/source.rs` | Imported images: premultiplied RGBA8 mip pyramid. |
| `src/nodes/` | Node kinds. Each implements `NodeKind` and is registered in `nodes::registry()`; forms and on-canvas controls come from declarative `ParamDef`/`Overlay`. |
| `src/graph.rs` | DAG model, size inference, structural hashing, layer-stack chains. |
| `src/expr.rs` | Delos expressions: the plain-text graph format. |
| `src/project.rs` | Project folders, input copies, atomic saves, `.delosproject` archives. |
| `src/formats.rs` | `Codec` registry (PNG, JPEG, WebP, GIF, BMP, TIFF, ICO) and banded export. PNG streams. |
| `src/worker.rs` | Background preview, import and export (latest-request-wins previews, progressive refinement). |
| `src/ui/` | Dioxus UI: image view, graph view, inspector, dialogs. |

Key decisions:

- **Pull-based, resolution-independent rendering.** Nothing is ever rendered at full resolution except during export.
  Previews request only the visible region at screen resolution.
- **Generators are size-less.** They render at whatever extent is requested. Shown on their own, they use the output canvas,
  which defaults to the window size and can be resized by dragging its corner.
- **Minimal caching.** Only expensive results are cached (the background-removal mask, in a 256 MiB LRU).
  Everything else is regenerated on demand.
- **Editing upstream nodes in place.** With the output relocated (or "Preview selection" off), selecting a node keeps the view
  and draws that node's controls over it. `NodeKind::input_map` describes how each node moves coordinates
  (Clip, Resize, Downscale), and `Graph::space_map` composes these maps down the graph.
- **Stack chains** `(a STACK (b STACK c))` are shown as a single "Layer Stack" with per-layer enable/reorder/remove.
  Disabling a layer inserts a `Disable` node.

## Adding things

- **Node**: implement `NodeKind` (usually a `Kernel` in `kernels.rs` plus a struct in `src/nodes/`) and add it to that module's `kinds()`.
- **Format**: add a `Codec` entry in `formats.rs`.

## Known gaps / next steps

- The web build runs on the CPU with one thread. WebGPU needs async readback, and wasm threads need a COOP/COEP setup.
- Undo history keeps removed images in memory (bounded to 60 steps) and is lost when the project is closed.
- Projects on web and mobile (see Platform status above).
- No AI image generator node yet; the cached-source pattern used by background removal is the intended hook.
- AVIF is not supported (it needs native dav1d/rav1e).
- The mobile build compiles, but it has not been run on a device.
