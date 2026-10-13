# Future performance investigations

Performance improvements that have been identified but deferred. Ordered by
priority: the expected gain weighed against effort and risk. Numbers come from
the `large_images` tests on an M1 Pro (8 performance + 2 efficiency cores),
measured with `samply` and confirmed with direct timing.

Profiling note: samply on macOS repeats a sleeping thread's last captured
stack, so a blocked thread can look busy. Weight samples by CPU time
(`threadCPUDelta`) and confirm surprising findings with direct timing before
acting on them.

## 1. Keep source images resident on the GPU

**Observation.** On the GPU backend, every render samples the source image
pyramid on the CPU (`SourceImage::render`) into a premultiplied `f32` tile and
uploads it (`Gpu::upload`): about 19 MB per 1080² preview frame. Before
sampling was sped up this was ~70% of the GPU path's CPU time during
previews; sampling is now ~2.5× faster, but the uploads remain, and the GPU
backend is still only slightly faster than the CPU one (~19 vs ~20 ms per 16K
preview re-render). In a 16K PNG export the uploads alone cost ~1 s (copies in
`Queue::write_texture`).

**Idea.** Upload each source image's pyramid once, as an RGBA8 texture with
mip levels, and sample it in a kernel.

**Tradeoffs to settle.**
- Memory: a 16K image's pyramid is ~1.33 GB. On unified memory (Apple) this is
  in addition to the CPU copy, which is still needed for the CPU fallback,
  saving projects (`to_rgba8`) and background removal. Needs a residency budget
  and eviction (LRU per image or per level).
- `max_texture_dimension_2d` is 16384 on Apple silicon and lower elsewhere.
  Larger images need splitting across textures; a first version could make only
  images that fit resident and keep the current path for the rest.
- Upload cost moves to import (~1.3 GB once). Upload coarse levels first so the
  fitted preview is available immediately.
- Hardware bilinear filtering has limited fractional precision on some GPUs;
  sampling manually with `textureLoad` keeps parity with the CPU path.
- Mobile needs a much smaller budget (iOS kills memory-heavy apps). The web
  build has no GPU backend.

## 2. Cheaper noise kernel

**Observation.** The fractal Perlin noise kernel is about half of a CPU
preview re-render of the test graph (10.9 ms of ~20 ms at 1080²), though only
graphs that use noise pay for it. It is compute-bound and already scales 7.1×
from 1 to 8 threads, so the remaining lever is doing less work per pixel.

**Idea.** Now that kernels run a row at a time, the integer hashing that
depends only on the row (`hash_u(iy ^ hash_u(seed))` per octave and lattice
row) can be computed once per row and octave instead of per pixel, removing
about two thirds of the hash work. Integer-only, so bit-exact.

## 3. Node output caching

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

## 4. Band-parallel rendering

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

## 5. GPU-side conversion to 8-bit before readback

**Observation.** `Gpu::download` reads back 16 B/px `f32` tiles that are then
converted to 8-bit (`tile_to_rgba8` for export, `encode_display_bmp` for
previews).

**Idea.** A final kernel that unpremultiplies and quantises into an
`rgba8unorm` texture cuts readback 4× for both previews and 8-bit exports.

**Measured.** In a 16K PNG export on the GPU backend, readback costs ~1.85 s
of wall time, mostly waiting for the GPU to finish the band; the row copies in
`Gpu::download` are ~0.4 s and the (now parallel) `tile_to_rgba8` ~0.1 s.
Both would shrink or disappear.

**Tradeoffs.** GPU rounding may differ from the CPU path by one level; a second
implementation of the conversion to maintain; high-bit-depth exports would still
need the `f32` path.

## 6. Faster image import

**Observation.** Importing a 16K image takes ~150–350 ms, dominated by
building the mip pyramid (`downsample`).

**Ideas.** Better parallel granularity in `downsample`; building the pyramid
on the GPU when it becomes resident (see 1); building levels lazily.

## 7. Fusing chains of per-pixel kernels

**Observation.** Each node materialises a full tile, so a chain like
invert → blend → over writes and reads the whole image several times; these
kernels are memory-bound.

**Idea.** Evaluate chains of point-wise kernels in a single pass. Highest
memory-bandwidth savings but a compiler-like project; spatial kernels
(reflect, resample) need halo handling.

## 8. Rendering independent inputs concurrently

**Idea.** Render a node's independent inputs with `rayon::join`. Requires `Ctx`
to become `Sync` (its hash memo is a `RefCell`). Little expected gain: kernels
already use all performance cores, and gaps between nodes are ~3% of a render.

## 9. Parallel PNG compression

**Observation.** PNG encoding is single-threaded: ~2.2 s for a 16K export
(~75% fdeflate compression, ~25% row filtering; disk writes ~60 ms). It
overlaps rendering, which still takes longer (16K PNG export: ~4.6 s CPU
backend, ~3.6 s GPU backend), so it only matters once rendering gets
faster than it.

**Idea.** A multithreaded encoder (e.g. the `mtpng` crate), or compressing
chunks of rows independently and concatenating the deflate streams.
