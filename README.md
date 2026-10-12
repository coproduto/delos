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

## Tests

```sh
cargo test                                                  # correctness + GPU/CPU parity
cargo test --release --test large_images -- --nocapture     # 16K (16384²) timings
```

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
- No project save/load yet. Undo history keeps references to removed images (bounded to 60 steps).
- No AI image generator node yet; the cached-source pattern used by background removal is the intended hook.
- AVIF is not supported (it needs native dav1d/rav1e).
- The mobile build compiles, but it has not been run on a device.
