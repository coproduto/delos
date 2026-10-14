# 07 · Projects and sessions

Projects are where input goes in **both directions**. The user chooses
folders and archives, and the program then reads **files that someone else
may have written**: `project.delos`, the images in `inputs/`, and
`.delosproject` zip archives. Projects exist on desktop and mobile only
(`PROJECTS_SUPPORTED`, [session.rs:87](../../src/ui/session.rs#L87)). The web
build is always in quick mode.

A session is in one of three states ([session.rs:15](../../src/ui/session.rs#L15)):
**Fresh** (nothing imported), **Quick** (edit in place, nothing saved) or
**Project(dir)** (every change autosaved).

| Input | Entry |
|---|---|
| **Project ▾** button / backdrop | [mod.rs:521](../../src/ui/mod.rs#L521), [mod.rs:523](../../src/ui/mod.rs#L523) |
| New project from current work… / Save as new project… | [mod.rs:528](../../src/ui/mod.rs#L528) |
| Import project… (`.delosproject` or `project.delos`) | [mod.rs:535](../../src/ui/mod.rs#L535) → picker [platform.rs:169-176](../../src/ui/platform.rs#L169) |
| Open project folder… | [mod.rs:536](../../src/ui/mod.rs#L536) → picker [platform.rs:180-189](../../src/ui/platform.rs#L180) |
| Share project… | [mod.rs:541](../../src/ui/mod.rs#L541) → save dialog [platform.rs:194](../../src/ui/platform.rs#L194), [platform.rs:202-214](../../src/ui/platform.rs#L202) |
| Close project / End quick session | [mod.rs:547](../../src/ui/mod.rs#L547) |
| "Discard quick-mode work?" confirmation | [platform.rs:219-235](../../src/ui/platform.rs#L219) |
| Drop a project on the window | [mod.rs:86](../../src/ui/mod.rs#L86) |
| <a id="prompt"></a>Project prompt: name, Change… (location), Quick mode, Create project, Cancel / × / Esc | [dialogs.rs:250](../../src/ui/dialogs.rs#L250), [dialogs.rs:261](../../src/ui/dialogs.rs#L261), [dialogs.rs:286](../../src/ui/dialogs.rs#L286), [dialogs.rs:295](../../src/ui/dialogs.rs#L295), [dialogs.rs:290](../../src/ui/dialogs.rs#L290), [dialogs.rs:226](../../src/ui/dialogs.rs#L226) |
| Autosave (an effect; every graph or session change counts as input) | [session.rs:251](../../src/ui/session.rs#L251) |
| Files read: `project.delos`, `inputs/*`, archives | [project.rs:216](../../src/project.rs#L216), [project.rs:239](../../src/project.rs#L239), [project.rs:306](../../src/project.rs#L306), [worker.rs:368](../../src/worker.rs#L368) |

## Trace: first import → "Create project"

(The import half is in [01-images-in.md](01-images-in.md).)

1. The prompt shows the suggested name and `~/.delos/projects` (or `$DELOS_PROJECTS_DIR`, [project.rs:39](../../src/project.rs#L39)). Typing in the name field calls `set_prompt_name` and clears any error. **Change…** opens a native folder picker and sets the parent folder.
2. **Create project** → `session::create_project` ([session.rs:154](../../src/ui/session.rs#L154)):
   - `dir = parent / sanitize_name(name)` ([project.rs:48](../../src/project.rs#L48)). A non-empty existing folder puts an error in the prompt and stops here.
   - `project_busy = "Creating project …"` disables the menu and the prompt.
   - ⇢ thread `Project::create` ([project.rs:69](../../src/project.rs#L69)) checks again, makes `inputs/` and writes an empty `project.delos`.
3. ⇢ `Msg::ProjectCreated` → `on_project_created` ([session.rs:168](../../src/ui/session.rs#L168)): session becomes `Project`, `save_state = Idle`, toast, then **the pending imports start** (with `keep_bytes`).
4. ⇢ effect **autosave** (session changed) saves the current graph. When each import lands (`finish_import` edits the graph), autosave runs again and copies the image in.

**New project from current work** ([mod.rs:528](../../src/ui/mod.rs#L528) → `prompt_new_project`, [session.rs:134](../../src/ui/session.rs#L134))
is the same prompt with no pending imports. The first autosave copies
every image the graph uses into the new project. Opened from inside a
project, it becomes "Save as new project": the session moves to the new
folder and the old project stops receiving saves.

<a id="autosave"></a>
## Trace: autosave after any edit

1. The effect ([session.rs:251-269](../../src/ui/session.rs#L251)) reads `session` and `graph`. It skips the first run after a project switch if the project is already `Saved` (just opened). Otherwise it bumps a local sequence number, sets `save_state = Saving` (footer: "Saving…") and calls `Worker::save_project(project, graph snapshot, app.all_images(), seq)`. `all_images` includes every image held by the undo and redo history ([state.rs:230](../../src/ui/state.rs#L230)).
2. ⇢ thread ([worker.rs:356-378](../../src/worker.rs#L356)): takes the `saved_seq` lock (saves run one at a time, and a save older than one already written is skipped), then:
   - `Project::sync_inputs` ([project.rs:141](../../src/project.rs#L141)): every image the graph **uses** but the project lacks is copied in. The copy comes from its origin file, byte for byte, or is re-encoded as PNG when there is no origin (`adopt_image`, `add_input`; identical bytes under the same name are reused). Every file in `inputs/` the graph **no longer uses** is **moved** to the session stash, and images still held by the history are repointed at the stashed copy so an undo can bring them back.
   - `expr::to_text` serialises the graph ([expr.rs:376](../../src/expr.rs#L376)). The file is written atomically (temporary file, then rename, [project.rs:392](../../src/project.rs#L392)), and only if the text differs from what is on disk.
3. ⇢ `Msg::ProjectSaved` → footer "All changes saved", or "Not saved: …" ([mod.rs:225-231](../../src/ui/mod.rs#L225)).

Every graph write triggers this, including each pointer move of a node
drag, slider drag or overlay drag ([observations](findings.md#observations)).

<a id="open"></a>
## Trace: Import project… → `trip.delosproject`

1. **Pick** ([mod.rs:472-489](../../src/ui/mod.rs#L472)): `open_with` ⇢ task: native file picker filtered to `.delosproject` and `.delos`. If the session is Quick with work in it (`has_unsaved_work`, [session.rs:243](../../src/ui/session.rs#L243)), a native **confirm** comes next. Cancel stops here.
2. `session::open_project` ([session.rs:194](../../src/ui/session.rs#L194)) sets `project_busy` ⇢ thread `project::open(path, projects_root)` ([project.rs:289](../../src/project.rs#L289)):
   - **archive** → `unpack` ([project.rs:305](../../src/project.rs#L305)): every entry must have an `enclosed_name`, so no `..` or absolute paths. The archive must contain a `project.delos`, either at its root or inside one top folder. It is extracted into a **new** unique folder under the projects root, then `verify`ed. On any failure the folder is deleted.
   - **`project.delos`** → its parent folder, **in place**.
   - **folder** → in place.
3. `Project::load` ([project.rs:229](../../src/project.rs#L229)) → `verify` ([project.rs:208](../../src/project.rs#L208)): `project.delos` and `inputs/` exist. The text **parses** (errors carry `line:col`, `expr::parse`). Every `(image "…")` path stays inside `inputs/` (`resolve`, [project.rs:196](../../src/project.rs#L196)) and exists. The images are decoded **in parallel**, then `expr::build` ([expr.rs:470](../../src/expr.rs#L470)) checks node kinds, parameter names and types, choice values, `def` names and output references, and lays out nodes that have no `:at`.
4. Each image is made GPU-resident ⇢ `Msg::ProjectOpened`.
5. `on_project_opened` ([session.rs:202](../../src/ui/session.rs#L202)): `replace_document(graph)` (new history, selection, view, `canvas_user_set = false`), session = Project, **save_state = Saved**, toast. On failure: toast "Could not open the project: …", and the current session is untouched.
6. ⇢ effects: autosave sees "just opened, already saved" and writes nothing. The preview renders. If the output is size-less, the **canvas effect rewrites the canvas size, and autosave then writes the project** ([finding 3](findings.md#finding-3)).

**Open project folder…** and **dropping** a project
([mod.rs:93-100](../../src/ui/mod.rs#L93)) take the same path from step 2.

## Trace: Share project…

[mod.rs:491-503](../../src/ui/mod.rs#L491): a save dialog with the default name
`<project>.delosproject` ⇢ `share_project` → `project_busy = "Packing
project…"` ⇢ thread ([worker.rs:381](../../src/worker.rs#L381)). The thread waits on
the `saved_seq` lock so it never packs a half-written save, then
`Project::share` ([project.rs:255](../../src/project.rs#L255)) zips the folder into a
temporary file: `project.delos` deflated, images stored, dotfiles and
`.tmp` files skipped. It renames the file into place ⇢ `Msg::ProjectShared`
→ toast.

## Trace: Close project / End quick session

[mod.rs:509-517](../../src/ui/mod.rs#L509): unsaved quick work needs a native
confirm. `close_session` ([session.rs:219](../../src/ui/session.rs#L219)) replaces
the document with an empty graph and sets the session to `Fresh` and
`save_state` to `Idle`. A project needs no confirmation because it is
already saved. **On mobile `confirm` always answers yes** ([platform.rs:231-235](../../src/ui/platform.rs#L231)),
so ending a quick session there discards the work without asking.

## Decisions

- **In place vs. copy.** Folders and `project.delos` files open **in place**, so later edits autosave into them, even when someone else sent them. Only archives are copied into the projects folder.
- **Files from other people are input.** `verify` and `build` check structure, paths, kinds, types and choices. Parameter **ranges are not checked**, and image decoding runs with **no size limits** ([formats.rs:75](../../src/formats.rs#L75)). The comment there explains why: 16K images are normal. Zip extraction has no size limit either.
- **Autosave granularity** is "every graph write". Saving is cheap but not free: each save runs `sync_inputs`, which reads the `inputs/` folder, plus a serialisation and a file comparison.
- **The undo history outlives files.** Images removed from the project live in the session stash in `/tmp` until another session cleans up stashes older than a day.
