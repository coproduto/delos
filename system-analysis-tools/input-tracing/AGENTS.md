# inputtrace: instructions for agents

An **input-trace model** explains a system by following every input from
the moment it enters to where it ends: a changed document, a render, a
file, a message on screen. Your job is to make the traces accurate and
complete enough that a human can use them to decide things: what an input
changes, which steps are undoable or saved, where validation happens, and
where work crosses a thread. The human makes the decisions.

- Concepts, output reading guide and limitations: [README.md](README.md).
- Every option, schema, exit code and matching rule: [REFERENCE.md](REFERENCE.md).

Set these once. Run every command from the project root.

```sh
T=system-analysis-tools/input-tracing/inputtrace
M=system-models/input-tracing                      # the model directory
```

Global options (`--src`, `--receiver`) go **before** the subcommand:
`$T --src crates/app/src inventory .`.

## Procedure: build a model

1. **Inventory.**
   `$T inventory . --md $M/inventory.md --json $M/inventory.json`.
   A root or `--src` that does not exist stops with
   `source directory not found` (exit 2). If the first line of
   `inventory.md` says `0 entry points`, `--src` points at the wrong
   directory. Read all of it. Group the entries by
   *where the user is* (a toolbar, a view, a dialog, the keyboard), not by
   file. Each group becomes one trace document. Non-UI kinds (`env`, `fs`,
   `dialog`, `webview`) usually join the group whose handler leads to
   them, plus one "environment and platform" document.
2. **Find the spine first.** `$T signals .` and `$T channels .` show the
   shared machinery: which signals rerun which effects, who writes each
   signal, and which messages come back from where. Write `00-spine.md`
   before any trace. If you skip it, every trace repeats it.
3. **Trace one representative input per group** with
   `$T trace . FILE:LINE` (the `Where` column of the inventory). A
   `FILE:LINE` inside no entry's span is an error, and an argument that
   matches several entries or functions is an error that lists them (see
   [REFERENCE.md](REFERENCE.md#how-trace-resolves-its-entry-argument)).
   Then **read the code of every branch** the tree passes through. The tree
   over-approximates (it ignores `cfg`, branches and early returns) and
   under-approximates (it drops ambiguous calls). It is a map, not the
   trace. Write numbered steps that cite `path:line`. Mark
   the hops a call graph does not show: ⇢ effect, ⇢ thread/task,
   ⇢ `Msg::X`.
   - Too much noise: `--collapse QUAL` (repeatable) or `--stop REGEX`.
   - Nodes ending in `…` hit the depth limit (`--depth`, default 6). Trace
     them on their own: `$T trace . Type::name`.
   - `(above)` means the node was expanded earlier in the same tree.
4. **Cover the rest of the group** with a table of entries, each with a
   one-line note on how it differs from the representative.
5. **End each document with Decisions**: choices visible in the trace that
   a human should own (what is undoable, what is saved, what is validated
   where, what is discarded silently).
6. **Check coverage.** `$T coverage . $M` must exit 0. It fails (exit 1)
   when an entry is uncited or when there are no entries to check, and a
   missing root or `--src` directory is an error (exit 2). It checks `handler`,
   `effect`, `env`, `dialog` and `webview` entries. Add `--kind fs` if
   file reads should be cited too.
7. **Confirm suspicions with failing tests.** Every claim of a bug needs a
   probe in `$M/probes/` that replays *exactly* the calls the input path
   makes (cite the handler lines in a comment). Copy it into `tests/`, run
   it, record the result, and **remove the copy**. Write the probe to
   expect the correct behaviour, so it fails while the bug exists. If you
   cannot drive the path headlessly (DOM bubbling, native dialogs), say
   "from code, not reproduced" instead.

## Procedure: update a model after code changes

1. Regenerate the inventory (step 1 above) and `git diff $M/inventory.md`.
   New rows are new inputs. Changed `Calls` or `Writes` cells are changed
   behaviour.
2. Run `$T coverage . $M`. For each `uncited` line, either fix a stale
   citation (the code moved) or trace the new input into the right
   document.
3. Re-run `$T signals .`. A new writer of a document signal is the most
   important change to check (see below).

## What to look for (it found all of Delos's findings)

- **Every writer of the document state.** `$T signals .` lists who writes
  each signal. A writer outside the official edit function (`App::edit`
  here) is a bypass. Check whether it skips undo, post-edit checks or
  persistence. Make a matrix: path × {snapshot, clears redo, post-edit
  checks, effects}. A write inside a handler or effect is listed under
  that entry's id only. A component is listed only for writes in its own
  body, outside its handlers and effects.
- **Effects that write state.** An effect that writes the *document* is an
  input with no user. Ask when it runs: on open, on resize, on toggles.
- **Gesture coalescing.** Continuous gestures (drags, sliders) must share
  one undo step. Check each gesture's key, and especially gestures that set
  two values per event.
- **Undo/redo vs. derived state.** Anything computed after an edit
  (conflicts, warnings) must be recomputed after undo and redo too.
- **Event propagation.** One global key handler plus opt-outs means every
  new text field outside the opt-outs inherits the shortcuts.
- **Validation per path.** For one parameter, list every way a value can
  enter (slider, typed number, on-canvas drag, file). Compare the checks.

## Pitfalls

- **Do not invent examples.** Name only node kinds, parameters and buttons
  that exist. Grep before you write "Gaussian Blur".
- **Write citations `coverage` can see.** Either `src/ui/mod.rs:84` (or
  `:84-90`) written out, or a link whose text ends in the line:
  `[mod.rs:84](../../src/ui/mod.rs#L84)`. The path is the file's path
  from the project root (`crates/app/src/main.rs:10` under another
  `--src`), optionally behind `../`. A bare `mod.rs:84`, or a link like
  `[the handler](../../src/ui/mod.rs#L84)`, does not count.
- **Files named `inventory*.md` never count** as citations, because they
  cite everything.
- **A missing edge is not proof of absence.** Calls through unmapped
  receivers with common names (`get`, `set`, `run`, `new`, …) and trait
  objects are dropped. Grep before you write "never
  calls".
- **Channel edges assume one enum.** Every `Msg::X` in an arm pattern is a
  receive (`Ok(Msg::X)`, `Msg::A | Msg::B` included). Messages of a second
  enum, or sent through a helper that hides the variant, are not seen.
  Read the receiving function.
- **A probe that passes may be testing the wrong thing.** Before
  concluding "not a bug", check that the assertion distinguishes the two
  behaviours.
- **Leave `src/` and `tests/` unchanged.** Probes live in the model
  directory.
