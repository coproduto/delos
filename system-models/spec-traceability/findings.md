# Findings: where the code and the spec disagree

Building the matrix means writing, for every spec sentence, what the code
does about it. Where the two disagree, the disagreement goes here. Each
finding names the clauses it affects, says what was observed, and ends
with the decision it asks of a human.

Kinds:

- **Defect.** The code contradicts the spec without meaning to. Two of these were already confirmed by failing probes in the [invariants model](../invariants/findings.md).
- **Gap.** The spec asks for something the code does only in part.
- **Decision.** The code made a choice the spec does not settle, or settles differently. It may well be the right choice, but the spec and the code should agree.
- **Spec issue.** The spec's wording cannot be implemented as written, or contradicts itself.

Consequences marked *derived from the code* were not observed in a
running application. Defects marked *confirmed* have a failing probe test,
here in [probes/](probes/) or in the invariants model's. Run this
model's probes with `spectrace probe system-models/spec-traceability`.

| # | Finding | Kind | Clauses |
|---|---|---|---|
| 1 | [Add node after a split rewires the second output](#d-add-after-split) | Defect | S5 |
| 2 | [Stale image after undo or a new document](#d-stale) | Defect | F2.1, O5 |
| 3 | [A failed project import can leave a folder behind](#d-import-leftover) | Defect | P15 |
| 4 | [Opening a generator-only project can rewrite it](#d-open-writes) | Defect | P6.2 |
| 5 | [The size-conflict dialog hides the next conflict](#d-conflict) | Defect | Z1 |
| 6 | [Export memory is bounded for PNG files only](#d-export-memory) | Gap | F4.2, Q3 |
| 7 | [Quick-mode work can be lost without confirmation](#d-quick-exit) | Gap | P7 |
| 8 | [Platform gaps beyond the accepted ones](#d-plat) | Gap | R2, R3, O5 |
| 9 | [Downscaling filters are not what they say](#d-resize) | Gap | O8, O4 |
| 10 | [Overlay mapping: one path, affine maps only](#d-overlay) | Gap | F3.3.1 |
| 11 | [Project errors without a column](#d-error-column) | Gap | E11, P14 |
| 12 | [Only the layer stack has a display modality](#d-modalities) | Gap | F5 |
| 13 | [Default export while editing on the input](#d-export-target) | Decision | F4 |
| 14 | [Importing replaces what feeds the output](#d-import) | Decision | F1.1 |
| 15 | [Derived image parameters are not written](#d-derived-params) | Decision | E8 |
| 16 | [Spec wording that cannot hold as written](#d-spec-wording) | Spec issue | N1, F5, O4, P2 |

---

## D-ADD-AFTER-SPLIT · Add node after a split rewires the second output {#d-add-after-split}

**Kind.** Defect. Confirmed by the probe `add_node_after_a_split_keeps_each_consumer_on_its_output` of the invariants model (its finding 2).

**Clauses.** [S5](03-operators.md#s5): "the inputs of other nodes … can each refer to a specific output of a split".

**Observed.** With a split selected, "Add node" moves *every* `consumers` of the split, from both outputs, onto the new node, and feeds the new node the split's output 0 ([state.rs:470-476](../../src/ui/state.rs#L470)). A node that read `split.right` now reads an effect applied to `split.left`.

**Consequence.** The localised-effect pattern the spec motivates splits with (split, effect on one side, stack) quietly breaks: both layers show the same half. Undo restores it.

**Decision.** Which output should "Add node after this split" attach to: output 0, the output being viewed, or one the user picks? Or should it be refused for nodes with several outputs? A regression test should come with the fix ([Q1](06-quality.md#q1)).

## D-STALE · Stale image after undo or a new document {#d-stale}

**Kind.** Defect. Confirmed by the probe `structural_hash_distinguishes_images_after_undo` of the invariants model (its finding 1).

**Clauses.** [F2.1](02-editing-and-views.md#f2-1) (the view shows what goes to the output), [O5](03-operators.md#o5) (background removal).

**Observed.** A static image is identified in the structural hash by a graph-local id from `fresh_id` ([graph.rs:109-113](../../src/graph.rs#L109)); undo and `Graph::new` reuse ids. The preview `key` is that hash ([image_view.rs:107-120](../../src/ui/image_view.rs#L107)), and the background-removal mask cache key builds on it (`link_hash`) ([bgremove.rs:66-70](../../src/nodes/bgremove.rs#L66)).

**Consequence, derived from the code.** Import A, undo, import B of the same size: no render is requested and the view keeps showing A. A background-removal node then masks B with A's cutout until the cache entry is evicted.

**Decision.** None about the spec, which is clear; this is a fix. The invariants model proposes hashing the process-unique `SourceImage::id` instead.

## D-IMPORT-LEFTOVER · A failed project import can leave a folder behind {#d-import-leftover}

**Kind.** Defect. Found by the independent review. **Confirmed.** Probe `failed_import_leaves_nothing_behind` ([probes/spec_probes.rs](probes/spec_probes.rs)): the import fails and leaves `future/` in the projects folder.

**Clauses.** [P15](04-sessions-and-projects.md#p15): "A failed import leaves nothing behind."

**Observed.** `unpack` deletes the destination only when extraction or `verify` fails ([project.rs:360-362](../../src/project.rs#L360)). `open` then calls `load`, which decodes the images and builds the graph, and returns its error without cleaning up ([project.rs:289-301](../../src/project.rs#L289)). An archive with a well-formed structure but an image that does not decode, or a node kind this version does not know, fails to open and leaves its unpacked copy in the projects folder.

**Decision.** None about the spec; this is a fix (move the `load` inside `unpack`'s cleanup, or clean up in `open`). A regression test should build such an archive, as `import_rejects_invalid_archives_and_cleans_up` does for invalid ones.

## D-OPEN-WRITES · Opening a generator-only project can rewrite it {#d-open-writes}

**Kind.** Defect. Found by the independent review. **Confirmed.** Probe `opening_a_generator_project_writes_nothing` ([probes/spec_probes.rs](probes/spec_probes.rs)): after opening, with a 1000×800 image pane, `project.delos` is rewritten with `:canvas_w 920 :canvas_h 736`.

**Clauses.** [P6.2](04-sessions-and-projects.md#p6-2): "Opening a project does not change anything on disk until the user edits it."

**Observed.** `replace_document` resets `canvas_user_set` ([state.rs:257](../../src/ui/state.rs#L257)). When the shown content is size-less, the image view calls `fit_canvas_to_window(false)` ([image_view.rs:69-76](../../src/ui/image_view.rs#L69)), which writes `canvas_w` and `canvas_h` to the graph if they differ from 92% of the viewport ([state.rs:323-344](../../src/ui/state.rs#L323)). That write triggers `use_autosave`. The end-to-end test runs without a viewport, so it cannot see this.

**Decision.** Should a saved canvas size be respected on open (set `canvas_user_set` when the loaded canvas is not the default), or is re-fitting to the window wanted? The spec's sentence says the former.

## D-CONFLICT · The size-conflict dialog hides the next conflict {#d-conflict}

**Kind.** Defect (small). Derived from the code; not probed, because the logic lives in the dialog's button closures, which no test can call.

**Clauses.** [Z1](03-operators.md#z1).

**Observed.** The two resize buttons call `app.edit`, whose `after_edit` looks for the next conflict and stores it, and then call `app.conflict.set(None)` ([dialogs.rs:42-55](../../src/ui/dialogs.rs#L42)), erasing it. A second mismatch elsewhere in the graph is only reported after another edit.

**Decision.** None; drop the `set(None)` (the edit already recomputes the conflict).

## D-EXPORT-MEMORY · Export memory is bounded for PNG files only {#d-export-memory}

**Kind.** Gap.

**Clauses.** [F4.2](02-editing-and-views.md#f4-2) ("renders … in bands (to bound memory)"), [Q3](06-quality.md#q3) ("exports must run in bounded memory").

**Observed.** Every format is *rendered* in bands, but only `Png::encode` streams them ([formats.rs:135-149](../../src/formats.rs#L135)). `ImageCrate::encode`, used for JPEG, WebP, GIF, BMP and TIFF, appends every band to `all`, a buffer of the whole image, then encodes into a second in-memory buffer `buf` ([formats.rs:83-110](../../src/formats.rs#L83)). JPEG also builds an RGB copy (`rgb`). Exports to memory, used on the web, hold the whole encoded file even for PNG (`ExportDest::Memory`) ([worker.rs:311-315](../../src/worker.rs#L311)).

**Consequence, derived from the code.** A 16384² JPEG export holds about 1 GiB of RGBA plus 768 MiB of RGB plus the encoded file in memory at once. The 16K tests export PNG only, so nothing catches this.

**Decision.** Either stream the other encoders (JPEG and TIFF can be written in strips) or narrow the spec: "bounded memory for PNG; other formats need memory proportional to the image".

## D-QUICK-EXIT · Quick-mode work can be lost without confirmation {#d-quick-exit}

**Kind.** Gap.

**Clauses.** [P7](04-sessions-and-projects.md#p7): "Leaving a quick-mode session that has work in it asks for confirmation."

**Observed.** The menu's End quick session and opening a project ask through `platform::confirm`, which returns `true` without asking outside desktop ([platform.rs:217-236](../../src/ui/platform.rs#L217)); this matters on mobile, since the web has no Project menu. Nothing intercepts closing the window or quitting: `launch` sets no close behaviour ([mod.rs:26-38](../../src/ui/mod.rs#L26)), and no close-request or `beforeunload` handler exists in `src/`.

**Consequence.** On every platform, closing the window discards quick-mode work silently. On the web, where *all* work is quick mode ([X2](04-sessions-and-projects.md#x2)), so does reloading the page.

**Decision.** Does "leaving" include quitting? If so, desktop needs a close handler and the web a `beforeunload` prompt. If not, the spec should say "ending a quick-mode session from the Project menu".

## D-PLAT · Platform gaps beyond the accepted ones {#d-plat}

**Kind.** Gap.

**Clauses.** [R2](01-product-and-platform.md#r2), [R3](01-product-and-platform.md#r3), [O5](03-operators.md#o5). The spec accepts only the *project* limitations ([X3](04-sessions-and-projects.md#x3)).

**Observed.**
- The `web` feature enables neither `gpu` nor `native`, and only `desktop` enables `rembg` ([Cargo.toml:6-13](../../Cargo.toml#L6)). The web build therefore renders on the CPU, single-threaded (background jobs run through `spawn_local`) ([worker.rs:417-425](../../src/worker.rs#L417)), and has no background removal; mobile has GPU but no background removal (the stub `compute_mask`) ([bgremove.rs:169-172](../../src/nodes/bgremove.rs#L169)).
- The README says the mobile build "compiles, but it has not been run on a device". No test covers web or mobile.

**Decision.** Extend the spec's "accepted for now" list to these gaps, or schedule them. GPU on the web needs asynchronous readback; threads need cross-origin isolation (README, Known gaps).

## D-RESIZE · Downscaling filters are not what they say {#d-resize}

**Kind.** Gap.

**Clauses.** [O8](03-operators.md#o8): "Allows choosing upscale/downscale algorithm to use." Also [O4](03-operators.md#o4), Downscale's "box" method.

**Observed.** When the request is at least as coarse as the input, or the input is size-less, `Resize::render` returns the area-filtered input whatever the `filter` says; only "nearest" changes the result ([resize.rs:88-98](../../src/nodes/resize.rs#L88)). Bilinear, bicubic and Fourier downscales are identical.

Downscale's "box" method is the source pyramid: for factors below 2, `level_for` returns level 0, sampled nearest, so "box" equals "nearest"; above 2 it uses a power-of-two level, not a box over the factor; size-less inputs are point-sampled ([source.rs:114-133](../../src/engine/source.rs#L114)).

**Decision.** Keep one downscaler and say so in the spec and the UI, or implement the filters for downscaling. Today the inspector offers four Resize choices where three behave identically, and a Downscale "box" that is not a box for small factors.

## D-OVERLAY · Overlay mapping: one path, affine maps only {#d-overlay}

**Kind.** Gap.

**Clauses.** [F3.3.1](02-editing-and-views.md#f3-3-1): controls "must be placed correctly even when nodes between the edited node and the shown image move or scale pixels".

**Observed.** `space_map_rec` returns the *first* path from the shown node to the edited node, trying ports in order ([graph.rs:318-343](../../src/graph.rs#L318)). Maps are separable affine `Map2`s ([geom.rs:95-102](../../src/engine/geom.rs#L95)); `Reflect` keeps the default identity `input_map` although it mirrors pixels.

**Consequence, derived from the code.**
- If the edited node reaches the shown image twice, for example as `stack(clip(photo), photo)`, its controls follow only the first route (here, through the Clip). There is no single correct placement in that case.
- Controls of a node upstream of a Reflection are placed for the unmirrored half only.

**Decision.** What should the view do when a node appears in the shown image more than once, or through a fold: draw every instance, draw the first and say so, or refuse? The spec's "placed correctly" does not say.

## D-ERROR-COLUMN · Project errors without a column {#d-error-column}

**Kind.** Gap (small).

**Clauses.** [E11](05-expressions.md#e11) ("Errors are reported with a line and column"), [P14](04-sessions-and-projects.md#p14).

**Observed.** `Project::verify` reports a missing or unsafe image as `project.delos, line N: …`, formatting `pos.line` only, although `pos.col` is known ([project.rs:218-223](../../src/project.rs#L218)).

**Decision.** Add the column (one line each), or limit the spec to syntax and semantic errors.

## D-MODALITIES · Only the layer stack has a display modality {#d-modalities}

**Kind.** Gap, or a spec ambiguity.

**Clauses.** [F5](02-editing-and-views.md#f5): "Delos shall define specific "display modalities" for standard operators".

**Observed.** The layer stack is the only operator with a dedicated presentation (graph view rows `stack_chain`, the inspector's `LayerStack`, the `(stack …)` syntax) ([graph_view.rs:75-87](../../src/ui/graph_view.rs#L75)). Chains of Blend nodes, or a split and its recombination, are drawn as plain nodes.

**Decision.** Is the stack the only modality the spec wants (its only example), or should other operators get one? If more are wanted, which: blend chains as layer stacks with per-layer modes, a split and its re-stack as a "selection"?

## D-EXPORT-TARGET · Default export while editing on the input {#d-export-target}

**Kind.** Decision.

**Clauses.** [F4](02-editing-and-views.md#f4): by default, "export the output currently visible in image view".

**Observed.** When a Clip or a split is selected with "Preview selection" on, the view shows the node's *input* with its controls, but Export exports the node's *output*: `export_target` is the edited node when the view is not locked ([state.rs:380](../../src/ui/state.rs#L380)). The field's doc comment says this is intended (`Displayed`) ([state.rs:90-92](../../src/ui/state.rs#L90)).

**Decision.** The code's choice is defensible (nobody wants to export the uncropped image while cropping), but it contradicts the spec's wording. Update one of them.

## D-IMPORT · Importing replaces what feeds the output {#d-import}

**Kind.** Decision.

**Clauses.** [F1.1](02-editing-and-views.md#f1-1): an import creates a Static Image "connected directly to the output".

**Observed.** `finish_import` connects each new image to the output's only port, replacing the previous connection; the earlier node stays in the graph, detached ([mod.rs:264-268](../../src/ui/mod.rs#L264)). Importing several files at once leaves the last one connected.

**Decision.** The spec is followed literally, but with a second import it destroys the composition feeding the output. Alternatives: connect the first import only, or add later imports as layers of a stack (the code already supports `ImportTarget::StackLayer`).

## D-DERIVED-PARAMS · Derived image parameters are not written {#d-derived-params}

**Kind.** Decision (small).

**Clauses.** [E8](05-expressions.md#e8): "every parameter is written, including defaults".

**Observed.** A static image's `image`, `width` and `height` parameters are skipped when printing and rejected when reading (`IMAGE_DERIVED`) ([expr.rs:35-36](../../src/expr.rs#L35), [expr.rs:355-358](../../src/expr.rs#L355)), because they are derived from the file.

**Decision.** None needed beyond noting the exception in the spec: these are hidden, derived values, not user parameters.

## D-SPEC-WORDING · Spec wording that cannot hold as written {#d-spec-wording}

**Kind.** Spec issue.

**Observed.**
- [N1](03-operators.md#n1): generators "should act as generators of data which can be input to other generators", but the previous sentence says they take no inputs. The code reads "other operators".
- [F5](02-editing-and-views.md#f5): a layer stack "will never be empty", yet the inspector offers "+ Empty slot" and an empty slot is a `None` layer (`push(None)`) ([inspector.rs:523](../../src/ui/inspector.rs#L523)). The chain itself is never empty, so no NIL node is needed, which is the spec's point; the wording invites confusion.
- [O4](03-operators.md#o4): "Downscale operator - should allow for downscaling" is a sentence fragment, and Resize already downscales. The code gives Downscale a factor from 1 to 64 and a box or nearest method. The spec does not say how the two should differ.
- [P2](04-sessions-and-projects.md#p2): projects are "for sharing and collaborating with others"; only sharing archives exists. Collaboration needs a definition before it can be traced.

**Decision.** Reword these four places so that each states something the code can be checked against.
