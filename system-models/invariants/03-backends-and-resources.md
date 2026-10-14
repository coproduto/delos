# 3 · Backends and GPU resources

Delos has two interchangeable backends. Each `Kernel` is written twice, in WGSL
for the GPU (wgpu) and in Rust for the CPU (rayon). Above them, the engine keeps
tiles on the GPU between kernels, reuses textures, and keeps source images
resident on the GPU. These invariants let the GPU be an optimisation that can be
switched off, fail or run out of memory without changing what the user sees.

Files: [kernels.rs](../../src/engine/kernels.rs), [gpu.rs](../../src/engine/gpu.rs),
[pool.rs](../../src/engine/pool.rs), [resident.rs](../../src/engine/resident.rs),
[engine/mod.rs](../../src/engine/mod.rs).

---

## B1 · Each kernel computes the same function on GPU and CPU {#b1}

**Statement.** For every `Kernel` in `kernels::ALL`, the WGSL `kernel(p)` and the
Rust `cpu` function give the same output for the same inputs, parameters and
grid, within floating-point tolerance.

**Shared contract** (header of [kernels.rs:1-10](../../src/engine/kernels.rs#L1)):
- `p` is the pixel centre, and `node(p)` and `extent` mean the same on both sides.
- Parameters sit at the same indices, read with `P(i)` or `env.p[i]`. Each kernel documents its layout in a doc comment, such as "P0..3 color A, P4..7 color B…".
- At most `MAX_INPUTS = 3` inputs and `MAX_PARAMS = 32` floats ([kernels.rs:15-16](../../src/engine/kernels.rs#L15)).
- Samples outside a tile are transparent, and filter taps are clamped to the edge. Input mode `3` means "no input" and reads as transparent ([kernels.rs:153](../../src/engine/kernels.rs#L153), [gpu.rs:351](../../src/engine/gpu.rs#L351)).
- Integer hashing is bit-identical on both sides ([kernels.rs:654-705](../../src/engine/kernels.rs#L654)).

**Maintained by.** Convention and tests. Nothing in the type system ties the two
bodies together. They are two string and function literals side by side in one
constant.

**Tests.** `tests/engine.rs::gpu_matches_cpu`, `tests/split.rs::gpu_split_matches_cpu`,
`tests/resident.rs::gpu_sampling_matches_cpu` and `tests/kernel_profile.rs`
(all kernels on both backends).

**If broken.** The picture changes when the user clicks the backend badge, or
when the engine falls back to the CPU because of a GPU error ([B3](#b3)) or a
large tile ([B2](#b2)). Within one render a graph can mix backends, so a
mismatch can also show as seams.

**Limits.** Going over `MAX_PARAMS` panics in `copy_from_slice`
([engine/mod.rs:279](../../src/engine/mod.rs#L279)). Going over `MAX_INPUTS`
panics on the CPU, where `s[n]` is out of bounds
([kernels.rs:1003](../../src/engine/kernels.rs#L1003)), but is **silently
truncated** on the GPU (`take(MAX_INPUTS)`, [gpu.rs:349](../../src/engine/gpu.rs#L349)).
`Kernel::inputs` is declared but never checked.

**Status.** Holds within tolerance. It is the most important invariant that is
"maintained by tests only".

---

## B2 · Backend choice is per kernel call and never changes the result's meaning {#b2}

**Statement.** `Ctx::kernel` runs a kernel on the GPU when the GPU is active and
healthy, the output and every input fit in a texture, and the pipeline compiled.
Otherwise it runs on the CPU ([engine/mod.rs:283-323](../../src/engine/mod.rs#L283)).
Inputs are converted as needed: `Clear` becomes an absent input, `Cpu` is
uploaded, `Gpu` is used in place, and `to_cpu` downloads.

**Sub-invariants**
- A `Tile::Gpu` exists only when `engine.gpu` is `Some`. `to_cpu` depends on this (`expect("gpu tile without gpu")`, [engine/mod.rs:239](../../src/engine/mod.rs#L239)).
- Every `GpuTile` the engine returns is `Rgba32Float`. Resident source levels are `Rgba8Unorm` and are only ever *inputs* to the resample kernel, never returned ([engine/mod.rs:254-271](../../src/engine/mod.rs#L254)).
- Tiles stay on the GPU between consecutive GPU kernels. They are read back only by `to_cpu`: for CPU-only work such as Fourier resizing or mask computation, and at the end of a render (`render_to_cpu`).

**Depended on by.** [B1](#b1) gives each kernel the same meaning on both
backends. This invariant lets them mix freely within one render, so the CPU
fallback is complete.

**Status.** Holds.

---

## B3 · GPU failure degrades to the CPU and is never fatal {#b3}

**Statement**
- No adapter, a software adapter, `DELOS_NO_GPU`, or a panic during setup all mean `gpu = None`, and every render uses the CPU ([gpu.rs:93-119](../../src/engine/gpu.rs#L93)).
- An uncaptured device error sets `failed` ([gpu.rs:134-137](../../src/engine/gpu.rs#L134)). `active_gpu()` then returns `None` ([engine/mod.rs:142-147](../../src/engine/mod.rs#L142)), and `download` fails the render that is in progress ([gpu.rs:322-324](../../src/engine/gpu.rs#L322)). **The flag is never reset**, so after one device error the session uses the CPU for good.
- A kernel whose shader fails to compile is cached as `None` and always runs on the CPU ([gpu.rs:241-249](../../src/engine/gpu.rs#L241)).
- Upload errors for resident levels, such as running out of memory, are caught inside error scopes. They mark the level `Skipped` and do not count as device failures ([resident.rs:234-275](../../src/engine/resident.rs#L234)).

**Depended on by.** The spec requirement to degrade gracefully, and the
platform matrix: web has no GPU path (WebGPU would need async readback).

**Status.** Holds.

---

## B4 · Pooled textures may hold stale data because every kernel writes every texel {#b4}

**Statement.** Textures and readback buffers go back to a `Pool` when their last
`GpuTile` is dropped ([gpu.rs:34-40](../../src/engine/gpu.rs#L34)) and are handed
out again for a request of the same size ([pool.rs:64-73](../../src/engine/pool.rs#L64)).
They are not cleared. This is safe because:
1. `WGSL_MAIN` writes every texel inside `out_size`, and the dispatch covers `⌈w/8⌉ × ⌈h/8⌉` workgroups ([kernels.rs:184-191](../../src/engine/kernels.rs#L184), [gpu.rs:386](../../src/engine/gpu.rs#L386)).
2. `out_size` always equals the texture's size, because `tile(w, h)` is requested with the dispatch's own `w` and `h` ([gpu.rs:360](../../src/engine/gpu.rs#L360)).
3. `upload` writes the whole texture ([gpu.rs:261-266](../../src/engine/gpu.rs#L261)).
4. The queue runs submissions in order, so a texture returned while earlier work still reads it is overwritten only after that work ([pool.rs:11-12](../../src/engine/pool.rs#L11)).

**Further invariants.** Idle bytes never exceed the cap; anything larger than
the cap is dropped instead of pooled ([pool.rs:110-128](../../src/engine/pool.rs#L110)).
Resident levels never enter the pool (`pool: None`, [gpu.rs:48-52](../../src/engine/gpu.rs#L48)).
A pooled resident level would have the wrong format for its size key.

**If broken.** A kernel that writes only some texels, for example one that
returns early for transparent regions, would let pixels from an earlier frame
show through. This would happen only on the GPU and only intermittently.

**Tests.** `tests/gpu_pool.rs::reused_textures_do_not_leak_stale_pixels`,
`idle_memory_stays_within_the_cap` and `repeated_renders_reuse_textures`.

**Status.** Holds. Every new WGSL kernel must keep it, which is easy because
`WGSL_MAIN` is shared.

---

## B5 · Source residency is best effort, within budget, and never blocks a render {#b5}

**Statement** ([resident.rs](../../src/engine/resident.rs)):
1. **Never blocks.** `Resident::get` returns a level only if it is already resident. Otherwise it queues the upload and returns `None`, and the render samples on the CPU ([resident.rs:130-142](../../src/engine/resident.rs#L130), [engine/mod.rs:265](../../src/engine/mod.rs#L265)).
2. **Budget.** `bytes` is the sum of the `Resident` slots and stays ≤ `budget`. Space is made before an upload by evicting the least recently used levels **of other images** ([resident.rs:76-91](../../src/engine/resident.rs#L76), [resident.rs:203-208](../../src/engine/resident.rs#L203)). Levels larger than a texture or than the whole budget are `Skipped` ([resident.rs:178-179](../../src/engine/resident.rs#L178)).
3. **Lifetime.** A slot holds a `Weak<SourceImage>`. When no graph and no undo snapshot holds the image, its levels are freed the next time the registry is touched (`drop_dead`, [resident.rs:62-72](../../src/engine/resident.rs#L62)).
4. **Identity.** Slots are keyed by `SourceImage::id`, which is unique for the life of the process ([source.rs:16-17](../../src/engine/source.rs#L16), [source.rs:63](../../src/engine/source.rs#L63)). Compare this with graph-local image ids in [I1](04-structural-identity.md#i1).
5. **Uploads that race with re-budgeting** are discarded if the slot is no longer `Queued` when the upload finishes ([resident.rs:213-216](../../src/engine/resident.rs#L213)). An evicted texture stays alive until the renders using it finish, through `Arc` clones.

**Depended on by.** GPU preview and export speed. Correctness never depends on
it, because the CPU path is always available.

**Known weakness.** Eviction thrashing when the working set exceeds the budget
([FUTURE_INVESTIGATIONS.md](../../FUTURE_INVESTIGATIONS.md) §2). Point 2 is
`budget` minus a level that could not be placed, not a hard per-frame guarantee.

**Tests.** `tests/resident.rs`: every level uploaded, LRU eviction within
budget, dropped images released, oversize levels skipped, rendering queues
missing levels.

**Status.** Holds.

---

## B6 · The expensive-result cache is a byte-bounded LRU that keeps its newest entry {#b6}

**Statement.** `Cache::insert` evicts the least recently used entries until the
total fits the budget, but never the entry it just inserted, even when that one
alone exceeds the budget ([cache.rs:31-44](../../src/engine/cache.rs#L31)). By
design the cache holds only results that are expensive to recompute. Today that
is only background-removal masks ([cache.rs:1-4](../../src/engine/cache.rs#L1)).

**Depended on by.** Background removal running its network once for each input
and parameter set. **The cache is only as correct as its keys.** Keys come from
the structural hash ([I2](04-structural-identity.md#i2)).

**Status.** Holds as a container. The keys it receives can collide; see [finding 1](findings.md#finding-1).
