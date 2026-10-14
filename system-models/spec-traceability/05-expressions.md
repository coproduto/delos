# 5 · Delos expressions

The spec's *Delos expressions*: the plain-text graph format used as
`project.delos`. Everything lives in `src/expr.rs`: a parser to `Sexp`, a
printer (`to_text`) and a builder (`build`). The unit tests at the end of
that file are cited as `src/expr.rs::name`.

## E1 · A Lisp-like plain-text form of the graph {#e1}

> The node graph has a plain-text representation, so we can speak of Delos *expressions*.
> It is a Lisp-like format, in keeping with layer stacks being treated as non-empty lists:

**Status.** Implemented.

**Implemented by.**
- `parse` reads top-level forms into `Sexp` lists, symbols, strings and numbers ([expr.rs:101-112](../../src/expr.rs#L101)); `Parser::form` ([expr.rs:152-202](../../src/expr.rs#L152)).
- `to_text` prints a graph, and `build`/`from_text` read one ([expr.rs:382-442](../../src/expr.rs#L382), [expr.rs:470-511](../../src/expr.rs#L470)). `Project::save` uses `to_text`, and `Project::load` parses through `verify` and calls `expr::build` ([project.rs:186-189](../../src/project.rs#L186), [project.rs:231](../../src/project.rs#L231), [project.rs:250](../../src/project.rs#L250)).

**Tests.** `src/expr.rs::nested_expression_builds_graph`, `src/expr.rs::shared_nodes_round_trip_through_def`.

**Notes.** The spec's example uses `def` for a node used twice; `to_text` binds a node with `def` when it has two or more consumers or several outputs (`shared`) ([expr.rs:408-426](../../src/expr.rs#L408)).

## E2 · Nodes, inputs in port order, `_` for empty ports {#e2}

> A node is written `(kind input… :param value…)`.
> Inputs come in port order, and `_` leaves a port empty.

**Status.** Implemented.

**Implemented by.**
- Printing: `Printer::node` writes the inputs up to the last connected one, with `_` for empty ports (`reference`) ([expr.rs:352-353](../../src/expr.rs#L352), [expr.rs:317](../../src/expr.rs#L317)).
- Reading: `Builder::fill` separates `:key value` pairs from `positional` inputs, checks the `ports` count and connects each input ([expr.rs:584-602](../../src/expr.rs#L584), [expr.rs:628-637](../../src/expr.rs#L628)); `Builder::input` maps `_` to an empty port ([expr.rs:526](../../src/expr.rs#L526)).

**Tests.** `src/expr.rs::detached_nodes_and_empty_ports_are_kept`, `src/expr.rs::semantic_errors_are_reported`.

## E3 · Several outputs: `def` and `name.output` {#e3}

> Nodes with several outputs (splits) are always bound with `def`, and each output is referred to as `name.output`, e.g. `s.left`.

**Status.** Implemented. One exception the spec does not mention: a split with no consumers is written as a plain detached form, without `def`, since nothing refers to its outputs.

**Implemented by.**
- Printing: nodes with several `outputs` are in `shared` even with one consumer ([expr.rs:408-411](../../src/expr.rs#L408)); `Printer::reference` writes `name.output` ([expr.rs:320-322](../../src/expr.rs#L320)).
- Reading: `Builder::input` passes names to `Builder::reference` ([expr.rs:527](../../src/expr.rs#L527)), which resolves `name.output` and explains the alternatives when it is missing or wrong ([expr.rs:540-555](../../src/expr.rs#L540)); `Builder::input` rejects an inline multi-output node ([expr.rs:531-533](../../src/expr.rs#L531)).

**Tests.** `src/expr.rs::split_outputs_are_referred_to_by_name`, `src/expr.rs::a_split_used_once_is_still_named`, `src/expr.rs::split_reference_errors`, `tests/project.rs::projects_with_splits_round_trip`.

## E4 · `(stack front … back)` {#e4}

> `(stack front … back)` is a layer stack.

**Status.** Implemented.

**Implemented by.**
- Printing: a chain of single-use, untitled stacks (`collapsed`) is flattened into one list ([expr.rs:336-348](../../src/expr.rs#L336), [expr.rs:389](../../src/expr.rs#L389)).
- Reading: `fill` builds the chain with `set_stack_layers` when there are more than two layers ([expr.rs:618-627](../../src/expr.rs#L618)).

**Tests.** `src/expr.rs::stacks_are_lists_of_layers`.

**Notes.** A stack in the middle of a chain that has its own title is printed as a nested `(stack …)` so the title survives.

## E5 · `(image "inputs/…")` {#e5}

> `(image "inputs/…")` is a static image.

**Status.** Implemented.

**Implemented by.**
- Printing: the node's project path from `image_file` ([expr.rs:349-350](../../src/expr.rs#L349), [expr.rs:38-41](../../src/expr.rs#L38)); `to_text` refuses images without a project file ([expr.rs:376-381](../../src/expr.rs#L376)).
- Reading: `fill` asks the caller's `image` function for the decoded image and sets the derived parameters ([expr.rs:603-614](../../src/expr.rs#L603)).

**Tests.** `src/expr.rs::images_need_files_to_serialize`, `src/expr.rs::shared_nodes_round_trip_through_def`.

**Notes.** `fill` also gives the node a default `label`, the file name ([expr.rs:615](../../src/expr.rs#L615)). `image_files` lists the paths so a project can be verified before it is built ([expr.rs:448-466](../../src/expr.rs#L448)).

## E6 · `:at` and `:title` {#e6}

> `:at (x y)` is a node's position in the graph view, and `:title "…"` gives it a custom title.

**Status.** Implemented.

**Implemented by.**
- Printing: `:title` unless it is an image's implicit file-name title, and always `:at` ([expr.rs:364-370](../../src/expr.rs#L364)).
- Reading: `Builder::attribute` handles `at` and `title` ([expr.rs:644-653](../../src/expr.rs#L644)).

**Tests.** `src/expr.rs::every_parameter_is_written_and_titles_kept`.

## E7 · Values and comments {#e7}

> Values are numbers, `true`/`false`, choices as symbols, colors as `(r g b a)` lists and text as strings.
> Comments start with `;`.

**Status.** Implemented.

**Implemented by.**
- Printing: `value_text` ([expr.rs:293-302](../../src/expr.rs#L293)).
- Reading: `parse_value` checks each value against the parameter's default type and choice list ([expr.rs:698-745](../../src/expr.rs#L698)); `skip_space` drops `;` comments ([expr.rs:141-144](../../src/expr.rs#L141)).

**Tests.** `src/expr.rs::all_value_types_round_trip_exactly`, `src/expr.rs::parses_atoms_strings_comments_and_positions`.

**Notes.** Colours may also be written `(r g b)`; alpha then defaults to 1.

## E8 · Write every parameter; read with defaults and auto-layout {#e8}

> When saving, every parameter is written, including defaults, so a project keeps its meaning if a default changes later.
> When reading, omitted parameters take their defaults, and nodes without `:at` are laid out automatically, so expressions can be written by hand.

**Status.** Implemented, with one deliberate exception: a static image's derived parameters (image id, width, height) are not written, because they come from the file (`IMAGE_DERIVED`).

**Implemented by.**
- Printing: `Printer::node` writes every `ParamDef`, falling back to its default ([expr.rs:355-363](../../src/expr.rs#L355)); `IMAGE_DERIVED` ([expr.rs:35-36](../../src/expr.rs#L35)).
- Reading: nodes start from `default_params` (`add_node_at`) ([graph.rs:136](../../src/graph.rs#L136)); `Builder::layout` places nodes without `:at` in columns by depth ([expr.rs:667-695](../../src/expr.rs#L667)).

**Tests.** `src/expr.rs::every_parameter_is_written_and_titles_kept`, `src/expr.rs::nodes_without_positions_are_laid_out`.

## E9 · Fast serialisation {#e9}

> Serializing and deserializing must be fast.

**Status.** Implemented.

**Implemented by.**
- `Doc::width` stops measuring once a list cannot fit, keeping pretty-printing linear ([expr.rs:221-237](../../src/expr.rs#L221)).
- `Project::load` decodes images in parallel before building (`into_par_iter`) ([project.rs:235-248](../../src/project.rs#L235)).

**Tests.** `tests/project.rs::large_graphs_serialize_quickly`.

**Notes.** "Fast" has no number in the spec. The README reports 5 ms to serialise and 16 ms to load a 3,400-node graph.

## E10 · Round trip gives a functionally identical graph {#e10}

> Doing both must give a functionally identical graph: the same nodes, connections, parameters, titles and positions, rendering the same pixels.

**Status.** Implemented.

**Implemented by.**
- The printer and builder above; node ids are renumbered on load, which "functionally identical" allows.
- Positions are rounded to whole units when printed (`:at`) ([expr.rs:370](../../src/expr.rs#L370)).

**Tests.** `tests/project.rs::save_and_load_give_the_same_graph` (compares pixels), `tests/project.rs::projects_with_splits_round_trip`, `tests/project_e2e.rs::edits_are_autosaved_and_reopen_identically`, `src/expr.rs::all_value_types_round_trip_exactly`.

**Notes.** Floating-point parameters print with Rust's shortest round-trip form, so they read back bit for bit (the value test checks `0.3333333333333333`).

## E11 · Errors with line and column {#e11}

> Errors are reported with a line and column.

**Status.** Partial. Syntax and semantic errors of the expression carry a line and a column. When a project is verified, a missing or unsafe image path is reported with its line only ([D-ERROR-COLUMN](findings.md#d-error-column)).

**Implemented by.**
- `ExprError` with `line` and `col`, displayed as `line L:C: message` ([expr.rs:48-65](../../src/expr.rs#L48)); every `Sexp` keeps its `Pos` ([expr.rs:70-90](../../src/expr.rs#L70)).
- `Project::verify` formats only `pos.line` for image errors ([project.rs:218-223](../../src/project.rs#L218)).

**Tests.** `src/expr.rs::syntax_errors_report_positions`, `src/expr.rs::semantic_errors_are_reported`, `tests/project.rs::verify_rejects_broken_projects`.
