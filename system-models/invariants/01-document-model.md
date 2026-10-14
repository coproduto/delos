# 1 · The document model

The document is a `Graph` ([src/graph.rs](../../src/graph.rs)): a `BTreeMap` of
`Node`s, the id of the one output node, a map of decoded images, and an id
counter. Every other part of Delos reads this structure: the engine renders it,
the UI draws and edits it, `expr` serializes it, and the project layer copies
the files it names. These invariants define what a *well-formed* graph is.

All of `Graph`'s collections and `Node`'s fields are `pub`. **No invariant in
this file is protected by the type system.** They hold because every mutation
path goes through a small set of methods (`connect`, `remove_node`,
`set_stack_layers`, `insert_resize`, the `expr` builder) and because the UI
routes edits through `App::edit` (see [U3](05-concurrency-and-ui.md#u3)).

---

## G1 · There is exactly one output node, and it never goes away {#g1}

**Statement.** `graph.output` names a node of kind `output`. It is created first
by `Graph::new`, it cannot be removed, and it has no outputs that can be
connected.

**Maintained by**
- `Graph::new` creates it ([graph.rs:103-107](../../src/graph.rs#L103)).
- `remove_node` returns early for it ([graph.rs:180](../../src/graph.rs#L180)), and so does `App::delete_node` ([state.rs:506](../../src/ui/state.rs#L506)).
- `connect` refuses the output as a source ([graph.rs:249](../../src/graph.rs#L249)).
- `Output::user_creatable() == false` keeps it out of the Add menu ([basic.rs:55](../../src/nodes/basic.rs#L55)), and the expression builder accepts `(output …)` only once and only at the top level ([expr.rs:491-498](../../src/expr.rs#L491), [expr.rs:564](../../src/expr.rs#L564)).

**Depended on by.** `Graph::canvas()` indexes `nodes[&self.output]` without a
check ([graph.rs:302](../../src/graph.rs#L302)). The canvas size is the
extent of every size-less node ([R4](02-geometry-and-rendering.md#r4)). The output
is also the default view target, the root of `expr::to_text`, and the base of
`space_map`.

**If broken.** The canvas lookup panics, and nothing can be displayed or saved.

**Status.** Holds. Enforced by API checks in more than one place.

---

## G2 · The graph is acyclic {#g2}

**Statement.** No node depends on itself, directly or through other nodes.

**Maintained by**
- `Graph::connect` calls `depends_on(from, to)` before writing the edge ([graph.rs:255](../../src/graph.rs#L255)).
- Connecting to a layer-stack port checks `depends_on` by hand before calling `set_stack_layers`, which does no check of its own ([graph_view.rs:121](../../src/ui/graph_view.rs#L121)).
- The inspector lists only input candidates that would not form a cycle ([inspector.rs:76-82](../../src/ui/inspector.rs#L76)).
- The expression builder is acyclic **by construction**: inputs are built before their consumer, and a `def` name is bound only after its expression is built, so a name can refer only to earlier definitions ([expr.rs:633-634](../../src/expr.rs#L633), [expr.rs:488-489](../../src/expr.rs#L488)). It writes `inputs` directly and does not call `connect`.
- `insert_resize` places a new node on an existing edge, so it cannot create a cycle ([graph.rs:367](../../src/graph.rs#L367)).

**Depended on by.** Everything that recurses over inputs:
`size_memo` ([graph.rs:289](../../src/graph.rs#L289)), `hash_of` ([graph.rs:381](../../src/graph.rs#L381)),
`Ctx::render` (pull-based recursion), `space_map_rec`, `expr::to_text`'s DFS
ordering, and the layout pass. `depends_on` itself cannot loop because it keeps
a `seen` set.

**If broken.** Unbounded recursion and a stack overflow in size inference,
hashing and rendering. A size query happens on almost every UI render, so the
app crashes almost at once.

**Tests.** `tests/engine.rs::cycles_are_rejected`.

**Status.** Holds, but only by the discipline of the code that writes edges.
Any new code that writes `node.inputs[...]` directly takes on this proof
obligation.

---

## G3 · Every edge points at an existing output {#g3}

**Statement.** For every `Some(link)` in any `node.inputs`, `link.node` exists
and `link.output < kind(link.node).outputs().len()`.

**Maintained by**
- `connect` validates both ([graph.rs:245-254](../../src/graph.rs#L245)).
- `remove_node` calls `unlink`, which clears every input reading any output of the removed node ([graph.rs:189-197](../../src/graph.rs#L189)). `set_stack_layers` does the same for the stack nodes it drops ([graph.rs:481-484](../../src/graph.rs#L481)).
- The expression builder resolves `name.output` against `outputs()` and reports unknown outputs ([expr.rs:540-555](../../src/expr.rs#L540)).
- Undo and redo swap in a whole snapshot, which was well-formed when it was taken.

**Depended on by.** `Ctx::render` turns a dangling edge into a
`missing node` error. `size_memo` silently returns `None`, so the node would
quietly become size-less. The `expr` printer indexes `g.nodes[&l.node]` and
would panic.

**Status.** Holds. Enforced by the API.

---

## G4 · A node has one input slot per declared port {#g4}

**Statement.** `node.inputs.len() == kind.inputs().len()`.

**Maintained by.** `add_node_at` allocates the vector from the kind
([graph.rs:136](../../src/graph.rs#L136)). The only code that replaces the
vector is for stacks, and it always writes two entries
([graph.rs:489](../../src/graph.rs#L489), [expr.rs:624](../../src/expr.rs#L624)).
The builder rejects too many positional inputs ([expr.rs:629](../../src/expr.rs#L629)).

**Depended on by.** Code that indexes ports without bounds checks:
`Output::size` reads `inputs[0]` ([basic.rs:50](../../src/nodes/basic.rs#L50)),
`stack_chain` reads `inputs[0]` and `inputs[1]` ([graph.rs:451-452](../../src/graph.rs#L451)),
every single-input kind reads `inputs[0]` in `size`, and the graph view zips rows
with `n.inputs[i]` ([graph_view.rs:93](../../src/ui/graph_view.rs#L93)).

**Status.** Holds.

---

## G5 · Node ids and image ids are unique within a graph {#g5}

**Statement.** `fresh_id` hands out increasing ids from `next_id`, shared by
nodes and images ([graph.rs:109-113](../../src/graph.rs#L109)). In one `Graph`
value, no two nodes and no two images share an id.

**Caveat: this is not true across a graph's history.** `next_id` is part of
the `Graph` value. Undo restores an older snapshot together with its older
`next_id`, and `Graph::new` (on closing a session or loading a project) starts
again from 1. **Ids are therefore reused for different content over the life
of a session.** That does no harm inside one graph. It matters wherever an id
outlives the graph that issued it, which is exactly what the structural hash
assumes it never does. See [I1](04-structural-identity.md#i1) and
[finding 1](findings.md#finding-1).

**Depended on by.** Selection, pinning, error maps and the graph view are all
keyed by `NodeId`. `fix_selection` re-validates those keys after undo and redo
([state.rs:286-300](../../src/ui/state.rs#L286)), so they stay correct even when
an id now names a different node.

**Status.** Holds within one graph. Across snapshots it does not, and the
structural hash depends on it doing so.

---

## G6 · Static image nodes and `graph.images` agree {#g6}

**Statement.**
1. Every `image` node's hidden `image` parameter is a key of `graph.images`.
2. Its hidden `width` and `height` parameters equal that image's dimensions.
3. After a `remove_node`, `graph.images` holds only images some node uses.

**Maintained by**
- Imports add the image and the node in one edit and copy the dimensions into the parameters ([ui/mod.rs:255-263](../../src/ui/mod.rs#L255)). The expression builder does the same ([expr.rs:603-616](../../src/expr.rs#L603)).
- The three parameters are `ParamKind::Hidden`, so the inspector never shows them ([basic.rs:78-82](../../src/nodes/basic.rs#L78)). `expr` neither prints nor accepts them (`IMAGE_DERIVED`, [expr.rs:36](../../src/expr.rs#L36), [expr.rs:356](../../src/expr.rs#L356), [expr.rs:658](../../src/expr.rs#L658)). The file is the only source of truth for an image's size.
- `remove_node` runs `gc_images` ([graph.rs:199-203](../../src/graph.rs#L199)).

**Depended on by.** `StaticImage::size` reads the *parameters*, not the image
([basic.rs:84-86](../../src/nodes/basic.rs#L84)). Every size-dependent decision
downstream trusts them: extent, export dimensions, conflict detection and
split regions. `sync_inputs` and `to_text` look images up through the parameter.

**Note.** Part 3 is weaker than it looks. Removing a layer from a stack leaves
its node detached rather than removed, so `images` can hold images whose nodes
feed nothing. That is harmless: `sync_inputs` works from image *nodes*, not from
`graph.images`.

**Status.** Holds. The design avoids the problem by hiding the derived parameters.

---

## G7 · Parameters are complete and correctly typed; ranges are not enforced {#g7}

**Statement.**
1. A node's `Params` holds every key its kind declares, starting from the defaults (`default_params`, [nodes/mod.rs:269-275](../../src/nodes/mod.rs#L269)).
2. Each value has the same `Value` variant as the declared default.
3. A choice value is one of its declared options.
4. **Numeric ranges (`min`/`max`) are UI hints, not invariants.** Code that reads a parameter must tolerate any value.

**Maintained by**
- (1) `add_node_at` fills in the defaults, and the builder only overrides them ([expr.rs:661](../../src/expr.rs#L661)).
- (2) `parse_value` dispatches on the *default's* variant ([expr.rs:703](../../src/expr.rs#L703)). Each inspector field emits the variant that matches its `ParamKind` ([inspector.rs:256-399](../../src/ui/inspector.rs#L256)).
- (3) `parse_value` checks choices against the options ([expr.rs:721-726](../../src/expr.rs#L721)), and the UI offers only the declared options.
- (4) The expression format accepts any number. Consumers clamp instead: octaves ([kernels.rs:818](../../src/engine/kernels.rs#L818)), iterations, opacity, `scale.max(1e-4)` ([resize.rs:33](../../src/nodes/resize.rs#L33)), `factor.max(1.0)`, `canvas_w.max(1.0)`, and the threshold clamp in background removal.

**Depended on by.** Kernel parameter packing (`pack` functions in
[generators.rs](../../src/nodes/generators.rs)), the `expr` round trip
([E1](06-persistence.md#e1)) and inspector rendering.

**Degrades rather than fails.** The accessors are lenient. `Params::f` accepts
`Int` and `Bool`. `choice_index` falls back to index 0 for an unknown choice
([nodes/mod.rs:162-165](../../src/nodes/mod.rs#L162)), so a misspelled blend mode
silently becomes `normal`. Typing errors show up as wrong pictures, never as crashes.

**Status.** (1) to (3) hold. (4) is a deliberate non-invariant, and every new
kind has to follow it by clamping.

---

## G8 · Node kinds are stateless singletons; all per-node state is in `Params` {#g8}

**Statement.** A `NodeKind` is a `'static` value with no per-node state
([nodes/mod.rs:194](../../src/nodes/mod.rs#L194), registry at
[nodes/mod.rs:251-267](../../src/nodes/mod.rs#L251)). A node is fully described
by `(kind id, params, inputs, pos, label)`. Kind ids are unique and stable,
because they are the names written in saved files.

**Maintained by.** The trait shape: `render`, `size` and `input_map` receive the
`Node` and must derive everything from it. Uniqueness of ids is by convention
(`kind()` returns the first match in the registry).

**Depended on by.** This is the root invariant behind three features:
- **Undo** is a `Graph::clone()` ([state.rs:180](../../src/ui/state.rs#L180)). That works only because cloning the nodes captures all their state, and images are shared through `Arc`.
- **Serialization** writes `kind` plus `params` ([expr.rs:332-372](../../src/expr.rs#L332)).
- **Structural hashing** hashes `kind` plus `params` plus inputs ([graph.rs:387-400](../../src/graph.rs#L387)).

`Node::kind()` panics on an unknown id ([graph.rs:48](../../src/graph.rs#L48)).
Loading code checks ids first ([expr.rs:567](../../src/expr.rs#L567)).

**If broken.** A kind that cached per-node state internally, such as an
"AI generator" holding results, would break undo, saving and caching all at
once. The README names the cached-source pattern of background removal
([bgremove.rs](../../src/nodes/bgremove.rs)) as the intended alternative: keep
state in the engine `Cache`, keyed by the structural hash.

**Status.** Holds.

---

## S1 · Layer stacks are a derived view over binary `stack` nodes {#s1}

**Statement.** A layer stack is not stored as such. It is a chain of binary
`stack` nodes, `(front STACK (front₂ STACK … back))`. A stack is *interior*
exactly when its only consumer is the `back` port (1) of another stack
(`is_interior_stack`, [graph.rs:426-432](../../src/graph.rs#L426)). The chain
head is the first stack upward that is not interior (`chain_head`).

**Sub-invariants maintained by `set_stack_layers`** ([graph.rs:470-491](../../src/graph.rs#L470)):
- The chain uses `max(1, layers − 1)` stack nodes. The last node's `back` holds the last layer.
- **The head keeps its id**, so whatever consumes the stack stays connected while layers are added, removed, reordered or disabled.
- Stack nodes that are no longer needed are removed and unlinked.

**Depended on by**
- The graph view hides interior stacks and draws one node with a port per layer ([graph_view.rs:66](../../src/ui/graph_view.rs#L66), [graph_view.rs:75-87](../../src/ui/graph_view.rs#L75)). Layer ports are encoded as `1_000_000 + i` ([graph_view.rs:148-160](../../src/ui/graph_view.rs#L148)).
- The inspector's layer editor ([inspector.rs:404-527](../../src/ui/inspector.rs#L404)).
- `expr` prints a chain of unlabelled interior stacks as one `(stack a b c …)` list ([expr.rs:336-348](../../src/expr.rs#L336), [expr.rs:389](../../src/expr.rs#L389)).

**Consequences of a derived representation**
- Connecting a second consumer to an interior stack silently promotes it to a chain head. The outer chain then shows it as one layer. This is intended.
- `[a, b]` (one stack) and `[a, b, _]` (two stacks with an empty back) are different graphs. The `_` round-trips through `expr`.
- Interior stacks are not written with `:at`, so their graph positions are not preserved across save and load. `set_stack_layers` places them again ([graph.rs:476-479](../../src/graph.rs#L476)). See [E1](06-persistence.md#e1).
- **Disabling a layer** wraps it in a `disable` node, and enabling it again removes that node ([inspector.rs:448-461](../../src/ui/inspector.rs#L448)). The spec's "Disable operator" is therefore a real node, and it appears in saved files and in the hash.

**Tests.** `tests/engine.rs::stack_chain_editing` and `src/expr.rs::stacks_are_lists_of_layers`.

**Status.** Holds.
