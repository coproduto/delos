# 4 · Sessions and projects

The spec's *Sessions: quick mode and projects*, *Project structure*,
*Sharing and importing projects* and *Platform status*. Every session
action is a plain function in `src/ui/session.rs`, so the end-to-end tests
call exactly what the buttons call.

## P1 · Two ways of working; quick mode {#p1}

> Delos supports two ways of working:
> Quick mode is for one-off edits and quick work.
> Imported images are used where they are and nothing is saved.

**Status.** Implemented.

**Implemented by.**
- `Session` is `Fresh`, `Quick` or `Project` ([session.rs:14-24](../../src/ui/session.rs#L14)).
- In quick mode, `start_imports` sets `keep_bytes` to false, so nothing is copied ([session.rs:277](../../src/ui/session.rs#L277)); `Worker::import` decodes the image from its path or bytes and keeps that path as its `origin` ([worker.rs:250-256](../../src/worker.rs#L250), [worker.rs:266-269](../../src/worker.rs#L266)).
- `use_autosave` saves only a `Session::Project` ([session.rs:255](../../src/ui/session.rs#L255), [session.rs:263-265](../../src/ui/session.rs#L263)).

**Tests.** `tests/project_e2e.rs::quick_mode_imports_without_copying`.

## P2 · Project mode {#p2}

> Project mode is for longer-term work, and for sharing and collaborating with others.

**Status.** Implemented, as far as a purpose statement can be: projects persist, reopen and can be shared as archives. There is no collaboration beyond exchanging archives (no merging, no concurrent editing); the spec does not ask for more.

**Implemented by.**
- `Project` is a directory ([project.rs:33-36](../../src/project.rs#L33)); see [P8](#p8) to [P15](#p15).

**Tests.** `tests/project_e2e.rs::edits_are_autosaved_and_reopen_identically`, `tests/project_e2e.rs::share_and_import_a_project`.

## P3 · First import asks whether to start a project {#p3}

> When the user imports an image and neither a project nor a quick-mode session is open, Delos asks whether to create a project.

**Status.** Implemented on desktop and mobile. On the web, where projects are not supported, the first import starts quick mode without asking (as [X2](#x2) expects).

**Implemented by.**
- `session::import` opens a `ProjectPrompt` holding the pending imports when the session is `Fresh` and `PROJECTS_SUPPORTED` ([session.rs:105-115](../../src/ui/session.rs#L105)); `PROJECTS_SUPPORTED` is the `native` feature ([session.rs:86-87](../../src/ui/session.rs#L86)).
- `ProjectDialog` titles it "Start a project?" when imports are pending (`from_import`) and offers "Quick mode" and "Create project" ([dialogs.rs:210](../../src/ui/dialogs.rs#L210), [dialogs.rs:221](../../src/ui/dialogs.rs#L221), [dialogs.rs:281-288](../../src/ui/dialogs.rs#L281), [dialogs.rs:292-302](../../src/ui/dialogs.rs#L292)).

**Tests.** `tests/project_e2e.rs::first_import_asks_and_creates_a_project`, `tests/project_e2e.rs::cancelling_the_prompt_imports_nothing`.

**Notes.** Cancelling the prompt (× or Escape) drops the pending imports and stays `Fresh`.

## P3.1 · The question mentions the storage cost {#p3-1}

> The question makes clear that projects copy every imported image, so they take more storage.

**Status.** Implemented.

**Implemented by.**
- The dialog text: every image is `copied` into `inputs/`, "so projects take more disk space than the original files" ([dialogs.rs:231-238](../../src/ui/dialogs.rs#L231)).

**Tests.** None (UI text).

## P3.2 · Declining starts quick mode for good {#p3-2}

> If the user declines, a quick-mode session starts and later imports are added to it without asking again.

**Status.** Implemented.

**Implemented by.**
- `choose_quick` sets `Session::Quick` and runs the pending imports ([session.rs:123-131](../../src/ui/session.rs#L123)); `import` asks only while `Fresh` and otherwise goes straight to `start_imports` ([session.rs:105](../../src/ui/session.rs#L105), [session.rs:119](../../src/ui/session.rs#L119)).

**Tests.** `tests/project_e2e.rs::quick_mode_imports_without_copying`.

## P3.3 · Accepting creates a named project in the default location {#p3-3}

> If the user accepts, a project is created (named by the user; by default in `~/.delos/projects/<name>`, overridable with `DELOS_PROJECTS_DIR`) and the image is added to it.

**Status.** Implemented.

**Implemented by.**
- `default_root` reads `DELOS_PROJECTS_DIR`, else `~/.delos/projects` ([project.rs:38-45](../../src/project.rs#L38)); the prompt's name defaults to the image's file `stem` and its folder to `projects_root` ([session.rs:107-112](../../src/ui/session.rs#L107)); the user edits the name through `set_prompt_name` ([session.rs:140-143](../../src/ui/session.rs#L140)).
- `create_project` joins the name to the chosen folder and asks `Worker::create_project` to run `Project::create`, which makes the folder ([session.rs:156](../../src/ui/session.rs#L156), [session.rs:164](../../src/ui/session.rs#L164), [worker.rs:326-333](../../src/worker.rs#L326), [project.rs:73-76](../../src/project.rs#L73)).
- `pump` hands `Msg::ProjectCreated` to `on_project_created` ([mod.rs:221](../../src/ui/mod.rs#L221)), which switches to `Session::Project` and starts the pending imports (`start_imports`, which sets `keep_bytes` in a project) ([session.rs:172](../../src/ui/session.rs#L172), [session.rs:174](../../src/ui/session.rs#L174), [session.rs:177-179](../../src/ui/session.rs#L177)); the next autosave copies the image into `inputs/` (`sync_inputs`, see [P9](#p9)).

**Tests.** `tests/project_e2e.rs::first_import_asks_and_creates_a_project`, `tests/project_e2e.rs::project_name_collisions_and_save_failures_are_reported`, `tests/project.rs::names_and_locations`.

**Notes.** The name is passed through `sanitize_name` to make a folder name; the location can be changed with a folder picker on desktop. `create_project` refuses an existing non-empty folder with a message in the dialog ([session.rs:157-162](../../src/ui/session.rs#L157)), and `Project::create` checks the same ([project.rs:70-72](../../src/project.rs#L70)).

## P4 · New project from current work {#p4}

> The current work can also be turned into a project at any time ("New project from current work").
> The images the graph uses are copied into it.

**Status.** Implemented.

**Implemented by.**
- The Project menu item, labelled "Save as new project…" when a project is already open ([mod.rs:525-533](../../src/ui/mod.rs#L525)), opens the prompt with no pending imports (`prompt_new_project`) ([session.rs:134-137](../../src/ui/session.rs#L134)).
- After `on_project_created` switches the session, `use_autosave` saves the current graph, and `sync_inputs` copies every image it uses ([session.rs:255](../../src/ui/session.rs#L255), [session.rs:263-265](../../src/ui/session.rs#L263), [project.rs:141-153](../../src/project.rs#L141)).

**Tests.** `tests/project_e2e.rs::quick_session_becomes_a_project_with_the_images_it_uses`.

## P5 · The footer shows mode, location and save state {#p5}

> The bottom of the window always shows whether Delos is in quick mode or in a project (with its name and location), and whether the project's changes are saved.

**Status.** Implemented.

**Implemented by.**
- `Footer` reads the `session` and `save_state` ([mod.rs:560-561](../../src/ui/mod.rs#L560)) and shows "No project", "Quick mode" or "Project" with `name` and `dir`, and the `SaveState` as "Saving…", "All changes saved" or "Not saved: …" ([mod.rs:563-575](../../src/ui/mod.rs#L563), [mod.rs:578-579](../../src/ui/mod.rs#L578), [mod.rs:584-586](../../src/ui/mod.rs#L584)).
- `Worker::save_project` reports each save as `Msg::ProjectSaved` ([worker.rs:376](../../src/worker.rs#L376)), which sets the state ([mod.rs:225-231](../../src/ui/mod.rs#L225)).

**Tests.** `tests/project_e2e.rs::edits_are_autosaved_and_reopen_identically` (checks `SaveState`), `tests/project_e2e.rs::project_name_collisions_and_save_failures_are_reported`.

**Notes.** The desktop window title also shows the mode (`set_title`). The invariants model notes that out-of-order save completions could show "All changes saved" early (its fragile spot 10).

## P6 · Autosave after every change {#p6}

> Projects are saved automatically after every change.

**Status.** Implemented.

**Implemented by.**
- `use_autosave` is an effect on `graph` and `session`: each change bumps `seq` and calls `Worker::save_project` with every image the session holds (`all_images`) ([session.rs:251-256](../../src/ui/session.rs#L251), [session.rs:263-265](../../src/ui/session.rs#L263)).
- `save_project` runs in the background (`run_background`), serialises the graph with `to_text` and writes `project.delos` with `save_text` ([worker.rs:356](../../src/worker.rs#L356), [worker.rs:360](../../src/worker.rs#L360), [worker.rs:365](../../src/worker.rs#L365), [worker.rs:367-373](../../src/worker.rs#L367)).

**Tests.** `tests/project_e2e.rs::edits_are_autosaved_and_reopen_identically`.

**Notes.** "Every change" includes moving nodes in the graph view (positions are saved as `:at`) and the automatic canvas fit ([N3](03-operators.md#n3)). `save_project` skips saves older than the last written one (`saved_seq`) ([worker.rs:361-364](../../src/worker.rs#L361)), syncs `inputs/` first (`sync_inputs`, see [P10](#p10)) ([worker.rs:366](../../src/worker.rs#L366)), and rewrites `project.delos` only when its text changed ([worker.rs:368](../../src/worker.rs#L368)).

## P6.1 · Atomic saves {#p6-1}

> Saves are atomic, so a project file is never left half-written.

**Status.** Implemented, without `fsync`: a crash cannot leave a half-written file in place, but power loss can lose the last save.

**Implemented by.**
- `write_atomic` writes a temporary file and renames it ([project.rs:390-399](../../src/project.rs#L390)); `save_text` uses it for `project.delos` ([project.rs:191-193](../../src/project.rs#L191)) and `add_input` for copied images ([project.rs:103](../../src/project.rs#L103)).
- `share` writes the archive the same way (`tmp_path`, then `rename`) ([project.rs:258](../../src/project.rs#L258), [project.rs:277-283](../../src/project.rs#L277)).

**Tests.** None (atomicity under failure is not tested).

## P6.2 · Opening a project writes nothing {#p6-2}

> Opening a project does not change anything on disk until the user edits it.

**Status.** Partial. The autosave skips the first run after opening, but a generator-only project can be rewritten without any edit: `replace_document` resets `canvas_user_set`, and the image view's automatic canvas fit then writes `canvas_w`/`canvas_h` when the window size differs, which autosaves ([D-OPEN-WRITES](findings.md#d-open-writes)).

**Implemented by.**
- `on_project_opened` sets `SaveState::Saved` ([session.rs:210-211](../../src/ui/session.rs#L210)); `use_autosave` skips the first run after the project directory changes while the state is `Saved` (`switched`) ([session.rs:257-262](../../src/ui/session.rs#L257)).

**Tests.** `tests/project_e2e.rs::opening_a_project_does_not_rewrite_it` (runs headless, with no viewport, so the canvas fit never fires).

**Notes.** The fit: `fit_canvas_to_window` ([state.rs:323-344](../../src/ui/state.rs#L323)), called by the image view whenever size-less content is shown ([image_view.rs:69-76](../../src/ui/image_view.rs#L69)); `canvas_user_set` is reset in `replace_document` ([state.rs:257](../../src/ui/state.rs#L257)). Confirmed by a failing probe ([D-OPEN-WRITES](findings.md#d-open-writes)). Importing a `.delosproject` necessarily writes the unpacked copy; the clause concerns the opened project's own files.

## P7 · Confirm before leaving quick mode with work {#p7}

> Leaving a quick-mode session that has work in it asks for confirmation.

**Status.** Partial. On desktop, End quick session and opening a project ask first. On mobile, `confirm` answers yes without asking. On the web the Project menu is hidden, so the only way to leave is closing the tab, and on every platform closing the window or quitting is not intercepted ([D-QUICK-EXIT](findings.md#d-quick-exit)).

**Implemented by.**
- `has_unsaved_work`: not `Fresh`, not a project, more than the output node ([session.rs:243-245](../../src/ui/session.rs#L243)).
- Callers: "End quick session" (`close`) ([mod.rs:509-517](../../src/ui/mod.rs#L509)), opening a project from the menu (`open_with`) ([mod.rs:479-481](../../src/ui/mod.rs#L479)) or by dropping one, also guarded by `has_unsaved_work` ([mod.rs:96](../../src/ui/mod.rs#L96)).
- `platform::confirm` shows a native dialog on desktop and returns `true` elsewhere ([platform.rs:217-236](../../src/ui/platform.rs#L217)).

**Tests.** `tests/project_e2e.rs::quick_mode_imports_without_copying` asserts `has_unsaved_work`; the confirmation itself is UI and untested.

## P8 · A project is a directory with project.delos and inputs/ {#p8}

> A project is a directory:

**Status.** Implemented.

**Implemented by.**
- `PROJECT_FILE` and `INPUTS_DIR` ([project.rs:29-30](../../src/project.rs#L29)); `Project::create` makes `inputs/` and writes an empty graph ([project.rs:73-76](../../src/project.rs#L73)).
- Images are referred to as `inputs/<file>` (`add_input`) ([project.rs:112](../../src/project.rs#L112)).

**Tests.** `tests/project.rs::create_makes_the_project_layout`.

## P9 · Images copied unchanged, or as PNG {#p9}

> Imported images are copied into `inputs/` unchanged (no re-encoding) when their original bytes are available.
> Images known only by their pixels are written as PNG.

**Status.** Implemented.

**Implemented by.**
- `adopt_image` copies the bytes of the image's `origin` file, else encodes `to_rgba8` as PNG, and points the image at its copy (`set_project_file`) ([project.rs:117-131](../../src/project.rs#L117)). `add_input` writes those bytes as they are into `inputs/` (`write_atomic`) ([project.rs:90-93](../../src/project.rs#L90), [project.rs:101-104](../../src/project.rs#L101)).
- Images dropped or picked without a path keep their bytes in the session stash when a project is open (`keep_bytes`), so they too have an `origin` ([worker.rs:257-264](../../src/worker.rs#L257)).

**Tests.** `tests/project.rs::inputs_are_copied_and_deduplicated`, `tests/project.rs::sync_copies_images_without_a_file_and_moves_out_strays`, `tests/project_e2e.rs::first_import_asks_and_creates_a_project`.

**Notes.** `add_input` reuses an identical file of the same name and otherwise picks a free name ([project.rs:98-111](../../src/project.rs#L98)).

## P10 · inputs/ holds exactly the images in use {#p10}

> `inputs/` contains exactly the images the current graph uses.
> Images that are no longer used are removed from it.

**Status.** Implemented.

**Implemented by.**
- `sync_inputs` adopts used images that lack a file and moves every other file out to the stash ([project.rs:142-153](../../src/project.rs#L142), [project.rs:154-175](../../src/project.rs#L154)); it runs before each save ([worker.rs:366](../../src/worker.rs#L366)).

**Tests.** `tests/project.rs::sync_keeps_inputs_equal_to_the_images_in_use`, `tests/project_e2e.rs::removed_images_leave_the_project_and_undo_restores_them`.

**Notes.** Hidden files, `*.tmp` files and anything that is not a regular file (subfolders) are left alone. Until the first edit after opening, a stray file in `inputs/` stays (by [P6.2](#p6-2)).

## P11 · Undo history is not part of the project {#p11}

> Undo history is not part of a project.
> It belongs to the editing session and is not expected to survive closing and reopening a project.

**Status.** Implemented.

**Implemented by.**
- `History` (undo and redo stacks of `Graph` snapshots) lives in `App`, not in the project ([state.rs:100-106](../../src/ui/state.rs#L100)); `replace_document` clears it on open and close ([state.rs:249](../../src/ui/state.rs#L249)).
- `to_text` serialises only the graph ([expr.rs:376](../../src/expr.rs#L376)).

**Tests.** `tests/project_e2e.rs::edits_are_autosaved_and_reopen_identically` (asserts no undo after reopening).

## P11.1 · Images the history needs live in a session stash {#p11-1}

> Within a session, images the history may still need are kept outside the project (in a temporary session folder).

**Status.** Implemented.

**Implemented by.**
- `new_stash_dir` makes a per-session folder under the system temp directory, named from the process id and `now_ms` ([worker.rs:402-403](../../src/worker.rs#L402), [worker.rs:413-414](../../src/worker.rs#L413)).
- `sync_inputs` moves unused files there and repoints every `known` image to the stashed copy (`set_origin`) ([project.rs:163-180](../../src/project.rs#L163)).

**Tests.** `tests/project_e2e.rs::removed_images_leave_the_project_and_undo_restores_them`.

**Notes.** The decoded pixels of every image in the history also stay in memory (up to `HISTORY_LIMIT` = 60 snapshots). `new_stash_dir` also prunes stashes left by earlier sessions that are older than a day ([worker.rs:404-412](../../src/worker.rs#L404)).

## P11.2 · Undo copies images back unchanged {#p11-2}

> If an undo brings an image back, it is copied back into `inputs/` unchanged.

**Status.** Implemented.

**Implemented by.**
- After an undo, the next autosave's `sync_inputs` finds the image without a project file and calls `adopt_image`, which copies the stashed bytes (`origin`) ([project.rs:148-153](../../src/project.rs#L148), [project.rs:118-120](../../src/project.rs#L118)).

**Tests.** `tests/project_e2e.rs::removed_images_leave_the_project_and_undo_restores_them`.

**Notes.** If the stash was pruned (another instance, a session longer than a day), the image is re-encoded as PNG instead: the pixels survive, the bytes do not.

## P12 · Share Project writes a .delosproject zip {#p12}

> Share Project exports the current project as a `.delosproject` file: a zip archive containing the project directory.

**Status.** Implemented on desktop. On mobile the menu item is hidden (no pickers, [X1](#x1)).

**Implemented by.**
- `Project::share` zips the directory under one top-level folder ([project.rs:260-268](../../src/project.rs#L260), [project.rs:271-274](../../src/project.rs#L271)).
- The menu item's `share` handler asks for a destination (`pick_save_path`) and calls `share_project` ([mod.rs:491-500](../../src/ui/mod.rs#L491)).

**Tests.** `tests/project.rs::share_and_import_round_trip`, `tests/project_e2e.rs::share_and_import_a_project`.

**Notes.** `share` deflates `project.delos` and stores images as they are (`CompressionMethod`) ([project.rs:269-270](../../src/project.rs#L269)). `Worker::share_project` holds the save lock while packing, so the archive matches a finished save ([worker.rs:385-386](../../src/worker.rs#L385)).

## P13 · Import Project accepts three forms {#p13}

> Import Project accepts a `.delosproject` file, a `project.delos` file or a project directory.

**Status.** Implemented. The menu splits it in two: "Import project…" for files and "Open project folder…" for directories.

**Implemented by.**
- `project::open` dispatches on `ARCHIVE_EXT`, `PROJECT_FILE` or a directory ([project.rs:290-298](../../src/project.rs#L290)).
- Menu items `import_file` and `import_folder` ([mod.rs:535-536](../../src/ui/mod.rs#L535)), built with `open_with`, which runs a picker (`pick_project_file` or `pick_folder`) and then `open_project` ([mod.rs:472-478](../../src/ui/mod.rs#L472), [mod.rs:482](../../src/ui/mod.rs#L482), [mod.rs:487](../../src/ui/mod.rs#L487), [mod.rs:489](../../src/ui/mod.rs#L489), [platform.rs:172-173](../../src/ui/platform.rs#L172), [platform.rs:183](../../src/ui/platform.rs#L183)).
- `on_project_opened` replaces the document with the loaded graph and switches to `Session::Project` ([mod.rs:223](../../src/ui/mod.rs#L223), [session.rs:205-209](../../src/ui/session.rs#L205)).

**Tests.** `tests/project.rs::open_accepts_folder_or_project_file`, `tests/project.rs::share_and_import_round_trip`.

**Notes.** The file picker accepts any `*.delos` file (`pick_project_file`), but `open` accepts only one named exactly `project.delos`, so `other.delos` passes the picker and is then refused.

## P13.1 · Archives unpack under a new name {#p13-1}

> A `.delosproject` file is unpacked into the projects folder under a new name, so it never overwrites an existing project.

**Status.** Implemented.

**Implemented by.**
- `unpack` opens the archive, finds the project inside it (at the root or in one top folder) and names the copy after that folder or the archive's `stem` ([project.rs:306-307](../../src/project.rs#L306), [project.rs:317-327](../../src/project.rs#L317)), then extracts every entry into `unique_dir(root, name)` ([project.rs:328-355](../../src/project.rs#L328)); `unique_dir` appends `-2`, `-3`… ([project.rs:54-64](../../src/project.rs#L54)). `root` is the projects folder (`projects_root`) ([session.rs:197-198](../../src/ui/session.rs#L197)).

**Tests.** `tests/project.rs::share_and_import_round_trip`, `tests/project.rs::import_accepts_archives_without_a_top_folder`.

## P13.2 · Directories open in place {#p13-2}

> A directory is opened in place, without copying.

**Status.** Implemented.

**Implemented by.**
- `open` builds a `Project` with the directory itself as `dir` ([project.rs:294-295](../../src/project.rs#L294)).

**Tests.** `tests/project.rs::open_accepts_folder_or_project_file`.

## P13.3 · Opening by drag and drop {#p13-3}

> Projects can also be opened by dropping them onto the window.

**Status.** Implemented on desktop only (the drop path needs the file's path, a `desktop` feature).

**Implemented by.**
- `Root`'s drop handler finds the first dropped item that `is_project_path` and passes it to `open_project` ([mod.rs:92-99](../../src/ui/mod.rs#L92), [mod.rs:244-249](../../src/ui/mod.rs#L244)).

**Tests.** None.

## P14 · Verify the structure before opening {#p14}

> Before opening, Delos verifies the structure:
> `project.delos` exists and parses;
> `inputs/` exists;
> every image the graph refers to exists inside `inputs/`.
> It reports precisely what is wrong otherwise.

**Status.** Implemented.

**Implemented by.**
- `Project::verify` checks the file, the folder, the parse and each `image_files` path, with a message naming the project, the line and the image ([project.rs:206-225](../../src/project.rs#L206)). `load` calls it first and `unpack` calls it after extracting ([project.rs:231](../../src/project.rs#L231), [project.rs:357](../../src/project.rs#L357)).
- Failures reach the user as a toast (`on_project_opened`) ([session.rs:214](../../src/ui/session.rs#L214)).

**Tests.** `tests/project.rs::verify_rejects_broken_projects`, `tests/project_e2e.rs::invalid_projects_are_rejected_with_a_message`.

**Notes.** Missing-image messages give a line but no column (see [E11](05-expressions.md#e11)). `inputs/` is checked before parsing, the reverse of the spec's order; the outcome is the same.

## P15 · Unsafe paths are rejected; failed imports leave nothing {#p15}

> Paths that would point outside the project, whether in the project file or inside an archive, are rejected.
> A failed import leaves nothing behind.

**Status.** Partial. Unsafe paths are rejected. A failed import leaves nothing behind only when the failure is in extraction or `verify`: if the unpacked project then fails to *load* (an image that does not decode, an unknown node kind), `open` returns the error and the unpacked folder stays in the projects folder ([D-IMPORT-LEFTOVER](findings.md#d-import-leftover)).

**Implemented by.**
- In the project file: `resolve` accepts only `inputs/` followed by normal components ([project.rs:195-204](../../src/project.rs#L195)).
- In an archive: `unpack` checks every entry before extracting and refuses one without an `enclosed_name` ([project.rs:310-316](../../src/project.rs#L310)).
- Cleanup: `unpack` extracts and verifies inside one fallible `result` and removes the destination when extraction or `verify` fails ([project.rs:330](../../src/project.rs#L330), [project.rs:356-363](../../src/project.rs#L356)); an archive without `project.delos` is refused before anything is created ([project.rs:323](../../src/project.rs#L323)).

**Tests.** `tests/project.rs::verify_rejects_broken_projects`, `tests/project.rs::import_rejects_invalid_archives_and_cleans_up`.

**Notes.** The defect: `open` calls `load` after `unpack` returns, with no cleanup ([project.rs:291](../../src/project.rs#L291), [project.rs:299](../../src/project.rs#L299)). Confirmed by a failing probe ([D-IMPORT-LEFTOVER](findings.md#d-import-leftover)).


## X1 · Mobile: projects without pickers {#x1}

> Projects are currently desktop-first:
> Mobile supports projects but has no file pickers yet, so Import and Share Project are unavailable there.

**Status.** Implemented (the limitation is as described).

**Implemented by.**
- `mobile` enables `native`, hence projects ([Cargo.toml:9](../../Cargo.toml#L9)); `HAS_PICKERS` is the `desktop` feature ([platform.rs:164-165](../../src/ui/platform.rs#L164)) and hides Import, Open folder and Share ([mod.rs:534-544](../../src/ui/mod.rs#L534)).

**Tests.** None (no mobile build is tested).

**Notes.** On mobile, exports go to `~/Documents` without asking (`choose_save_target`) ([platform.rs:129-135](../../src/ui/platform.rs#L129)).

## X2 · Web: always quick mode {#x2}

> Web has no file system and always works in quick mode.

**Status.** Implemented.

**Implemented by.**
- `web` does not enable `native` ([Cargo.toml:10-11](../../Cargo.toml#L10)), so `PROJECTS_SUPPORTED` is false ([session.rs:86-87](../../src/ui/session.rs#L86)): the Project menu is hidden ([mod.rs:384-386](../../src/ui/mod.rs#L384)) and the first import starts `Session::Quick` ([session.rs:106](../../src/ui/session.rs#L106), [session.rs:117](../../src/ui/session.rs#L117)).

**Tests.** None (no web build is tested).

## X3 · Platform limits accepted for now {#x3}

> These limitations are accepted for now.
> Full functionality on all platforms is a later goal.

**Status.** Deferred.

**Implemented by.** Nothing; the README lists the same gaps.

**Notes.** The accepted limits are the project ones. The other platform gaps found here ([D-PLAT](findings.md#d-plat)) are not covered by this acceptance.
