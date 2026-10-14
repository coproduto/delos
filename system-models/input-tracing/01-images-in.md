# 01 · Images in: import, drop, "+ Image layer"

Image bytes from outside are the program's main raw input. They arrive
through three UI doors, all of which call **one function**,
`session::import` ([session.rs:100](../../src/ui/session.rs#L100)).

| Input | Entry | Target |
|---|---|---|
| Toolbar **Import** (hidden `<input type=file multiple>`) | [mod.rs:351](../../src/ui/mod.rs#L351) | connect to the output |
| **Drop files** anywhere on the window | [mod.rs:86](../../src/ui/mod.rs#L86) (`ondragover` [mod.rs:85](../../src/ui/mod.rs#L85) only allows the drop) | connect to the output |
| Layer stack **"+ Image layer"** in the inspector | [inspector.rs:499](../../src/ui/inspector.rs#L499) | becomes the front layer of that stack |

`ImportTarget` ([state.rs:70](../../src/ui/state.rs#L70)) carries the destination
across the asynchronous gap. It is stored in `App.imports` under a token
that comes back with the result.

## Trace: Toolbar → Import → pick `beach.png` (desktop, fresh window)

1. The `onchange` handler ([mod.rs:351-356](../../src/ui/mod.rs#L351)) wraps each
   `FileData` in `ImportSource::File` and calls
   `session::import(app, worker, files, ImportTarget::Output)`.
2. **The session decides what happens next** ([session.rs:100-120](../../src/ui/session.rs#L100)):
   - `Session::Fresh` on desktop or mobile: nothing is read yet. It sets
     `App.project_prompt` with the files as *pending*, a suggested name
     (`beach`) and the projects root, then **returns**. The
     `ProjectDialog` renders "Start a project?" ([dialogs.rs:204](../../src/ui/dialogs.rs#L204)).
   - Fresh on web (no projects): switches to `Quick` and continues.
   - `Quick` or `Project`: continues straight to step 4.
3. **The user answers the prompt** (see [07-projects.md](07-projects.md#prompt)):
   - **Quick mode** ([dialogs.rs:286](../../src/ui/dialogs.rs#L286) → `choose_quick`, [session.rs:123](../../src/ui/session.rs#L123)): session becomes `Quick`, and each pending file goes to `start_imports`.
   - **Create project** ([dialogs.rs:295](../../src/ui/dialogs.rs#L295) → `create_project`, [session.rs:154](../../src/ui/session.rs#L154)): ⇢ thread `Project::create`. When ⇢ `Msg::ProjectCreated` arrives, `on_project_created` ([session.rs:168](../../src/ui/session.rs#L168)) sets `Session::Project` and only then starts the pending imports.
   - **× / Esc / Cancel**: the prompt is dropped with its pending files, and **nothing is imported** (tested by `cancelling_the_prompt_imports_nothing`).
4. **`start_imports`** ([session.rs:276](../../src/ui/session.rs#L276)) bumps `App.importing` (the toolbar shows "Importing…"). It issues a token, stores `token → target` in `App.imports`, and sets `keep_bytes` when a project is open.
   - For a `File`, ⇢ task: on desktop, it records the file's real path as its **origin** when there is one ([session.rs:295-298](../../src/ui/session.rs#L295)). It reads the bytes (`read_bytes`, [session.rs:300](../../src/ui/session.rs#L300)) and calls `Worker::import`. A read error undoes the bookkeeping and shows a toast.
   - `Bytes` and `Path` sources (used by tests and project code) go straight to the worker.
5. **`Worker::import`** ([worker.rs:242](../../src/worker.rs#L242)) ⇢ thread:
   - bytes, or `fs::read(origin)` ([worker.rs:253](../../src/worker.rs#L253));
   - `formats::decode` ([formats.rs:183](../../src/formats.rs#L183)): sniffs the codec (PNG, JPEG, WebP, GIF, BMP, TIFF, ICO). "unsupported or unrecognised image format" ends here.
   - With no origin but `keep_bytes` (a project open, bytes from a browser drop), the original bytes are written to the session stash, so the project can later copy the **unchanged** file ([worker.rs:258-264](../../src/worker.rs#L258)).
   - `SourceImage::from_rgba8` premultiplies and builds the mip pyramid ([source.rs:43](../../src/engine/source.rs#L43)). `Engine::make_resident` starts uploading it to the GPU in the background.
   - ⇢ `Msg::Imported { token, result }` ([worker.rs:275](../../src/worker.rs#L275)).
6. **`pump`** ([mod.rs:180-187](../../src/ui/mod.rs#L180)) decrements `importing`, takes the target for the token, and calls `finish_import`. An error becomes the toast "Could not open beach.png: …".
7. **`finish_import`** ([mod.rs:251](../../src/ui/mod.rs#L251)) is **one undoable edit**:
   it registers the image in `Graph.images`, adds a Static Image node with
   the image id, width and height as parameters, and labels it with the file
   name. Then:
   - `Output` target: connects it to the output, **replacing** whatever fed the output. It clears the relocated output and the selection, and turns fit on.
   - `StackLayer(head)` target: inserts it as layer 0 of that stack.
   - Toast "Imported beach.png (W×H)".
8. ⇢ effects: `graph` changed, so the **preview** effect renders the new
   output, the **fit** effect fits it and, in a project, **autosave** copies
   the file into `inputs/` on its first save (`Project::sync_inputs` →
   `adopt_image`, [project.rs:141](../../src/project.rs#L141), [project.rs:117](../../src/project.rs#L117)).
   See [07-projects.md](07-projects.md#autosave).

Generated by `inputtrace trace . src/ui/mod.rs:351` (abridged; the raw tree
shows every hop including the effects).

## Trace: dropping files on the window

The `ondrop` handler ([mod.rs:86-105](../../src/ui/mod.rs#L86)):

1. On **desktop only**, if any dropped file is a project (a folder, a
   `project.delos` or a `.delosproject`, `is_project_path`
   [mod.rs:246](../../src/ui/mod.rs#L246)), the first such file wins. Unsaved quick
   work needs a native confirm first. Then `session::open_project` runs
   (trace in [07-projects.md](07-projects.md#open)). **Any image files dropped
   with it are ignored.**
2. Otherwise all dropped files go to `session::import(…, Output)` exactly as
   in the toolbar trace.

## Trace: Layer stack → "+ Image layer"

[inspector.rs:499-502](../../src/ui/inspector.rs#L499) is the same as the toolbar,
with target `StackLayer(head)`. In step 7 `finish_import` inserts the node as
the front layer through `Graph::set_stack_layers`, which reuses or adds
binary `stack` nodes ([graph.rs:470](../../src/graph.rs#L470)). The selection and
view are left alone. If the stack was deleted while the image was decoding,
`stack_chain(head)` returns `None` and the node is added unconnected.

## Decisions

- **The first import asks a question that changes where data lives.** One import can either leave the file where it is (quick mode) or start copying it into a project. Cancelling the prompt discards the import entirely. That is deliberate (and tested), but it is a product decision.
- **Origin vs. stash.** The original bytes are kept (as a path or a stash copy) so the project copy is byte-identical, not re-encoded. Only images that come from neither path are re-encoded as PNG ([project.rs:121-128](../../src/project.rs#L121)).
- **"Import" replaces the output's input.** A second import disconnects the first image from the output (the first node stays in the graph). There is no "add as layer" default.
- **Mixed drops.** Dropping a project together with images opens the project and silently drops the images.
