# Independent review

After the first pass, a separate reviewer (an agent that saw only the spec,
the matrix and the code) re-read every cited range, status, test and note
in the six clause files and reported 33 problems. Each was re-checked
against the code before being applied. One more claim, that X1 and X2 cite
the wrong `Cargo.toml` lines, was checked by the reviewer itself and
dropped.

| # | Entry | Kind | Problem | Resolution |
|---|---|---|---|---|
| 1 | P15 | Fact | A failed *load* after a successful `unpack` leaves the unpacked folder | Status → Partial; new finding D-IMPORT-LEFTOVER |
| 2 | P14 | Fact | `unpack` calls `verify` after extracting, not first | Reworded |
| 3 | P7 | Fact | On the web the Project menu is hidden, so `confirm` is never reached | Reworded; D-QUICK-EXIT corrected |
| 4 | P13 | Fact | The picker accepts any `*.delos`, `open` only `project.delos` | Note added |
| 5 | F4 | Fact | One Export button per node, exporting the viewed output or output 0 | Reworded |
| 6 | F4.2 | Fact | Threaded export is native only; memory exports hold the whole file | Status and D-EXPORT-MEMORY extended |
| 7 | O4 | Fact | Below factor 2, "box" samples level 0 nearest | Note rewritten |
| 8 | Z1 | Fact | No kind has three inputs; the dialog erases the next conflict | Note rewritten; new finding D-CONFLICT |
| 9 | O8 | Fact | The fallback threshold has a 65,536-pixel floor | Note corrected |
| 10 | D1 | Fact | The AI-hook statement is in README.md, not in `cache.rs` | Attribution corrected |
| 11 | O4 | Status | "Box" is not a box over the factor | Implemented → Partial; folded into D-RESIZE |
| 12 | O8 | Status | Filters coincide only when both axes downscale | "on both axes" added |
| 13 | P6.2 | Status | Opening a generator-only project can autosave a canvas fit | Implemented → Partial; new finding D-OPEN-WRITES |
| 14 | E3 | Status | An unused split is printed without `def` | Exception noted |
| 15 | P10 | Note | Non-regular files are skipped too | Note extended |
| 16 | F4.2 | Range | Close button is at dialogs.rs:194 | Citation added |
| 17 | F1.2 | Range | Canvas drag is at image_view.rs:277-286 | Range extended |
| 18 | F3.1 | Range | The badge's Reset clears `pinned`, does not set it | Reworded |
| 19 | O9 | Range | `set_lum`/`set_sat` start at kernels.rs:468 | Range corrected |
| 20 | S3 | Range | The horizontal split's region (split.rs:50) was missing | Citation added |
| 21 | O0 | Test | `gpu_matches_cpu` does not render every kernel | Test list corrected |
| 22 | O2 | Test | `invert_and_stack` does test Disable | Cited |
| 23 | O4 | Test | No test renders Downscale | "None" |
| 24 | N5, R3 | Test | `kernels_on_cpu_and_gpu` is an ignored benchmark | Removed or qualified |
| 25 | N4 | Test | The cited test does not check the generator's size | "None checks…" |
| 26 | S2 | Test | Registration only, not controls | Qualified |
| 27 | O3 | Test | Replace mode and vertical axis only | Qualified |
| 28 | F2.1 | Test | The cited test never calls `displayed()` | "None for the default view" |
| 29 | F4 | Test | The cited test covers only the relocated case | Qualified |
| 30 | F3.3.1 | Test | `clip_offsets_input` checks pixels, not overlay mapping | Qualified |
| 31 | F3.3.2 | Test | `space_map_follows_geometry_upstream` does assert `None` | Cited |
| 32 | P7 | Test | `has_unsaved_work` is asserted in an end-to-end test | Cited |
| 33 | P11 | Test | Reopening without history is tested elsewhere | Test replaced |

The pattern: the facts about what the code *does* were mostly right; the
mistakes clustered in what a test *covers* (13 of 33) and in edge cases of
a clause (a second failure path, a second platform). Both are cheap to
check and easy to get wrong when writing entries quickly, so the
`spectrace` AGENTS.md makes the review pass part of the procedure.
