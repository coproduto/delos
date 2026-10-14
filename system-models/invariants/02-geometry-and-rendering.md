# 2 · Geometry and rendering

Delos renders by **pulling**. A consumer asks a node for a `Request`, which
means "this region of your space, sampled on a W×H grid". The node turns it
into requests for its inputs, renders them, and combines the results with a
kernel. Nothing is rendered at a fixed resolution except during export. These
invariants are what make a preview at any zoom and a banded 1:1 export show the
same picture.

Core types: `Request`, `Map2` and `Size` ([src/engine/geom.rs](../../src/engine/geom.rs)),
`Tile` and `CpuTile` ([src/engine/tile.rs](../../src/engine/tile.rs)), and
`Ctx` ([src/engine/mod.rs](../../src/engine/mod.rs)).

---

## R1 · Every node has its own continuous pixel space; pixel *i* is centred at *i* + 0.5 {#r1}

**Statement.** Each node's space is measured in its own native pixels, from
`(0, 0)` to its extent. A request's grid maps output pixel centres to node
space through `pixel_to_node` ([geom.rs:180-182](../../src/engine/geom.rs#L180)).
Every sampler uses the same convention for texel centres: CPU `sample`
([tile.rs:59-106](../../src/engine/tile.rs#L59)), the WGSL `sN`
([kernels.rs:151-181](../../src/engine/kernels.rs#L151)), and pyramid taps
([source.rs:198-211](../../src/engine/source.rs#L198)).

**Depended on by.** Every spatial calculation: `texel_map_to`
([geom.rs:212-215](../../src/engine/geom.rs#L212)), split pixel-centre decisions
([R7](#r7)), Reflect's mirror, Resize's native cover and the pyramid. A
half-pixel disagreement between two of them would shift images or cause seams
between export bands.

**Status.** Holds. It is a convention shared by all code that does geometry,
and the parity tests check it indirectly.

---

## R2 · All coordinate maps are separable affine maps (`Map2`) {#r2}

**Statement.** Node to input, output pixel to node, and output pixel to texel
are all `x' = a·x + b`, `y' = c·y + d`: scale, translation and mirroring, with
no rotation or shear ([geom.rs:95-131](../../src/engine/geom.rs#L95)).
`NodeKind::input_map` must return such a map ([nodes/mod.rs:233-235](../../src/nodes/mod.rs#L233)).

**Depended on by**
- `Request::mapped` transforms a whole rectangle with `apply_rect` and keeps the grid ([geom.rs:190-192](../../src/engine/geom.rs#L190)). This is only correct for axis-aligned maps.
- `Graph::space_map` composes the maps down the graph to place an upstream node's controls over a downstream image ([graph.rs:313-343](../../src/graph.rs#L313)).
- Overlay hit-testing inverts the composed map ([image_view.rs:162-165](../../src/ui/image_view.rs#L162)).
- GPU uniforms send each map as four floats ([gpu.rs:62-69](../../src/engine/gpu.rs#L62)).

**If broken.** Adding a node that rotates (or shears or warps) is not "one more
kind". It needs a new request model and a new overlay-mapping model. This is the
main structural limit on the claim that "the architecture assumes more nodes".

**Status.** Holds. It is enforced by the type system: `Map2` cannot express anything else.

---

## R3 · `render` returns a tile of exactly the requested grid {#r3}

**Statement.** `ctx.render(link, req)` returns a `Tile` whose `dims()` are
`(req.width, req.height)`. A degenerate request (zero size or an empty region)
returns `Tile::Clear` of those dimensions ([engine/mod.rs:205-207](../../src/engine/mod.rs#L205)).

**Maintained by.** Each kind either forwards an input's tile rendered on the
same grid (Clip, Stack shortcuts, Split shortcuts, Disable) or calls
`ctx.kernel(..., req, ...)`, which always produces `req.width × req.height`
([engine/mod.rs:306](../../src/engine/mod.rs#L306), [engine/mod.rs:323](../../src/engine/mod.rs#L323)).
Sampling a source image produces the request's grid as well ([source.rs:150](../../src/engine/source.rs#L150), [engine/mod.rs:269](../../src/engine/mod.rs#L269)).

**Depended on by.** `KernelInput::aligned` declares an input to be on the
output grid: identity map, nearest sampling ([engine/mod.rs:160-162](../../src/engine/mod.rs#L160)).
The CPU fast path uses such an input's rows directly ([kernels.rs:65-69](../../src/engine/kernels.rs#L65)).

**If broken.** On the CPU, `aligned_row` falls back to resampling when widths
differ, so a wrong size gives a shifted image instead of a crash. On the GPU,
reads outside the tile return transparent pixels. Either way the error is
silent. It would show up only in pixel tests such as `export_matches_sequential_render`.

**Status.** Holds. It is a convention every `NodeKind::render` must follow and
is not checked at runtime.

---

## R4 · Size-less nodes render at whatever extent they are asked for {#r4}

**Statement.** `NodeKind::size` returning `None` means the node has no size of
its own. `Request::extent` carries the logical size the consumer wants.
Generators normalise their coordinates by `extent` (for example
`node(p) / extent()` in [kernels.rs:751](../../src/engine/kernels.rs#L751)), so
the same parameters describe the same picture at any extent and any resolution.
A node with a definite size passes *its input's* size as the input's extent
(`input_request`, [basic.rs:16-18](../../src/nodes/basic.rs#L16)). When a
size-less node is the thing shown or exported, its extent is the output canvas
(`extent_of`, [graph.rs:306-308](../../src/graph.rs#L306)).

**Depended on by**
- The spec's rule that generators adapt to whatever they are combined with.
- Fraction parameters (`ParamKind::Fraction`), which store positions as 0..1 of the extent so they stay valid at any size ([nodes/mod.rs:56-58](../../src/nodes/mod.rs#L56)).
- Canvas auto-sizing (`fit_canvas_to_window`, [state.rs:323-344](../../src/ui/state.rs#L323)).

**Subtlety.** `input_request` keeps `req.region` and changes only `extent`. It
assumes the input is **top-left aligned on the consumer's grid**, which is
exactly what combining nodes do ([R6](#r6)).

**Status.** Holds.

---

## R5 · Definite sizes are whole pixels {#r5}

**Statement.** Every `Some(size)` from `NodeKind::size` has integer `w` and `h`.

**Maintained by.** Each implementation rounds its own result. Image dimensions
and the canvas are integer parameters. Clip rounds
([basic.rs:335](../../src/nodes/basic.rs#L335)), Resize rounds or uses integer
parameters ([resize.rs:64-72](../../src/nodes/resize.rs#L64)), Downscale floors
([resize.rs:236](../../src/nodes/resize.rs#L236)), and the others pass a size
through or take a union of sizes. Integer parameters stay integers because of
[G7](01-document-model.md#g7).

**Depended on by**
- **Export.** `GraphBands` renders the region `(0, y, width, rows)` at `width × rows` with `width = extent.pixels().0` ([formats.rs:230-251](../../src/formats.rs#L230)). The request step is exactly 1 only when the extent is whole, which is what makes an export a true 1:1 render.
- Exact split partitions ([R7](#r7)).
- `size_conflict`'s comparison with a 0.5-pixel tolerance ([graph.rs:357](../../src/graph.rs#L357)).

**Status.** Holds. Every `size()` implementation follows the convention, and it
is not checked anywhere. A new kind that returns a fractional size would make
exports slightly resampled without any error.

---

## R6 · Combining nodes align inputs top-left; mismatches go to the user {#r6}

**Statement.** Stack takes the union of its inputs' sizes, and Blend takes the
base's size, falling back to the layer's ([basic.rs:22-24](../../src/nodes/basic.rs#L22), [basic.rs:219-221](../../src/nodes/basic.rs#L219)).
Inputs are placed at the origin, with no scaling. When two definite input sizes
differ, the user decides. `after_edit` finds the first conflict
([state.rs:304-313](../../src/ui/state.rs#L304)), and the dialog either inserts a
Resize node ([graph.rs:367-375](../../src/graph.rs#L367),
[dialogs.rs:44](../../src/ui/dialogs.rs#L44)) or records an "ignore" keyed by
`(node, size a, size b)` ([state.rs:545-548](../../src/ui/state.rs#L545)).

**Why it is shaped this way.** The spec has Delos *ask* which size wins. The
engine itself always renders something: a mismatch is a question for the user,
never an error.

**Status.** Holds.

---

## R7 · Splits partition pixels exactly, at any scale {#r7}

**Statement.** For a split node and any request, every output pixel goes to
exactly one of the two outputs. As a result
`stack(split.out0, split.out1) == input`, pixel for pixel, at any zoom.

**Maintained by** ([nodes/split.rs](../../src/nodes/split.rs)):
1. Each region is computed from the same `(node, req.extent)` for both outputs, with borders rounded to whole node units ([split.rs:40](../../src/nodes/split.rs#L40), [split.rs:50](../../src/nodes/split.rs#L50), [split.rs:67-72](../../src/nodes/split.rs#L67)).
2. Each pixel is assigned by its **centre**, with half-open intervals `[x0, x1)`, both in the shortcut test ([split.rs:117](../../src/nodes/split.rs#L117)) and in the kernel ([kernels.rs:276](../../src/engine/kernels.rs#L276), [kernels.rs:284](../../src/engine/kernels.rs#L284)).
3. A tile entirely on one side skips the kernel. This uses the same predicate on the first and last pixel centres, so it agrees with the per-pixel path ([split.rs:114-127](../../src/nodes/split.rs#L114)).
4. Pixels that are not kept are transparent `(0, 0, 0, 0)`. Under "over" compositing, two pixels with disjoint coverage simply add.

**Depended on by.** The whole "selections" story: localised effects are built
as split, then effect, then stack. Also [I3](04-structural-identity.md#i3): the
two outputs must hash differently.

**Tests.** All of `tests/split.rs`: partitions, recombination at any scale,
borders at the edges, size-less inputs, and GPU = CPU.

**Status.** Holds. It is one of the most thoroughly tested invariants.

---

## R8 · Work is proportional to the requested pixels, with explicit exceptions {#r8}

**Statement.** A node fetches only the region its output needs, at about the
output's resolution, so a preview's cost does not depend on how large the
sources are. Sources get this from the mip pyramid ([R10](#r10)).

**Documented exceptions**
- **Background removal** renders its whole input at a bounded resolution (512 to 2048 pixels) once, then caches the mask ([bgremove.rs:72-87](../../src/nodes/bgremove.rs#L72)).
- **Resize upsampling** fetches the native pixels under the region plus a 3-pixel margin. When that is more than 4× the output pixels, it falls back to direct sampling ([resize.rs:101-106](../../src/nodes/resize.rs#L101)).
- **Fourier resize** transforms the whole native cover on the CPU ([resize.rs:109-118](../../src/nodes/resize.rs#L109)).
- **Reflect** renders its input twice, once direct and once mirrored, and only when the destination interval meets the region ([basic.rs:283-292](../../src/nodes/basic.rs#L283)).
- Upstream nodes are evaluated once per consumer request. Nothing is shared between two consumers of the same node in one render ([FUTURE_INVESTIGATIONS.md](../../FUTURE_INVESTIGATIONS.md) §5).

**Depended on by.** The performance requirement (interactive previews at 16K)
and the memory requirement (no full-resolution intermediates).

**Tests.** `tests/large_images.rs` (timings and bounded export memory).

**Status.** Holds as a design rule. New kinds must follow it, and only the
16K tests would catch one that does not.

---

## R9 · Tiles hold premultiplied RGBA with 0 ≤ c ≤ a ≤ 1 {#r9}

**Statement.** Pixels in tiles are premultiplied, sRGB-encoded `f32` values
([tile.rs:3-5](../../src/engine/tile.rs#L3)). Source images are premultiplied
once at import ([source.rs:45-54](../../src/engine/source.rs#L45)).

**Maintained by.** Each kernel stays within range or clamps with
`clamp_premul`: Blend, bicubic sampling, additive Reflect and Fourier output
([tile.rs:123-126](../../src/engine/tile.rs#L123), and the matching WGSL).
Invert produces `a − c`, which is in range whenever `c ≤ a`.

**Gap.** Generator colours come from `Color` parameters, which the expression
format does not range-check ([G7](01-document-model.md#g7)). A hand-written
`(solid :color (2 0 0 1))` produces out-of-range premultiplied values. Display
and export clamp them when converting to 8-bit ([worker.rs:478](../../src/worker.rs#L478),
[formats.rs:203-205](../../src/formats.rs#L203)), but blend modes in between see
values outside their domain.

**Status.** Holds for every value the UI can produce. Values from hand-written
files can violate it, and the 8-bit conversions mask the effect.

---

## R10 · The source pyramid and level choice are the same on both backends {#r10}

**Statement.** Level *k* + 1 is the 2×2 box average of level *k*, rounded half
up, with dimensions `max(1, ⌊n/2⌋)` ([source.rs:214-235](../../src/engine/source.rs#L214)).
`level_for` picks the finest level whose texels are no smaller than the
request's pixels. Point sampling always uses level 0
([source.rs:114-121](../../src/engine/source.rs#L114)). Level 0 is sampled
nearest and higher levels bilinearly. The GPU path uses the same `level_for`
and the same `texel_map` ([engine/mod.rs:264-267](../../src/engine/mod.rs#L264),
[source.rs:126-133](../../src/engine/source.rs#L126)).

**Depended on by.** Agreement between CPU and GPU previews ([B1](03-backends-and-resources.md#b1)),
and Resize's "nearest" filter giving exact pixels.

**Known gap.** The GPU maps coordinates in `f32` and the CPU in `f64`, so at 16K
nearest sampling can pick the neighbouring texel near a boundary
([FUTURE_INVESTIGATIONS.md](../../FUTURE_INVESTIGATIONS.md) §2). If pyramid
generation ever moves to the GPU, it must match `downsample` exactly.

**Tests.** `tests/resident.rs::gpu_sampling_matches_cpu` and `tests/engine.rs::native_render_reproduces_source`.

**Status.** Holds, within a documented f32 tolerance.

---

## R11 · Rendering is deterministic and stateless {#r11}

**Statement.** The same graph and the same request give the same pixels.
Generators hash integer lattice coordinates (`hash_u` and `hash2`, written
identically in Rust and WGSL, [kernels.rs:654-705](../../src/engine/kernels.rs#L654)).
Dissolve hashes floor-of-node coordinates, so its pattern stays fixed in node
space under pan and zoom ([kernels.rs:629-632](../../src/engine/kernels.rs#L629)).

**Depended on by.** The choice to **not cache** ordinary results and to
regenerate instead ([cache.rs:1-4](../../src/engine/cache.rs#L1)). Export bands
that join without seams. Progressive refinement, where a quarter-resolution
preview is followed by the full one. Save and load "rendering the same pixels".

**Status.** Holds. Background removal is the one exception, since a neural
network is not guaranteed to be deterministic, and it is cached for exactly
that reason.

---

## R12 · Errors name the deepest failing node; cancellation passes through {#r12}

**Statement.** An error raised while rendering node *n* is wrapped as
`NodeError { node: n }` only if it is not already a `NodeError` or a `Cancelled`
([engine/mod.rs:208-214](../../src/engine/mod.rs#L208)). The innermost failing
node therefore keeps the blame, and cancellation reaches the worker unchanged,
where it is recognised with `is_cancelled` ([worker.rs:218](../../src/worker.rs#L218)).

**Maintained by.** Every `render` and every `kernel` call checks for
cancellation first ([engine/mod.rs:201](../../src/engine/mod.rs#L201),
[engine/mod.rs:275](../../src/engine/mod.rs#L275)), so a superseded preview
stops at the next node or kernel boundary.

**Depended on by.** The UI highlights the failing node
([ui/mod.rs:165-170](../../src/ui/mod.rs#L165)), and cancelled previews never
show up as errors.

**Status.** Holds.
