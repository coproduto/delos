# 06 · Export

Export is the one path that renders at **full resolution**. It is also the
only path where the user chooses a destination outside the program.

| Input | Entry |
|---|---|
| Toolbar **Export** / ⌘E: what the view shows | [mod.rs:391](../../src/ui/mod.rs#L391), [mod.rs:294](../../src/ui/mod.rs#L294) |
| Inspector **Export…**: this node (the viewed output of a split) | [inspector.rs:154](../../src/ui/inspector.rs#L154) |
| Format `<select>` | [dialogs.rs:133](../../src/ui/dialogs.rs#L133) |
| File name | [dialogs.rs:154](../../src/ui/dialogs.rs#L154) |
| JPEG quality | [dialogs.rs:166](../../src/ui/dialogs.rs#L166) |
| **Export…** (starts) | [dialogs.rs:193](../../src/ui/dialogs.rs#L193) → `start` [dialogs.rs:80](../../src/ui/dialogs.rs#L80) |
| Save dialog (desktop) | [platform.rs:120-128](../../src/ui/platform.rs#L120) |
| **Cancel export** | [dialogs.rs:184](../../src/ui/dialogs.rs#L184) |
| Close: ×, Close, Esc, click on the backdrop | [dialogs.rs:125](../../src/ui/dialogs.rs#L125), [dialogs.rs:194](../../src/ui/dialogs.rs#L194), [dialogs.rs:114](../../src/ui/dialogs.rs#L114), [dialogs.rs:115](../../src/ui/dialogs.rs#L115) |
| Presses inside the box (not on the backdrop) | [dialogs.rs:117](../../src/ui/dialogs.rs#L117), [dialogs.rs:121](../../src/ui/dialogs.rs#L121) |

## Trace: ⌘E → PNG → Export… → choose a file

1. **Open.** `App::open_export(None)` ([state.rs:392](../../src/ui/state.rs#L392)) asks `displayed()` for its `export_target`: the node being edited, or the shown output when the view is locked. It records the title, the **extent in pixels** (a size-less target uses the canvas) and a file name stem. It sets `App.export`, and the `ExportDialog` renders ([dialogs.rs:69](../../src/ui/dialogs.rs#L69)).
2. **Settings.** The format, file name and quality inputs write only `App.export`, never the graph. Changing the format rewrites the file name's extension.
3. **Start** ([dialogs.rs:80-109](../../src/ui/dialogs.rs#L80)) ⇢ task:
   - `platform::choose_save_target` ([platform.rs:120](../../src/ui/platform.rs#L120)): on desktop a native save dialog (cancel = nothing happens); on mobile `~/Documents/<name>`, with no dialog; on the web, in memory.
   - It takes a fresh export id, marks `running` and **snapshots the graph** (`app.graph.peek().clone()`). Edits made after this point do not affect the file being written.
   - `Worker::export` ([worker.rs:280](../../src/worker.rs#L280)) ⇢ thread with a cancel flag.
4. **Render and encode** ([worker.rs:285-319](../../src/worker.rs#L285), [formats.rs:259](../../src/formats.rs#L259)):
   - The file is created at the destination path.
   - On native, a scoped thread renders **bands** of about 4 Mpx each (`GraphBands`, [formats.rs:208](../../src/formats.rs#L208)) with `req.exact = true` through `render_to_cpu`. A one-band channel lets encoding overlap with rendering the next band. PNG streams. The other codecs collect the bands first.
   - Progress per band ⇢ `Msg::ExportProgress`. Node status texts (such as the background-removal step) ⇢ the same message with `fraction = -1`.
   - Any error, including cancellation, **deletes the partial file** ([worker.rs:304-307](../../src/worker.rs#L304)).
   - ⇢ `Msg::ExportDone`.
5. **Finish** ([mod.rs:199-219](../../src/ui/mod.rs#L199)): `Saved(path)` closes the dialog and shows the toast "Saved to …". `Bytes` (web) goes to `deliver_bytes` → browser download ([platform.rs:144](../../src/ui/platform.rs#L144)). An error keeps the dialog open with the message. "Export cancelled" is the cancel case.

## Trace: Cancel export

[dialogs.rs:184-189](../../src/ui/dialogs.rs#L184) → `Worker::cancel_export(id)`
sets the flag ([worker.rs:392](../../src/worker.rs#L392)). The render thread sees it
at its next node or kernel, the band iterator stops, the encoder fails, and
the partial file is removed. The `ExportDone` error closes nothing. The
dialog shows "Export cancelled".

## Trace: closing by clicking outside

`onmousedown` on the backdrop vs. the box records where the press started
(`export_pressed`, [state.rs:423](../../src/ui/state.rs#L423)). `onclick` on the
backdrop closes the dialog only if the press also started there
(`export_backdrop_clicked`, [state.rs:436](../../src/ui/state.rs#L436)). This
fixes a reported bug: selecting the file name by dragging past the dialog's
edge used to close it (regression test
`dragging_out_of_the_export_dialog_keeps_it_open`).

## Decisions

- **What "Export" exports** follows the view rules (`export_target`). In locked mode it is the output, even though a different node is being edited.
- **Snapshot at start.** The file reflects the document when Export… was pressed, so edits made during a long export are not included.
- **Mobile writes without asking**, to `~/Documents`, and overwrites a file with the same name.
- The dialog's text field is not shielded from global shortcuts ([finding 5](findings.md#finding-5)).
