# 6 · Persistence: expressions, projects and formats

A project is a folder holding `project.delos` (the graph as a Lisp-like *Delos
expression*) and an `inputs/` folder with copies of the images it uses.
Files: [expr.rs](../../src/expr.rs), [project.rs](../../src/project.rs),
[formats.rs](../../src/formats.rs), and the autosave in
[session.rs](../../src/ui/session.rs) and [worker.rs](../../src/worker.rs).

---

## E1 · Saving and loading reproduce a functionally identical graph {#e1}

**Statement** (spec): "Doing both must give a functionally identical graph: the
same nodes, connections, parameters, titles and positions, rendering the same
pixels." The implementation guarantees a precise version of this:

- **Printing is a fixed point.** `to_text(from_text(to_text(g))) == to_text(g)`.
- **Pixels are equal.** A graph loaded from a save renders the same pixels as the original.
- **Parameters round-trip exactly.** Floats are printed with Rust's shortest representation that parses back to the same value ([expr.rs:295](../../src/expr.rs#L295)).

**What is *not* preserved.** These are intentional or harmless, but they are
not listed anywhere:
- **Node and image ids** are reassigned on load. This is why [G5](01-document-model.md#g5) and [I1](04-structural-identity.md#i1) matter.
- **Positions of interior stacks.** They are written inline without `:at` and placed again by `set_stack_layers` ([S1](01-document-model.md#s1)).
- **An image node whose label was cleared** comes back titled with its file name, because the builder sets the label to the file name when none is given ([expr.rs:615](../../src/expr.rs#L615)).
- **Non-finite floats.** `+inf` parses as a number, but it prints as `inf`, which reads back as a symbol. Only a hand-written file can contain one.

**Maintained by**
- **Every parameter is written, defaults included** ([expr.rs:355-363](../../src/expr.rs#L355)), so changing a default later does not change the meaning of saved projects. When reading, omitted parameters take their defaults ([G7](01-document-model.md#g7)).
- **Nodes used more than once, and nodes with several outputs, are bound with `def`.** Defs are printed in dependency order (DFS post-order), so every name is defined before it is used ([expr.rs:391-411](../../src/expr.rs#L391)). That is also why loading is acyclic ([G2](01-document-model.md#g2)).
- **Generated names are unique** and never collide with reserved words or the `.output` syntax (`is_name`, the `taken` set, [expr.rs:281-291](../../src/expr.rs#L281), [expr.rs:412-426](../../src/expr.rs#L412)).
- **Detached nodes are printed as top-level forms**, so nothing is lost ([expr.rs:437-440](../../src/expr.rs#L437)).
- A node with several outputs that appears inline is rejected on load with a hint ([expr.rs:531-533](../../src/expr.rs#L531)).

**Tests.** The unit tests in `src/expr.rs`, `tests/project.rs::save_and_load_give_the_same_graph`
(pixel by pixel), `projects_with_splits_round_trip`, and `large_graphs_serialize_quickly`.

**Status.** Holds in the precise sense given above.

---

## E2 · Load errors carry a line and column {#e2}

**Statement.** Every syntax and semantic error from `parse` or `build` is an
`ExprError { line, col }` ([expr.rs:48-65](../../src/expr.rs#L48)). Project
verification adds the file name and line ([project.rs:217-221](../../src/project.rs#L217)).

**Depends on.** `Sexp::Num` holds only text that parses as `f64` (the `numeric`
test at [expr.rs:198](../../src/expr.rs#L198)), which is why the later
`.parse().unwrap()` calls are safe ([expr.rs:647](../../src/expr.rs#L647), [expr.rs:704-706](../../src/expr.rs#L704)).

**Status.** Holds.

---

## J1 · `inputs/` holds exactly the images the graph uses, after every successful save {#j1}

**Statement.** After `sync_inputs` succeeds ([project.rs:141-182](../../src/project.rs#L141)):
- Every image used by an image node exists in `inputs/`. Missing ones are copied in from their origin, or re-encoded as PNG when there is no origin (`adopt_image`, [project.rs:117-133](../../src/project.rs#L117)).
- Every other regular file in `inputs/` is moved to the session stash.

**Ordering.** `sync_inputs` runs **before** `project.delos` is written
([worker.rs:366-370](../../src/worker.rs#L366)). The file on disk never names an
image that has not been copied in yet.

**Between saves** the invariant can be briefly false: images are imported
without a project path and adopted at the next save. If a save fails, the
footer shows the error and the next edit retries.

**Copies are byte-for-byte** when the original bytes are available. Imports
inside a project keep the bytes in the stash when there is no origin path
(`keep_bytes`, [worker.rs:258-264](../../src/worker.rs#L258)). Without them, the
PNG re-encode keeps the rendered pixels but not the original encoding.

**Tests.** `tests/project.rs::sync_keeps_inputs_equal_to_the_images_in_use` and
`sync_copies_images_without_a_file_and_moves_out_strays`, and
`tests/project_e2e.rs::removed_images_leave_the_project_and_undo_restores_them`.

**Status.** Holds.

---

## J2 · `project.delos` is never half-written {#j2}

**Statement.** Every write goes through `write_atomic`, which writes a temporary
file named `<name>.<pid>.tmp` and renames it into place ([project.rs:392-399](../../src/project.rs#L392)).
Archives use the same pattern ([project.rs:258-283](../../src/project.rs#L258)).
`.tmp` files and dotfiles are ignored when syncing and packing.

**Scope.** This is atomic for *readers* and against the *process* dying. There
is no `fsync`, so after a power failure the file system may hold either version
or, on some file systems, an empty file. The spec's "a project file is never
left half-written" is met for the process but not for the disk.

**Status.** Holds against process crashes. Not durable against power loss.

---

## J3 · Opening a project changes nothing on disk until an edit {#j3}

**Statement.** Loading never writes. After `on_project_opened` sets
`SaveState::Saved` ([session.rs:211](../../src/ui/session.rs#L211)), the autosave
effect sees a **new project directory together with `Saved`** and skips that one
run ([session.rs:259-262](../../src/ui/session.rs#L259)). The first real edit
saves normally. Even then, `project.delos` is rewritten only if its text changed.

**Fragility.** The skip relies on the pairing "directory just switched and state
is `Saved`". If any other path ever set `Saved` before switching directory, that
path would skip its first save as well.

**Tests.** `tests/project_e2e.rs::opening_a_project_does_not_rewrite_it`.

**Status.** Holds.

---

## J4 · Project paths stay inside the project {#j4}

**Statement.**
- Every image path in `project.delos` must be `inputs/<normal components>`, with no `..`, no absolute paths and no root (`resolve`, [project.rs:196-204](../../src/project.rs#L196)). `verify` checks this before anything is decoded ([project.rs:218-223](../../src/project.rs#L218)).
- Archive entries must have an `enclosed_name` ([project.rs:312-314](../../src/project.rs#L312)). Entries outside the project's top folder are skipped ([project.rs:334-337](../../src/project.rs#L334)).

**Depended on by.** Safety when opening shared projects: a malicious
`.delosproject` file cannot write outside its new folder or read arbitrary files.

**Tests.** `tests/project.rs::verify_rejects_broken_projects` and
`import_rejects_invalid_archives_and_cleans_up`.

**Status.** Holds.

---

## J5 · Importing never overwrites, and a failed import leaves nothing behind {#j5}

**Statement.** An archive is unpacked into `unique_dir(root, name)`, which adds
`-2`, `-3` and so on as needed ([project.rs:55-64](../../src/project.rs#L55)).
If unpacking or verification fails, the new folder is deleted
([project.rs:360-362](../../src/project.rs#L360)). `Project::create` refuses a
folder that is not empty ([project.rs:70-72](../../src/project.rs#L70)).
Re-importing identical bytes under the same name reuses the existing file in
`inputs/` ([project.rs:106-108](../../src/project.rs#L106)).

**Status.** Holds.

---

## J6 · Every image the undo history can restore stays restorable {#j6}

**Statement.** For each `SourceImage` held by the current graph or any undo or
redo snapshot, its `file` record gives a way to get its original bytes back:
either a `project` path that exists, or an `origin` path (possibly in the stash).

**Maintained by**
- `SourceImage::file` is a `Mutex` **shared by every snapshot** that holds the image through its `Arc` ([source.rs:22-24](../../src/engine/source.rs#L22)). Updating the record once updates it for every snapshot.
- When `sync_inputs` moves a file out, every *known* image that pointed at it is updated: its project path is cleared and its origin set to the stashed copy ([project.rs:176-179](../../src/project.rs#L176)). The known images are `App::all_images()`, which covers the current graph, the undo list and the redo list ([state.rs:230-243](../../src/ui/state.rs#L230)).
- When an undo brings the image back, the next save sees no project path and copies it back from the stash unchanged ([project.rs:149-152](../../src/project.rs#L149)).

**Fragility**
- The stash is in the system temp folder, and a new Delos process deletes stash folders whose mtime is more than a day old ([worker.rs:402-415](../../src/worker.rs#L402)). A session longer than 24 hours, combined with a second instance being started, can lose stashed files. The fallback in `adopt_image` then re-encodes the pixels as PNG ([project.rs:121-128](../../src/project.rs#L121)). The pixels are right but the bytes are not the originals.
- An image dropped from all 60 snapshots is gone from memory, and its stash file is orphaned until the stash is pruned.

**Tests.** `tests/project_e2e.rs::removed_images_leave_the_project_and_undo_restores_them`.

**Status.** Holds within a normal session.

---

## F1 · Decoding sniffs content and yields straight RGBA8; export converts back exactly {#f1}

**Statement.**
- Codecs are tried in registry order by `can_decode` on the bytes. The file extension is never trusted ([formats.rs:183-186](../../src/formats.rs#L183)). Decoding has no size limits, since 16K images are normal here ([formats.rs:74-75](../../src/formats.rs#L74)).
- Decoded images are straight-alpha RGBA8. They are premultiplied once, in `SourceImage::from_rgba8` ([R9](02-geometry-and-rendering.md#r9)).
- Export unpremultiplies and rounds to RGBA8 (`tile_to_rgba8`, [formats.rs:189-205](../../src/formats.rs#L189)). JPEG composites over white because it has no alpha ([formats.rs:91-99](../../src/formats.rs#L91)).
- Adding a format means adding one `Codec` entry. Export dialogs and file-picker filters are derived from the registry (`encoders`, `accept_attr`).

**Tests.** `tests/engine.rs::native_render_reproduces_source` (decode → render → equal pixels) and `tests/export.rs`.

**Status.** Holds.
