# 4 · Structural identity (the structural hash)

Delos decides whether two images are "the same" with one function,
`Graph::hash_of` and its output variant `link_hash`
([graph.rs:381-416](../../src/graph.rs#L381)). Two features rely on it:

| Consumer | Uses the hash to… | Where |
|---|---|---|
| Image view | skip re-rendering when an edit does not change the shown image (for example moving nodes, or editing a branch that is not shown) | [image_view.rs:107-120](../../src/ui/image_view.rs#L107) |
| Background removal | key the neural-network mask in the engine `Cache` | [bgremove.rs:66-87](../../src/nodes/bgremove.rs#L66) |

It is also the intended hook for future caching: node output caching
([FUTURE_INVESTIGATIONS.md](../../FUTURE_INVESTIGATIONS.md) §5) and an
AI-generator node. That makes it a load-bearing invariant, with only a short
comment to state it.

---

## I1 · Equal structural hashes mean equal images {#i1}

**Statement** ([graph.rs:379-380](../../src/graph.rs#L379)): *"equal hashes
mean equal images, which lets expensive results be cached safely."*

**What is hashed.** Each node contributes its kind id, every `(key, value)`
parameter (floats by bit pattern), and the hash of each input in port order,
with `0` for an empty port. Position, label and node id are deliberately left
out, so moving or renaming a node does not invalidate anything.

**The hidden premise.** A static image node is hashed through its `image`
parameter, which is a **graph-local `ImageId`**. That stands in for the image's
content. For I1 to hold, this has to be true:

> An `ImageId` is never reused for different pixels anywhere a hash is compared.

That is true inside one `Graph` value ([G5](01-document-model.md#g5)). It is
**not true over a session**:

- **Undo** restores a snapshot with its older `next_id`, so the next import gets an id that a different image used before.
- **`Graph::new()`** (Close project, End quick session, loading a project) restarts at 1.

Both hash consumers outlive the graph:
- `last_key` in the image view is a hook value that survives every graph change ([image_view.rs:81](../../src/ui/image_view.rs#L81)).
- The engine `Cache` belongs to the long-lived `Worker` ([worker.rs:117](../../src/worker.rs#L117)).

**Status: VIOLATED.** Confirmed by a probe test. Two graphs that differ only in
which pixels their single image holds produce identical output hashes when the
second import follows an undo. See [finding 1](findings.md#finding-1) for the
reproduction and the visible consequences.

**What would restore it.** Hash something with process-wide identity. Either
the image's `SourceImage::id`, which is already unique per process
([source.rs:16-17](../../src/engine/source.rs#L16)) and already used for GPU
residency ([B5](03-backends-and-resources.md#b5)), or a content hash. The
alternative is to make `ImageId`s monotonic across snapshots. The first option
keeps the rest of the design unchanged.

---

## I2 · Cache keys contain every input to the cached computation {#i2}

**Statement.** The background-removal key is
`hash("rembg", link_hash(input), threshold, binary, sticker, resolution, model path, extent)`
([bgremove.rs:66-70](../../src/nodes/bgremove.rs#L66)). Anything that changes
the mask must be part of the key.

**Holes**
- Everything behind [I1](#i1).
- The `model` key is the *text of the parameter*. An empty string means "search the default locations" ([bgremove.rs:147-166](../../src/nodes/bgremove.rs#L147)). Installing a different model file during a session does not change the key, so the old mask is reused. This is an acceptable staleness, but it is not stated anywhere.
- `invert` is deliberately left out. It is applied when the mask is used, not when it is computed ([bgremove.rs:96](../../src/nodes/bgremove.rs#L96)).

**Status.** Holds apart from the [I1](#i1) hole and the model-file staleness above.

---

## I3 · Different outputs of one node hash differently {#i3}

**Statement.** `link_hash(Link(n, k))` for `k > 0` combines `(hash(n), k)`.
Output 0 keeps the node hash ([graph.rs:408-416](../../src/graph.rs#L408)).

**Depended on by.** The image view showing a split's `left` and then its `right`
output. Without I3 the second request would be deduplicated away.

**Tests.** `tests/split.rs::outputs_are_distinct_links`.

**Status.** Holds.

---

## I4 · The preview key covers everything that affects the shown pixels {#i4}

**Statement.** The image view sends a new `PreviewJob` only when this key
changes ([image_view.rs:107-116](../../src/ui/image_view.rs#L107)):

```
link_hash(target) · target · grid width/height · backend name · extent · region
```

**Depends on.** [I1](#i1) for the graph part. Toggling the backend is a separate
case: the badge handler also writes to `app.graph` to force the effect to run
again ([ui/mod.rs:437-439](../../src/ui/mod.rs#L437)). The key includes the
backend name so that the new render is not skipped.

**If broken.** The view keeps showing a stale frame with no error. Frames are
filtered only by matching extent ([image_view.rs:179](../../src/ui/image_view.rs#L179)),
so an old frame of the same size passes the filter.

**Status.** Inherits the [I1](#i1) violation.
