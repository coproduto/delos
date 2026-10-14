# Future performance investigations

Performance improvements that have been identified but deferred. Ordered by
priority: the expected gain weighed against effort and risk. Numbers come from
the `large_images` tests on an M1 Pro (8 performance + 2 efficiency cores),
measured with `samply` and confirmed with direct timing, and with the
engine profile (`Engine::profile`): `tests/kernel_profile.rs` compares every
kernel on both backends, and `DELOS_PROFILE=1` prints per-operation totals
for the `large_images` tests.

Profiling note: samply on macOS repeats a sleeping thread's last captured
stack, so a blocked thread can look busy. Weight samples by CPU time
(`threadCPUDelta`) and confirm surprising findings with direct timing before
acting on them.

## 1. Reusing GPU textures and readback buffers

**Observation.** Every GPU kernel writes into a newly created texture, and
every readback into a new buffer. A new texture costs far more on the GPU
than the kernel that fills it (on the M1 Pro, wgpu zero-fills it and Metal
maps fresh memory; neither shows in pass timestamps):

| solid kernel + wait | 512² | 1920×1080 | 16384×256 | 4096² |
|---|---:|---:|---:|---:|
| new output texture | 0.67 ms | 2.71 ms | 4.72 ms | 17.9 ms |
| reused texture | 0.37 ms | 0.66 ms | 0.86 ms | 3.0 ms |

In a 16K GPU preview re-render (~10.6 ms), kernels run for ~3.5 ms of GPU
time, yet waiting for the readback takes 6.9 ms. The rest is allocation,
zeroing and the copy to the readback buffer. Export bands are the same:
~12.6 ms of kernels in a 25.6 ms wait per 4 Mpx band.

**Idea.** A pool of textures and readback buffers on `Gpu`, keyed by size
(and format), that `GpuTile`s return to when dropped. Previews and export
bands request the same sizes over and over, so nearly every allocation
would be a reuse.

**Tradeoffs.** Pooled memory stays allocated: it needs a cap (or to count
against the residency budget) and trimming when sizes change. Reused textures
hold stale data, which is fine because every kernel writes every texel it
dispatches, but would hide a kernel that doesn't.

## 2. GPU-side conversion to 8-bit before readback

**Observation.** `Gpu::download` reads back 16 B/px `f32` tiles that are then
converted to 8-bit (`tile_to_rgba8` for export, `encode_display_bmp` for
previews). Earlier profiles blamed most of a GPU preview on readback, but
pass timestamps (see 1) show most of that "waiting for the GPU" is the
kernels themselves plus texture allocation. What remains attributable to
readback is the copy out of the mapped buffer (1.8 ms of a 10.6 ms preview
re-render; ~0.46 s of a 16K export) and part of the wait.

**Idea.** A final kernel that unpremultiplies and quantises into an
`rgba8unorm` texture cuts readback 4× for both previews and 8-bit exports.

**Tradeoffs.** GPU rounding may differ from the CPU path by one level; a second
implementation of the conversion to maintain; high-bit-depth exports would still
need the `f32` path.

## 3. Follow-ups to source residency

Source images are resident on the GPU in the least invasive form: each
pyramid level that fits is uploaded at import into an LRU-managed registry on
`Gpu` (`src/engine/resident.rs`), and the image node samples it with the
existing resample kernel. This halved GPU preview re-renders (21.7 -> 10.4 ms)
and cut a 16K GPU export from 3.74 to 2.32 s. These extensions would move more
work to the GPU or save more memory:

- **Fused source sampling.** Bind a resident level directly as an input of
  the first kernel that consumes the image, instead of first resampling it
  into a full-size intermediate tile. Saves a pass and a 16 B/px intermediate
  per frame, but changes the tile/kernel-input model.
- **Pyramid generation on the GPU.** Upload level 0 only and build the other
  levels with a compute kernel. Faster imports (see 7) and a quarter less
  upload traffic; the integer averaging must match `downsample` exactly so
  CPU and GPU pyramids stay identical.
- **Exact CPU/GPU sampling parity.** The GPU maps coordinates in `f32`
  (the CPU in `f64`), so at 16K nearest sampling can occasionally pick the
  neighbouring texel near boundaries. Uploading the CPU-computed column and
  row taps (a few KB per request) would make texel choice and weights match
  exactly.
- **Eviction thrashing.** When the levels used together need more than the
  budget (say, exporting a blend of two 16K images with a small budget), each
  render evicts levels the next one needs, re-uploading them in the
  background over and over. Rendering never waits, but bandwidth is wasted.
  Not evicting levels used in the last few seconds would stop it.
- **Dropping the CPU copy of resident levels.** On unified-memory machines
  the CPU and GPU pyramids double memory use (~2.7 GB for a 16K image).
  Resident levels could be dropped from the CPU and read back when the CPU
  path needs them (CPU backend, saving, background removal).

## 4. Cheaper noise kernel

**Observation.** The fractal Perlin noise kernel is about half of a CPU
preview re-render of the test graph (10.9 ms of ~20 ms at 1080²), though only
graphs that use noise pay for it. It is compute-bound and already scales 7.1×
from 1 to 8 threads, so the remaining lever is doing less work per pixel.

**Idea.** Now that kernels run a row at a time, the integer hashing that
depends only on the row (`hash_u(iy ^ hash_u(seed))` per octave and lattice
row) can be computed once per row and octave instead of per pixel, removing
about two thirds of the hash work. Integer-only, so bit-exact.

## 5. Node output caching

**Observation.** Previews re-render the entire graph every frame. Dragging a
slider on the last node recomputes everything upstream (noise, resize, …)
even though it hasn't changed. Upstream nodes are also evaluated once per
consumer request: Reflect renders its input twice (direct and mirrored). The
engine `Cache` is only used for background-removal masks, by design (see
`src/engine/cache.rs`).

**Idea.** Reuse unchanged upstream results between consecutive renders.
Caching only the frontier (outputs whose structural hash is unchanged but
which feed a node whose hash changed) captures the slider-drag case with one
or two tiles of memory.

**Tradeoffs to settle.**
- Memory: tiles are 16 B/px (18.7 MB at 1080², 33 MB at 1920×1080). Caching
  every node of a modest graph costs hundreds of MB.
- Exports must bypass the cache (bands never repeat).
- Panning/zooming changes the request, so only parameter edits benefit.
- Tiles are passed to kernels by value; cached CPU tiles would need `Arc` or a
  copy.
- Keys must include the full request (region, grid, sampling, `exact`).

## 6. Band-parallel rendering

**Original premise (disproved).** Rendering one node at a time, each a
separate parallel loop, was thought to leave cores idle between nodes.

**Measured.** Per 1080² CPU re-render, time outside kernels and source sampling
was ~0.9 ms of 25.7 ms (3%), before sampling was sped up. Kernels already parallelise: compute-bound ones
scale linearly to the 8 performance cores. Cheap kernels (invert, over,
gradients, ~0.8–1 ms each, ~28% together) are limited by memory traffic, not
threads.

**Remaining upside.** Rendering the whole graph per band, with bands sized to
stay in cache, could cut the memory-bound kernels' cost: perhaps 10–15% of a
re-render. Costs: halo regions for spatial nodes, whole-image nodes (FFT
resize, background removal) must not be split.

**Also tried and dropped.** Zeroing new tiles in parallel instead of on the
calling thread (`CpuTile::new`) made no measurable difference.

## 7. Faster image import

**Observation.** Importing a 16K image takes ~150–350 ms, dominated by
building the mip pyramid (`downsample`).

**Ideas.** Better parallel granularity in `downsample`; building the pyramid
on the GPU (see 3); building levels lazily.

## 8. Fusing chains of per-pixel kernels

**Observation.** Each node materialises a full tile, so a chain like
invert → blend → over writes and reads the whole image several times; these
kernels are memory-bound.

**Idea.** Evaluate chains of point-wise kernels in a single pass. Highest
memory-bandwidth savings but a compiler-like project; spatial kernels
(reflect, resample) need halo handling.

## 9. Rendering independent inputs concurrently

**Idea.** Render a node's independent inputs with `rayon::join`. Requires `Ctx`
to become `Sync` (its hash memo is a `RefCell`). Little expected gain: kernels
already use all performance cores, and gaps between nodes are ~3% of a render.

## 10. Parallel PNG compression

**Observation.** PNG encoding is single-threaded: ~2.2 s for a 16K export
(~75% fdeflate compression, ~25% row filtering; disk writes ~60 ms). It
overlaps rendering, which still takes longer (16K PNG export: ~4.6 s CPU
backend, ~3.6 s GPU backend), so it only matters once rendering gets
faster than it.

**Idea.** A multithreaded encoder (e.g. the `mtpng` crate), or compressing
chunks of rows independently and concatenating the deflate streams.
