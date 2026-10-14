# 1 · Product and platform requirements

The spec's opening description and its *Requirements* section. These
clauses are cross-cutting: each is implemented by build configuration and
by a backend split rather than by a single function.

## D1 · Generative image editing {#d1}

> Delos is to be an image editing tool focused on combining existing content with generative content.
> By generative content, we mean both AI-generated content and traditionally generative content, using classical procedural generation algorithms.

**Status.** Partial. Existing content (imports) and procedural content (seven generators) are combined through operators. There is no AI-generated content: the spec itself defers it ([N6](03-operators.md#n6)).

**Implemented by.**
- Existing content: the `StaticImage` node kind ([basic.rs:62-95](../../src/nodes/basic.rs#L62)), created by `finish_import`, which adds the image and its node with `add_image` and `add_node` ([mod.rs:251-263](../../src/ui/mod.rs#L251)).
- Procedural content: the `Generator` node kinds, registered in `kinds` ([generators.rs:218-220](../../src/nodes/generators.rs#L218)).
- Combination: every node kind is listed in the `REGISTRY` ([nodes/mod.rs:251-259](../../src/nodes/mod.rs#L251)); combining kinds report `combines_inputs` ([nodes/mod.rs:236-240](../../src/nodes/mod.rs#L236)).

**Tests.** `tests/engine.rs::invert_and_stack`, `tests/split.rs::localized_effect_on_one_side`.

**Notes.** The project README (Known gaps) names "the cached-source pattern used by background removal" as the intended hook for an AI generator; that pattern is the `Cache` ([cache.rs:1-15](../../src/engine/cache.rs#L1)).

## R1 · Written in Rust {#r1}

> Delos must be built using the Rust programming language.

**Status.** Implemented.

**Implemented by.**
- The crate manifest declares `edition` 2024 ([Cargo.toml:4](../../Cargo.toml#L4)). Everything under `src/` is Rust; WGSL kernels are embedded as string constants such as `WGSL_PRELUDE` ([kernels.rs:105-144](../../src/engine/kernels.rs#L105)).

**Tests.** None (not testable as behaviour).

## R2 · Desktop, web and mobile with Dioxus {#r2}

> Delos must be possible to run as a desktop application, a web-based application and a mobile application.
> We would like to use Dioxus as our framework for this purpose.

**Status.** Partial. One Dioxus UI serves all three targets through Cargo features, but the targets are not equal: the web build has no GPU, no threads, no projects and no background removal; the mobile build has no file pickers and no background removal, and according to the README it has never been run on a device.

**Implemented by.**
- Cargo features `desktop`, `mobile` and `web` pick the Dioxus renderer; only `desktop` enables `rembg` and `rfd` pickers, and `web` enables neither `native` nor `gpu` ([Cargo.toml:8-10](../../Cargo.toml#L8)).
- `launch` starts the desktop window or the generic Dioxus launcher ([mod.rs:26-38](../../src/ui/mod.rs#L26)).
- Background work runs on threads where `native` is enabled and cooperatively (`spawn_local`) on the web ([worker.rs:417-425](../../src/worker.rs#L417)).
- Frames reach the webview through a custom protocol on native targets and through blob URLs on the web: `frame_url` ([platform.rs:53-65](../../src/ui/platform.rs#L53)).

**Tests.** None. Every test runs the native (desktop) configuration.

**Notes.** The platform gaps are listed in [D-PLAT](findings.md#d-plat). Only the project gaps are explicitly accepted by the spec ([X3](04-sessions-and-projects.md#x3)); the GPU, threading and background-removal gaps are not mentioned in it.

## R3 · GPU acceleration wherever possible {#r3}

> Delos will extensively display and manipulate images.
> Using GPU acceleration wherever possible is a requirement for this project.

**Status.** Partial. Every per-pixel kernel has a GPU (wgpu) version, and source images are kept resident on the GPU. Fourier resampling, the background-removal network, banded export encoding and the whole web build run on the CPU only.

**Implemented by.**
- Kernels are written twice, WGSL and Rust, with one parameter layout; see the `Kernel` struct ([kernels.rs:18-25](../../src/engine/kernels.rs#L18)) and the list `ALL` ([kernels.rs:978-980](../../src/engine/kernels.rs#L978)).
- `Ctx::kernel` dispatches to the GPU when `active_gpu` is available and the tiles fit, and keeps results on the GPU as `Tile::Gpu` ([engine/mod.rs:283-309](../../src/engine/mod.rs#L283)).
- Source images are sampled on the GPU from resident pyramid levels by `source_on_gpu` ([engine/mod.rs:257-271](../../src/engine/mod.rs#L257)); the levels are uploaded on a background thread (`upload_loop`) that `Resident::new` spawns ([resident.rs:113-116](../../src/engine/resident.rs#L113)).

**Tests.** `tests/engine.rs::gpu_matches_cpu`, `tests/split.rs::gpu_split_matches_cpu`, `tests/resident.rs::gpu_sampling_matches_cpu`. `tests/kernel_profile.rs::kernels_on_cpu_and_gpu` times both backends but is an ignored benchmark.

**Notes.** The CPU-only steps: `fourier_resample` ([resize.rs:126-183](../../src/nodes/resize.rs#L126)) and the background-removal `compute_mask` ([bgremove.rs:101-139](../../src/nodes/bgremove.rs#L101)), both reached through `to_cpu`. See [D-PLAT](findings.md#d-plat) for the web build.

## R4 · Graceful CPU fallback {#r4}

> However, in case GPU acceleration is not available, Delos should degrade gracefully to CPU rendering.

**Status.** Implemented. The fallback works at four levels: no adapter, a kernel that does not compile, a device error during the session, and a tile larger than the GPU's texture limit.

**Implemented by.**
- `Gpu::new` returns `None` when `DELOS_NO_GPU` is set, when no adapter exists, or when initialisation fails or panics ([gpu.rs:91-109](../../src/engine/gpu.rs#L91)); software adapters are refused in `init` ([gpu.rs:117-119](../../src/engine/gpu.rs#L117)).
- A kernel whose WGSL fails validation is cached as `None` by `pipeline` and runs on the CPU ([gpu.rs:221-250](../../src/engine/gpu.rs#L221)).
- A device error sets `failed`, so `healthy` turns false ([gpu.rs:134-137](../../src/engine/gpu.rs#L134)) and `active_gpu` stops returning the GPU ([engine/mod.rs:146](../../src/engine/mod.rs#L146)).
- Tiles beyond `max_dim` are not sent to the GPU: `fits` ([gpu.rs:217-219](../../src/engine/gpu.rs#L217)), checked in `Ctx::kernel` ([engine/mod.rs:284-289](../../src/engine/mod.rs#L284)).
- The CPU path of `Ctx::kernel` runs `run_cpu` ([engine/mod.rs:311-323](../../src/engine/mod.rs#L311)).

**Tests.** `tests/engine.rs::gpu_matches_cpu`, `tests/gpu_pool.rs::reused_textures_do_not_leak_stale_pixels`, `tests/resident.rs::gpu_sampling_matches_cpu`. The large-image tests run both backends: `tests/large_images.rs::preview_16k_is_interactive`.

**Notes.** The toolbar badge shows `backend_name` and toggles `use_gpu` at run time ([mod.rs:429-443](../../src/ui/mod.rs#L429)); `active_gpu` returns no GPU while `use_gpu` is off ([engine/mod.rs:143-145](../../src/engine/mod.rs#L143)). The spec asks only for the automatic fallback. Graceful means the same pixels: the parity tests compare every kernel on both backends. The fallback *paths* (compile failure, device loss) are not exercised by any test.
