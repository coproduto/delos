# Findings

While cataloguing the invariants, I checked each one against the code that is
supposed to maintain it. Two are **broken**, and both are confirmed by tests in
[`probes/invariant_probes.rs`](probes/invariant_probes.rs), which fail on commit
`197e30c`. A few more are **fragile**: they hold today only because of a
convention, or they hold in a narrower sense than their documentation claims.

Nothing in `src/` was changed.

---

## Finding 1 · Structural hash collision: image ids are reused after undo or a new document {#finding-1}

**Invariant.** [I1](04-structural-identity.md#i1): equal structural hashes mean
equal images.

**Mechanism.** A static image node is hashed through its `image` parameter, a
graph-local `ImageId` that comes from `Graph::next_id`. That counter is part of
each snapshot:
- undo restores an older counter ([state.rs:262-265](../../src/ui/state.rs#L262));
- `Graph::new()` restarts it at 1 ([graph.rs:104](../../src/graph.rs#L104)), on Close, End quick session and project load.

The next import then gets an id that a different image held before. If its
dimensions are the same, the graph's hash is identical.

**Confirmed.** Probe `structural_hash_distinguishes_images_after_undo`: a red
2×2 image and, after an undo, a blue 2×2 image give the same output hash.

**Consequences, derived from the code.** The hash itself is confirmed. These
UI-level effects were not run through a window.
1. **Stale preview.** Import photo A, undo, then import photo B of the same size. The preview key ([image_view.rs:107-120](../../src/ui/image_view.rs#L107)) equals the last key sent, so no render is requested. The old frame of A still passes the extent filter ([image_view.rs:179](../../src/ui/image_view.rs#L179)), so **the view shows A while the document contains B**. The same happens after End quick session or Close project followed by importing an image of the same size, because `last_key` lives in a hook that survives document replacement. Any later edit that changes the hash corrects the view.
2. **Wrong background-removal mask.** The mask cache key builds on the same hash ([bgremove.rs:67](../../src/nodes/bgremove.rs#L67)), and the cache lives as long as the `Worker`. With the same sequence of steps, plus a Background Removal node with the same settings, B is masked with A's cutout. **This one is not corrected by later edits.** It lasts until the entry is evicted or a parameter changes.

**Why it went unnoticed.** Inside one graph the premise holds
([G5](01-document-model.md#g5)), and every hash test builds a single graph. The
failure needs a hash to be compared across two graph lineages, and the only
places that do that are long-lived UI and engine state.

**Repair options** (not implemented):
- Hash the image's process-unique `SourceImage::id` ([source.rs:17](../../src/engine/source.rs#L17)) instead of the `ImageId` parameter. This is the smallest change, and GPU residency already uses this identity.
- Or make `ImageId`s monotonic across snapshots, for example with a global counter.

Turning the probe into a regression test would cover either option.

---

## Finding 2 · "Add node" after a split rewires the second output's consumers onto the first {#finding-2}

**Invariant.** [U6](05-concurrency-and-ui.md#u6): inserting after the selected
node keeps each consumer on the data it was reading.

**Mechanism.** `App::add_node` reconnects *every* consumer of the anchor
(`consumers(a)` covers all outputs) to the new node, then feeds the new node the
anchor's **output 0** ([state.rs:470-476](../../src/ui/state.rs#L470)).

**Confirmed.** Probe `add_node_after_a_split_keeps_each_consumer_on_its_output`:
with `noise → split_vertical`, `left → invertA` and `right → invertB`, selecting
the split and adding Invert leaves `invertB` reading `invert(split.left)`.

**User-visible effect.** Every consumer of the split now shows the left part.
The localised-effect pattern (split, then effect, then stack) quietly breaks,
because both stack layers become the same half. Undo restores it.

**Why it went unnoticed.** The function was written when every node had one
output. The README calls out multi-output nodes as a later addition, and
`tests/ui_state.rs` covers deleting around splits but not inserting after them.

**Repair options.** Insert only for consumers of one chosen output: output 0,
the relocated output, or a choice made in the UI. Alternatively, refuse to
insert after nodes with several outputs.

---

## Fragile spots

These hold today, but only because of conventions that nothing checks. They are
listed roughly by how easily a future change could break them.

| # | Invariant | What could break it | Detected by |
|---|---|---|---|
| 1 | [B1](03-backends-and-resources.md#b1): GPU = CPU | Changing one half of a kernel and not the other | Parity tests, if the kernel and that parameter range are covered |
| 2 | [G2](01-document-model.md#g2): acyclic graph | New code that writes `node.inputs[...]` directly, as `set_stack_layers`, `insert_resize` and the expression builder already do | Nothing at runtime: the result is a stack overflow |
| 3 | [R3](02-geometry-and-rendering.md#r3) and [R5](02-geometry-and-rendering.md#r5): exact tile size, whole-pixel sizes | A new `NodeKind` that returns a different grid or a fractional size | Pixel tests only; otherwise silent resampling |
| 4 | [B4](03-backends-and-resources.md#b4): every kernel writes every texel | A WGSL kernel with its own `main` that exits early | `gpu_pool` tests, intermittently |
| 5 | [U4](05-concurrency-and-ui.md#u4): signal borrows end before writes | Any UI handler that holds a `read()` across an edit | `ui_state` regression tests, after the fact |
| 6 | `MAX_INPUTS` and `MAX_PARAMS` | A kernel with 4 inputs: panics on the CPU, is **silently truncated** on the GPU ([gpu.rs:349](../../src/engine/gpu.rs#L349)) | Only the CPU panic |
| 7 | [W4](05-concurrency-and-ui.md#w4): bounded export memory | Exporting 16K as JPEG, WebP, TIFF and so on, which buffers the whole RGBA8 image | Not tested; only PNG is |
| 8 | [J2](06-persistence.md#j2): atomic save | Power loss (there is no `fsync`) | Not tested |
| 9 | [J6](06-persistence.md#j6): undo can restore images | A stash pruned by a second instance during a session longer than 24 hours | Falls back to re-encoding as PNG: the pixels survive, the bytes do not |
| 10 | [W3](05-concurrency-and-ui.md#w3): save status | Out-of-order completion shows "All changes saved" early | Cosmetic |
| 11 | [I2](04-structural-identity.md#i2): complete cache keys | A model file replaced during a session while the `model` parameter is empty | Not detected |
