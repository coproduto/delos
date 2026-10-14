# 6 · Quality requirements

Requirements on how Delos is developed rather than on what it does. Their
status is `Process`: the matrix can show the tests that exist, not that
the process was followed.

## Q1 · Regression tests for fixed bugs {#q1}

> Bugs that are fixed must get regression tests that reproduce them.

**Status.** Process. Regression tests exist for the UI-state bugs fixed so far. Whether every fixed bug got one cannot be read from the code. The two open defects found by the invariants model have probes but no tests in `tests/`, which is consistent with the rule since they are not fixed.

**Implemented by.**
- The headless UI harness `with_app` runs scenarios against a real `App` ([ui_state.rs:34-41](../../tests/ui_state.rs#L34)).

**Tests.** `tests/ui_state.rs::delete_selected_node_while_output_is_relocated`, `tests/ui_state.rs::delete_relocated_node_resets_output`, `tests/ui_state.rs::delete_selected_edge`, `tests/ui_state.rs::dragging_out_of_the_export_dialog_keeps_it_open`, `tests/export.rs::write_errors_stop_the_export`.

**Notes.** Checking this clause properly needs the bug history (commit messages or an issue tracker) next to the tests; `spectrace impact` on a fix commit lists the entries it touches, which is where its regression test should appear.

## Q2 · Projects: unit and end-to-end tests on the real services {#q2}

> Project features must be covered by unit tests and by end-to-end tests.
> The end-to-end tests run the application's real state, background worker and autosave without a window, and drive them as the user interface does.

**Status.** Process. Both suites exist. Every project clause in [04-sessions-and-projects.md](04-sessions-and-projects.md) except [P3.1](04-sessions-and-projects.md#p3-1), [P6.1](04-sessions-and-projects.md#p6-1), [P7](04-sessions-and-projects.md#p7), [P13.3](04-sessions-and-projects.md#p13-3), [X1](04-sessions-and-projects.md#x1) and [X2](04-sessions-and-projects.md#x2) cites at least one test.

**Implemented by.**
- `use_services` builds the same `App`, `Worker`, message `pump` and `use_autosave` for the window and for tests ([mod.rs:44-46](../../src/ui/mod.rs#L44), [mod.rs:49-54](../../src/ui/mod.rs#L49)); session actions are plain functions of `App` so tests call what buttons call (`session.rs` header) ([session.rs:4-6](../../src/ui/session.rs#L4)).
- The harness `Delos::start` runs a headless `VirtualDom` and drives it with `run` and `until` ([project_e2e.rs:39-77](../../tests/project_e2e.rs#L39)).

**Tests.** `tests/project.rs::create_makes_the_project_layout` (and the rest of `tests/project.rs`), `tests/project_e2e.rs::first_import_asks_and_creates_a_project` (and the rest of `tests/project_e2e.rs`).

## Q3 · 16K performance tests {#q3}

> Performance at 16K resolution is tested: previews must stay interactive, and exports must run in bounded memory.

**Status.** Process. The tests exist and assert time budgets on both backends. Bounded export memory is asserted only for the band size of a generator export, and only PNG is exported at 16K; the formats that buffer the whole image are not tested ([D-EXPORT-MEMORY](findings.md#d-export-memory)).

**Implemented by.**
- `build_graph` makes a 16384² image through invert, reflect, an overlay blend with noise and a stack with a gradient ([large_images.rs:60-69](../../tests/large_images.rs#L60), [large_images.rs:82-102](../../tests/large_images.rs#L82)).
- Previews of 1080² and 1920×1080 must take under 400 ms each (`budget`) ([large_images.rs:106-140](../../tests/large_images.rs#L106)); the band `peak` is asserted in `export_16k_streams_in_bands` ([large_images.rs:183-195](../../tests/large_images.rs#L183)).

**Tests.** `tests/large_images.rs::preview_16k_is_interactive`, `tests/large_images.rs::export_16k_png`, `tests/large_images.rs::export_16k_streams_in_bands`.

**Notes.** `export_16k_png` is ignored in debug builds; run the suite with `--release`.
