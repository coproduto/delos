# 05 · Keyboard

Delos has **one** key handler for the whole window: `onkeydown` on the root
`div.app` ([mod.rs:84](../../src/ui/mod.rs#L84)). The div is focusable
(`tabindex: 0`) and calls `on_key` ([mod.rs:286](../../src/ui/mod.rs#L286)). Keys
reach it by **bubbling** from whatever element has focus. The only element
that stops them is the inspector ([inspector.rs:19](../../src/ui/inspector.rs#L19)),
so typing in a parameter field does not trigger shortcuts.

| Key | Condition | Action |
|---|---|---|
| ⌘Z / Ctrl+Z | | `App::undo` ([state.rs:262](../../src/ui/state.rs#L262)); same as the toolbar's ↶ ([mod.rs:395](../../src/ui/mod.rs#L395)) |
| ⇧⌘Z, ⌘Y | | `App::redo` ([state.rs:274](../../src/ui/state.rs#L274)); same as the toolbar's ↷ ([mod.rs:396](../../src/ui/mod.rs#L396)) |
| ⌘E | something is shown | `App::open_export(None)` → [06-export.md](06-export.md) |
| Delete, Backspace | | `App::delete_selection` ([state.rs:491](../../src/ui/state.rs#L491)): the selected edge, otherwise the selected node (never the Output) |
| Esc | project prompt open and not busy | `session::cancel_prompt`. **Pending imports are discarded.** |
| Esc | export dialog open | closes it, unless an export is running |
| Esc | otherwise | deselects |
| F | no modifier | `view.fit = true` |

## Trace: Delete with a node selected

1. Keydown bubbles to `div.app` → `on_key` → `delete_selection`.
2. It copies `selected_edge` and `selected` out of their signals first ([state.rs:492-495](../../src/ui/state.rs#L492)). The comment there records an earlier crash: a borrow held across the edit panicked with `AlreadyBorrowed` (regression tests in `tests/ui_state.rs`).
3. `delete_node` ([state.rs:505](../../src/ui/state.rs#L505)) is one edit, bridging single-input nodes ([03-inspector.md](03-inspector.md#trace-delete)).
4. ⇢ preview, autosave.

## Trace: ⌘Z

`App::undo` pops a snapshot and swaps it in, then `fix_selection` drops a
selection or relocated output that no longer exists. ⇢ preview, autosave.
In a project, an undo that brings back a removed image makes autosave copy
it back into `inputs/` from the session stash ([07-projects.md](07-projects.md#autosave)).
Undo does not recompute size conflicts ([finding 2](findings.md#finding-2)).

## Keys typed into dialogs reach the shortcuts

The dialogs (`ExportDialog`, `ProjectDialog`, `ConflictDialog`) are rendered
**inside** `div.app` ([mod.rs:119-121](../../src/ui/mod.rs#L119)), not inside the
inspector, and none of them stops `keydown`. From the code, a keystroke in
the export dialog's **File name** field ([dialogs.rs:154](../../src/ui/dialogs.rs#L154))
or the project prompt's **Project name** field ([dialogs.rs:250](../../src/ui/dialogs.rs#L250))
bubbles to `on_key`:

- **Backspace** edits the text *and* deletes the selected node or edge;
- **⌘Z** undoes a graph edit instead of the text edit;
- **F** turns fit mode on.

⌘E with a node selected opens the export dialog *for that node*, so the
selection is very likely to be present while the user edits the file name.
If Backspace deletes the export target, the dialog stays open and the export
fails when it renders ([finding 5](findings.md#finding-5); from code, not
reproduced in a window).

## Decisions

- **Global shortcuts with an opt-out list.** Today the opt-out list has one entry, the inspector. Every new text field outside it inherits the shortcuts. The alternative is to ignore key events whose target is an editable element.
- **Esc in the "Start a project?" prompt discards the imports**, the same as ×. Nothing asks for confirmation.
