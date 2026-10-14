# refgraph reference

The exact behaviour of every command, option and file format. For concepts
and workflows, see [`README.md`](README.md). For the agent procedure, see
[`AGENTS.md`](AGENTS.md).

Commands below use `T=system-analysis-tools/code-clusters/refgraph`, run from
the project root.

- [Invocation](#invocation)
- [Conventions shared by all commands](#conventions-shared-by-all-commands)
- Commands: [`index`](#index) · [`graph`](#graph) · [`sweep`](#sweep) ·
  [`cluster`](#cluster) · [`report`](#report) · [`names`](#names) ·
  [`where`](#where) · [`analyze`](#analyze)
- [Exit codes](#exit-codes)
- File formats: [`index.scip`](#indexscip-input) · [`graph.json`](#graphjson) ·
  [`sweep.json`](#sweepjson) · [`clusters.json`](#clustersjson) ·
  [`names.json`](#namesjson) · [`report.md`](#reportmd)
- [Environment and configuration](#environment-and-configuration)

## Invocation

```text
refgraph [-h] {index,graph,sweep,cluster,report,names,where,analyze} ...
```

| Form | Requires |
|---|---|
| `system-analysis-tools/code-clusters/refgraph …` | `uv` on `PATH`. The shebang is `#!/usr/bin/env -S uv run --quiet --script`; uv provides Python ≥ 3.10 and networkx ≥ 3.2. |
| `python3 system-analysis-tools/code-clusters/refgraph …` | Python ≥ 3.10. networkx only for `sweep`, `cluster`, `analyze`. |

A subcommand is required. `refgraph` alone prints
`refgraph: error: the following arguments are required: cmd` and exits 2.
Every command accepts `-h/--help`.

## Conventions shared by all commands

- **Data** goes to stdout or to the `-o` file. **Progress, warnings and
  errors** go to stderr.
- When a command writes a file it logs `wrote PATH` on stderr. When the
  output is `-`, it writes to stdout and logs nothing.
- JSON is written with `indent=1` and a trailing newline.
- Existing output files are overwritten without warning.
- Paths are used as given, relative to the current directory.
- Input JSON files are not validated. Their `format` field is never
  checked; a file only needs the keys the command reads.
- Options that take one or more values (`--features`, `-r/--resolutions`)
  consume every following word up to the next option. Put positional
  arguments before them.
- Options marked *repeatable* use argparse `append`: give the option once
  per value (`--tests 'a/*' --tests 'b/*'`).
- Globs are Python `fnmatch` patterns matched against the SCIP document
  path (relative to the indexed root, e.g. `src/engine/gpu.rs`). `*`
  matches `/` too. Matching is case-sensitive on macOS and Linux.

## `index`

Run rust-analyzer to produce a SCIP index.

```text
refgraph index [-h] [-o OUTPUT] [--features F [F ...]] [--all-features]
               [--no-default-features] [-v] [root]
```

| Argument | Type | Default | Repeatable | Meaning |
|---|---|---|---|---|
| `root` | path | `.` | no | Cargo project or workspace root. Must contain `Cargo.toml`. |
| `-o`, `--output` | path | `index.scip` | no | Where rust-analyzer writes the index. |
| `--features F [F ...]` | one or more strings | none | no (takes a list) | Cargo features to enable. |
| `--all-features` | flag | off | no | Enable all features. Takes precedence over `--features`. |
| `--no-default-features` | flag | off | no | Disable default features. |
| `-v`, `--verbose` | flag | off | no | Print rust-analyzer's output even on success. |

Behaviour:

1. If `ROOT/Cargo.toml` does not exist, prints a message naming other
   SCIP indexers and exits 2.
2. If `rust-analyzer` is not on `PATH`, exits 2.
3. Runs `rust-analyzer scip ABS_ROOT --output ABS_OUTPUT`. When any feature
   option is given, it writes a temporary JSON file
   `{"cargo": {"features": [...] | "all", "noDefaultFeatures": true}}`
   (only the keys that apply), passes it with `--config-path`, and deletes
   it afterwards.
4. rust-analyzer's stdout and stderr are captured together. They are
   printed on stderr if `-v` is given or rust-analyzer fails.

| Stream | Content |
|---|---|
| stdout | nothing |
| stderr | `running: <full rust-analyzer command>`; rust-analyzer output (see above); `wrote OUTPUT` on success |
| files | `OUTPUT` (written by rust-analyzer) |

Exit: 0 on success; 2 for no `Cargo.toml` or no rust-analyzer;
rust-analyzer's own exit code if it fails.

## `graph`

Build `graph.json` from a SCIP index.

```text
refgraph graph [-h] [-o OUTPUT] [--granularity {item,type,file}]
               [--tests GLOB] [--exclude GLOB] [--exclude-symbols REGEX]
               [--package NAME] [--owner-weight OWNER_WEIGHT]
               [--impl-weight IMPL_WEIGHT] index
```

| Argument | Type | Default | Repeatable | Meaning |
|---|---|---|---|---|
| `index` | path | required | no | SCIP index to read. |
| `-o`, `--output` | path or `-` | `graph.json` | no | Output file; `-` for stdout. |
| `--granularity` | `item` \| `type` \| `file` | `item` | no | Node unit. `type`: methods are merged into their owning type. `file`: one node per file. |
| `--tests GLOB` | glob | see below | yes | Documents treated as tests. Their definitions are not nodes; their references to nodes are recorded under `tests`. **Giving the option replaces the default list.** |
| `--exclude GLOB` | glob | none | yes | Documents ignored entirely (not nodes, not tests, not references). |
| `--exclude-symbols REGEX` | Python regex | `(^|/)tests?(/|::|$)` | no | Items whose key matches (`re.search`) are dropped, and references inside them are not attributed to anything. `''` disables. The key includes the `package:` prefix when present. |
| `--package NAME` | string | all packages that define symbols | yes | Restrict the project to packages with this *name* (the second field of the SCIP package, e.g. `delos`). |
| `--owner-weight` | int | `2` | no | Weight added for each method → owning type edge. `0` disables. |
| `--impl-weight` | int | `1` | no | Weight added for each trait-impl method → trait method declaration edge. `0` disables. |

Default `--tests` globs: `tests/*`, `*/tests/*`, `test/*`, `*/test/*`,
`*_test.*`, `test_*.*`, `*/test_*.*`, `*.test.*`, `*.spec.*`.

| Stream | Content |
|---|---|
| stdout | the graph JSON when `-o -` |
| stderr | optional warning `warning: N definitions had no enclosing range and were skipped; the indexer may not emit enclosing_range (references cannot be attributed without it)`; then `N nodes, M edges, R references; packages: P, …` where `M` counts directed edges and `R` is the sum of edge weights; then `wrote OUTPUT` |
| files | `OUTPUT` ([`graph.json`](#graphjson)) |

Exit: 0, including for an empty graph. 2 with a one-line message for a
missing or unreadable index (`refgraph: PATH: No such file or directory`)
or an invalid regex (`refgraph: invalid --exclude-symbols regex 'R': …`).
1 on a malformed index (Python traceback).

How the graph is built:

1. Documents matching `--exclude` are dropped.
2. **Project packages** are the packages of every symbol that has a
   definition occurrence in the remaining documents, filtered by
   `--package`. If there is more than one package name, every key gets a
   `NAME:` prefix.
3. Each symbol of a project package is mapped to an item (see
   [Symbol → item mapping](#symbol--item-mapping)). Other symbols, locals,
   and symbols that fail to parse are ignored silently.
4. Each definition occurrence of an item with an `enclosing_range` is an
   item body. Definitions without one are counted for the warning.
5. Each non-definition occurrence is attributed to the innermost body
   that contains it. If that body's item is excluded, or none contains
   it, the reference is dropped. In a test document, the reference is
   added to `tests[document][target]`; otherwise it adds 1 to the edge
   `source → target`, unless source and target are the same.
6. Structural edges: each method adds `--owner-weight` to
   `method → owner`. The owner key is used if it is a node; otherwise the
   unique node with the same simple name; otherwise the edge (and the
   node's `owner`) is dropped. Each trait-impl method of trait `T` adds
   `--impl-weight` to `method → K` for every other node `K` whose last
   component is `T::method`.
7. Edges and test references to keys that are not nodes are dropped.
8. For `type` and `file` granularity, nodes and edges are merged (weights
   summed, internal edges dropped).

### Symbol → item mapping

A SCIP symbol is `<scheme> <manager> <package> <version> <descriptors>`
(a space inside a field is written as two spaces) or `local <id>`.
Descriptor suffixes: `/` namespace, `#` type, `.` term, `().` method,
`[x]` type parameter, `(x)` parameter, `:` meta, `!` macro. Names may be
backtick-quoted.

Namespaces become the key prefix `a/b/`. Then, by the first non-namespace
descriptor:

| Descriptors (after namespaces) | Item key | `owner` | `trait` | Folded? |
|---|---|---|---|---|
| `Type#` | `a/b/Type` | – | – | exact |
| `Type#method().` | `a/b/Type::method` | `a/b/Type` | – | exact |
| `Type#field.`, `Type#Variant.`, anything else under a type | `a/b/Type` | – | – | folded into the type |
| `impl#[Self][Trait]member().` (or `.` / `!`), `Trait` defined in the project | `a/b/Self::member` | the `Self` type key (see below) | `Trait` | exact |
| `impl#[Self]member().` (inherent impl) | `a/b/Self::member` | the `Self` type key | – | exact |
| `impl#[Self][Trait]member().`, `Trait` not defined in the project | the `Self` type key | – | – | folded into the type |
| `impl#[Self]…` with no member | the `Self` type key | – | – | folded |
| `name().` (free function), `NAME.` (const/static), `name!` (macro) | `a/b/name` | – | – | exact |
| anything nested under a function, const or macro | the enclosing item | – | – | folded |
| `[T]`, `(p)`, `meta:` first | none | | | ignored |
| namespaces only (a module) | none | | | ignored |

`a/b/` is the module of the symbol itself, so for `impl` members it is the
module of the `impl` block, not of the type. The *`Self` type key* is
where the package defines a type of that name: `a/b/Self` if the type is
defined there, else the type's one definition elsewhere in the package
(so an external-trait impl in module `y` of a type defined in `x` folds
into `x/Self`). If the package defines no type of that name, or several in
other modules, the impl's module is used (`a/b/Self`); such owners may
still be reattached by simple name in step 6 of `graph`. `Self` and `Trait` are reduced to
their base name: `&crate::a::Foo<T>` →
`Foo`. A trait counts as "defined in the project" if a project symbol has
it as its first type descriptor. *Exact* items set a node's `lines` and
`kind`; folded symbols only route references to their parent.

## `sweep`

Cluster at several resolutions and report how the partition changes.

```text
refgraph sweep [-h] [-r RESOLUTIONS [RESOLUTIONS ...]] [--json]
               [--weighting {log,raw,binary}] [--seeds SEEDS] graph
```

| Argument | Type | Default | Repeatable | Meaning |
|---|---|---|---|---|
| `graph` | path | required | no | A [`graph.json`](#graphjson). |
| `-r`, `--resolutions` | one or more floats | `0.4 0.6 0.8 1.0 1.2 1.5 2.0` | no (takes a list) | Resolutions to try, in the order given. |
| `--json` | flag | off | no | Print JSON instead of the table. |
| `--weighting` | `log` \| `raw` \| `binary` | `log` | no | Edge weight for Louvain: `1 + ln(n)`, `n`, or `1`, where `n` is the summed weight of both directions. |
| `--seeds` | int | `30` | no | Louvain runs per resolution, with seeds `0 … SEEDS-1`. |

| Stream | Content |
|---|---|
| stdout | Table: columns `resolution`, `clusters`, `modularity` (3 dp), `stability` (3 dp), `sizes` (first 12 cluster sizes, then `…`), followed by `suggested resolution: R`. With `--json`: an object of the [`sweep.json`](#sweepjson) form. |
| stderr | nothing (unless an error) |
| files | none |

Exit: 0; 1 without networkx or on malformed input; 2 if an input file cannot be opened.

The suggested resolution is the row with the highest modularity rounded
to 3 decimals; ties go to the resolution closest to 1.0.

## `cluster`

Partition the graph at one resolution and compute metrics.

```text
refgraph cluster [-h] [-r RESOLUTION] [-o OUTPUT]
                 [--weighting {log,raw,binary}] [--seeds SEEDS] graph
```

| Argument | Type | Default | Repeatable | Meaning |
|---|---|---|---|---|
| `graph` | path | required | no | A [`graph.json`](#graphjson). |
| `-r`, `--resolution` | float | `1.0` | no | Louvain resolution. |
| `-o`, `--output` | path or `-` | `clusters.json` | no | Output file; `-` for stdout. |
| `--weighting` | `log` \| `raw` \| `binary` | `log` | no | As for `sweep`. |
| `--seeds` | int | `30` | no | As for `sweep`. |

| Stream | Content |
|---|---|
| stdout | the clusters JSON when `-o -` |
| stderr | `N clusters, modularity Q, stability S` then `wrote OUTPUT` |
| files | `OUTPUT` ([`clusters.json`](#clustersjson)) |

Exit: 0; 1 without networkx or on malformed input; 2 if an input file cannot be opened.

Algorithm: the directed graph is made undirected (`A→B` and `B→A`
weights summed into `raw`; `weight` = weighting of `raw`). Every node is
kept, including isolated ones. networkx `louvain_communities` runs once per
seed. The partition with the highest standard (resolution 1) weighted
modularity is kept. Stability is the mean over edges of
`max(together, SEEDS − together) / SEEDS`, where `together` is the number
of runs that put the edge's ends in the same cluster (1.0 for a graph with
no edges). Clusters are sorted by size descending, then by first member.

## `report`

Render a `clusters.json` as Markdown.

```text
refgraph report [-h] [--names NAMES] [--members MEMBERS] [-o OUTPUT] clusters
```

| Argument | Type | Default | Repeatable | Meaning |
|---|---|---|---|---|
| `clusters` | path | required | no | A [`clusters.json`](#clustersjson). |
| `--names` | path | none | no | A [names file](#namesjson). Adds names, tags and summaries. |
| `--members` | int | `25` | no | Members listed per cluster. `0` omits the line. |
| `-o`, `--output` | path or `-` | `-` (stdout) | no | Output file. |

| Stream | Content |
|---|---|
| stdout | the report when `-o -` (default) |
| stderr | `wrote OUTPUT` when writing a file |
| files | `OUTPUT` ([`report.md`](#reportmd)) |

Exit: 0; 1 on malformed input; 2 if an input file cannot be opened. Does not need networkx.

## `names`

Write a names template, or carry names over from an earlier run.

```text
refgraph names [-h] [--carry-from OLD_CLUSTERS OLD_NAMES]
               [--threshold THRESHOLD] [-o OUTPUT] clusters
```

| Argument | Type | Default | Repeatable | Meaning |
|---|---|---|---|---|
| `clusters` | path | required | no | The new [`clusters.json`](#clustersjson). |
| `--carry-from OLD_CLUSTERS OLD_NAMES` | two paths | none | no | Copy names from an earlier clusters file and its names file. |
| `--threshold` | float | `0.5` | no | Minimum Jaccard overlap to copy a name. Only used with `--carry-from`. |
| `-o`, `--output` | path or `-` | `-` (stdout) | no | Output file. |

Without `--carry-from`, every cluster gets an empty entry with a `_hint`.

With `--carry-from`, for each new cluster:

1. Find the old cluster with the highest Jaccard overlap of member keys
   (the first one wins a tie; overlap 0 means no match, `id: null`).
2. Record it in `_hint.closest_old` (`id`, `overlap` to 3 dp, `name`).
3. If the overlap is ≥ `--threshold` and the old id has an entry in
   `OLD_NAMES`, copy whichever of `name`, `region`, `tags`, `summary` it
   has.

Two new clusters can inherit the same old name (a split). An old name
that no new cluster reaches is dropped silently.

| Stream | Content |
|---|---|
| stdout | the names JSON when `-o -` (default) |
| stderr | `wrote OUTPUT` when writing a file |
| files | `OUTPUT` ([`names.json`](#namesjson)) |

Exit: 0; 1 on malformed input; 2 if an input file cannot be opened. Does not need networkx.

## `where`

Find which cluster an item or file belongs to.

```text
refgraph where [-h] [--names NAMES] [--limit LIMIT] clusters query
```

| Argument | Type | Default | Repeatable | Meaning |
|---|---|---|---|---|
| `clusters` | path | required | no | A [`clusters.json`](#clustersjson). |
| `query` | string | required | no | Case-insensitive substring of a file path or an item key. |
| `--names` | path | none | no | A [names file](#namesjson) to label clusters. |
| `--limit` | int | `50` | no | Maximum file lines, and separately maximum item lines, printed (up to `LIMIT` of each). |

Output on stdout, in this order:

```text
file PATH: C<id> <name> (<items>), C<id> <name> (<items>), …     one line per matching file in file_split, sorted by path (first LIMIT)
… N more files (use --limit)                                     if more files matched than --limit
KEY  ->  C<id> <name>                                            one line per matching item, sorted by key (first LIMIT)
… N more items (use --limit)                                     if more items matched than --limit
```

`<name>` is omitted when there is no names file or no name. Only files
that contain graph nodes are searched, so test files never match.

Exit: 0 if anything matched; 1 and `no item or file matches 'QUERY'` on
stderr if nothing did; 2 if an input file cannot be opened.

## `analyze`

Run `index`, `graph`, `sweep`, `cluster`, `names` and `report` in one go.

```text
refgraph analyze [-h] [--out-dir OUT_DIR] [--index INDEX] [--keep-index]
                 [--features F [F ...]] [--all-features]
                 [--no-default-features] [-v] [-r RESOLUTION]
                 [--carry-names OLD_CLUSTERS OLD_NAMES]
                 [--threshold THRESHOLD]
                 [--granularity {item,type,file}] [--tests GLOB]
                 [--exclude GLOB] [--exclude-symbols REGEX]
                 [--package NAME] [--owner-weight OWNER_WEIGHT]
                 [--impl-weight IMPL_WEIGHT]
                 [--weighting {log,raw,binary}] [--seeds SEEDS]
                 [root]
```

| Argument | Type | Default | Repeatable | Meaning |
|---|---|---|---|---|
| `root` | path | `.` | no | Cargo root to index. Ignored with `--index`. |
| `--out-dir` | path | `refgraph-out` | no | Output directory; created if missing. |
| `--index` | path | none | no | Use this SCIP index instead of running rust-analyzer. |
| `--keep-index` | flag | off | no | Keep the generated `OUT_DIR/index.scip`. No effect with `--index`. |
| `--features F [F ...]` | one or more strings | none | no (takes a list) | As for `index`. Ignored with `--index`. |
| `--all-features` | flag | off | no | As for `index`. Ignored with `--index`. |
| `--no-default-features` | flag | off | no | As for `index`. Ignored with `--index`. |
| `-v`, `--verbose` | flag | off | no | As for `index`. Ignored with `--index`. |
| `-r`, `--resolution` | float | auto | no | Cluster at this resolution instead of the suggested one. The sweep still runs and includes it. |
| `--carry-names OLD_CLUSTERS OLD_NAMES` | two paths | none | no | Carry names as `names --carry-from` does. |
| `--threshold` | float | `0.5` | no | As for `names`. Only used with `--carry-names`. |
| `--granularity` | `item` \| `type` \| `file` | `item` | no | As for `graph`. |
| `--tests GLOB` | glob | default list | yes | As for `graph`. |
| `--exclude GLOB` | glob | none | yes | As for `graph`. |
| `--exclude-symbols REGEX` | Python regex | `(^|/)tests?(/|::|$)` | no | As for `graph`. |
| `--package NAME` | string | all packages that define symbols | yes | As for `graph`. |
| `--owner-weight` | int | `2` | no | As for `graph`. |
| `--impl-weight` | int | `1` | no | As for `graph`. |
| `--weighting` | `log` \| `raw` \| `binary` | `log` | no | As for `sweep`. |
| `--seeds` | int | `30` | no | As for `sweep`. |

Only sweep's `-r/--resolutions` list has no `analyze` equivalent; run
`sweep` separately to try other resolutions.

Steps and files, all under `OUT_DIR`:

| Step | File | Notes |
|---|---|---|
| index | `index.scip` | Only without `--index`. Deleted at the end unless `--keep-index`. |
| graph | `graph.json` | |
| sweep | `sweep.json` | The default resolutions, plus `-r` if given and not among them (rows in ascending order). `suggested` is computed over all rows. |
| cluster | `clusters.json` | At `-r`, or the sweep's suggestion. |
| names | `names.json` | With `--carry-names`: carried names, overwriting any existing file. Without: a template if the file does not exist; an existing file is left untouched and `kept existing PATH (ids may be stale; use --carry-names to remap)` is logged. |
| report | `report.md` | Labelled with the names in `names.json` when it was carried or kept. A freshly written template has no names, so the report is unlabelled. |

| Stream | Content |
|---|---|
| stdout | nothing |
| stderr | `index` messages, the enclosing-range warning if any, `wrote PATH` per file, the names message, and finally `resolution R: N clusters, modularity Q, stability S`. The `N nodes, …` graph summary is not printed. |

Exit: as for `index` when indexing fails (2, or rust-analyzer's code);
0 on success; 2 for a missing input file or an invalid `--exclude-symbols`
regex (one-line message, as for `graph`); 1 without networkx or on
malformed input. If a later step fails,
files already written stay, and so does `index.scip`.

## Exit codes

| Code | When |
|---|---|
| 0 | Success. Also when stdout is closed early (a `BrokenPipeError`, e.g. `refgraph report … \| head`). |
| 1 | `where` found no match. Any uncaught Python exception (malformed JSON or SCIP input, a JSON file missing keys), printed as a traceback. networkx missing for `sweep`/`cluster`/`analyze`, with the message ``refgraph: clustering needs networkx. Run through `uv run` (the launcher declares it) or `pip install networkx`.`` |
| 2 | argparse usage error (unknown option, missing argument, invalid choice or number). Any command: a file that cannot be opened (missing or unreadable input, unwritable output), printed as `refgraph: PATH: REASON`; an invalid `--exclude-symbols` regex (`graph`, `analyze`), printed as `refgraph: invalid --exclude-symbols regex 'R': REASON`. `index`/`analyze`: no `Cargo.toml` in the root, or `rust-analyzer` not on `PATH`. |
| other | `index`/`analyze`: rust-analyzer's own non-zero exit code. |
| 127 | Shell: the launcher was run directly and `uv` is not on `PATH` (`env: uv: No such file or directory`). |

## File formats

All JSON keys are listed. Floats rounded by refgraph are noted with their
decimal places (dp).

### `index.scip` (input)

A [SCIP](https://github.com/sourcegraph/scip/blob/main/scip.proto)
protobuf file. refgraph decodes only these fields and skips the rest:

| Message | Field (number) | Use |
|---|---|---|
| `Index` | `documents` (2) | each document |
| `Document` | `relative_path` (1) | document path for globs and `file` |
| `Document` | `occurrences` (2) | definitions and references |
| `Document` | `symbols` (3) | symbol kinds |
| `Occurrence` | `range` (1) | 3 ints `[line, start_col, end_col]` or 4 ints `[start_line, start_col, end_line, end_col]`, zero-based |
| `Occurrence` | `symbol` (2) | the SCIP symbol string |
| `Occurrence` | `symbol_roles` (3) | bit 1 (`Definition`) marks definitions; other bits are ignored |
| `Occurrence` | `enclosing_range` (7) | the definition's full body; required to attribute references |
| `SymbolInformation` | `symbol` (1), `kind` (5) | kind recorded on nodes |
| `SymbolInformation` | `display_name` (6) | decoded, not used |

Wire types 0, 1, 2 and 5 are supported. Any other wire type raises
`ValueError: unsupported protobuf wire type N` (exit 1).

### `graph.json`

Written by `graph` and `analyze`. Read by `sweep`, `cluster`.

```jsonc
{
 "format": "refgraph.graph/1",
 "meta": {
  "index": "/…/index.scip",
  "packages": ["cargo delos 0.1.0"],
  "granularity": "item",
  "test_globs": ["tests/*", "*/tests/*", "…"],
  "exclude_globs": [],
  "exclude_symbols": "(^|/)tests?(/|::|$)",
  "owner_weight": 2,
  "impl_weight": 1
 },
 "nodes": {
  "engine/Ctx::cancelled": {"file": "src/engine/mod.rs", "kind": 26, "lines": 3,
                            "owner": "engine/Ctx", "trait": null}
 },
 "edges": [["engine/Ctx", "engine/Engine", 1], …],
 "tests": {"tests/ui_state.rs": {"ui/state/App": 35, "graph/NodeId": 6, …}}
}
```

| Key | Type | Meaning |
|---|---|---|
| `format` | string | Always `refgraph.graph/1`. |
| `meta.index` | string | The index path as given on the command line. |
| `meta.packages` | string[] | Project packages as `manager name version`, sorted. |
| `meta.granularity` | string | `item`, `type` or `file`. |
| `meta.test_globs` | string[] | Test globs used. |
| `meta.exclude_globs` | string[] | `--exclude` globs used. |
| `meta.exclude_symbols` | string | The regex used; `""` if disabled. |
| `meta.owner_weight`, `meta.impl_weight` | int | Structural weights used. |
| `nodes` | object | Node key → node, sorted by key. |
| `nodes.*.file` | string | Document of the first definition seen for the item (usually its declaration). For coarsened nodes, the file of the first member. |
| `nodes.*.kind` | int | SCIP `SymbolInformation.Kind` of the item's own definition (else of the first folded definition; `0` if unknown). `0` for `file` nodes. |
| `nodes.*.lines` | int | Lines spanned by the item's own definition (largest if several); `0` if only folded members were seen. Coarsened nodes: sum over members. |
| `nodes.*.owner` | string \| null | Owning type key for methods, after resolution; `null` otherwise or if unresolved. Always `null` when coarsened. |
| `nodes.*.trait` | string \| null | Implemented project trait's simple name for trait-impl methods. Always `null` when coarsened. |
| `nodes.*.items` | int | Only at `type`/`file` granularity: item count merged into the node. |
| `edges` | `[string, string, int][]` | Directed `[source, target, weight]`, sorted. No self-loops; both ends are nodes. |
| `tests` | object | Test document path → {node key → reference count}. May contain empty objects. |

SCIP kinds seen in the Delos index: 8 Constant, 11 Enum, 15 Field,
17 Function, 26 Method, 49 Struct, 53 Trait, 55 TypeAlias, 70 TraitMethod,
80 StaticMethod, 82 StaticVariable. See `scip.proto` for the full enum.

### `sweep.json`

Written by `analyze`; the same object is printed by `sweep --json`. No
`format` key.

```json
{
 "rows": [
  {"resolution": 0.4, "clusters": 7, "modularity": 0.6293, "stability": 0.942,
   "sizes": [210, 189, 172, 135, 39, 19, 14]}
 ],
 "suggested": 0.8
}
```

| Key | Type | Meaning |
|---|---|---|
| `rows[].resolution` | float | Resolution tried. |
| `rows[].clusters` | int | Number of clusters in the kept partition. |
| `rows[].modularity` | float (4 dp) | Standard modularity of the kept partition. |
| `rows[].stability` | float (4 dp) | Stability across seeds. |
| `rows[].sizes` | int[] | Cluster sizes, largest first. |
| `suggested` | float | Suggested resolution. |

### `clusters.json`

Written by `cluster` and `analyze`. Read by `report`, `names`, `where`.
`names --carry-from` reads only `clusters[].id` and `clusters[].members`
of the old file.

```jsonc
{
 "format": "refgraph.clusters/1",
 "graph_meta": { /* copy of graph.json "meta" */ },
 "params": {"resolution": 1.0, "weighting": "log", "seeds": 30},
 "modularity": 0.6722, "stability": 0.9604,
 "totals": {"nodes": 778, "edges": 2879, "refs": 6981},
 "clusters": [
  {"id": 0, "size": 110, "lines": 419, "internal_refs": 412, "external_refs": 304,
   "cohesion": 0.5754,
   "files": [["src/nodes/basic.rs", 62], …],
   "hubs": [["engine/geom/Size", 193], …],
   "interface": [["engine/geom/Size", 92], …],
   "neighbors": [[1, 110], [3, 83], …],
   "members": ["engine/geom/Size", "engine/geom/Size::min_dim", …]}
 ],
 "between": [[1, 2, 192], [0, 1, 110], …],
 "bridges": [{"item": "engine/geom/Size", "cluster": 0, "refs": 193,
              "clusters_touched": 6, "participation": 0.6705}, …],
 "file_split": {"src/engine/geom.rs": [[4, 31], [0, 4]], …},
 "tests": {"tests/engine.rs": [[1, 105], [4, 34], …], …},
 "layout_comparison": {
  "clusters": {"groups": 13, "modularity": 0.6722, "internal": 0.8035},
  "files": {"groups": 30, "modularity": 0.4772, "internal": 0.5476},
  "dirs": {"groups": 4, "modularity": 0.5095, "internal": 0.7767},
  "prefix@2": {"groups": 9, "modularity": 0.5279, "internal": 0.75}
 }
}
```

All reference counts below are `raw` undirected weights (source references
plus structural weights).

| Key | Type | Meaning |
|---|---|---|
| `format` | string | Always `refgraph.clusters/1`. |
| `graph_meta` | object | The input graph's `meta`. |
| `params` | object | `resolution` (float), `weighting` (string), `seeds` (int) used. |
| `modularity` | float (4 dp) | Standard modularity of the kept partition. |
| `stability` | float (4 dp) | Stability across seeds. |
| `totals.nodes` | int | Nodes. |
| `totals.edges` | int | Undirected edges (after merging directions). |
| `totals.refs` | int | Sum of raw weights. |
| `clusters[]` | object[] | Largest first. Ids are positions in this list. |
| `clusters[].id` | int | Cluster id, valid for this file only. |
| `clusters[].size` | int | Member count. |
| `clusters[].lines` | int | Sum of members' `lines`. |
| `clusters[].internal_refs` | int | References between members. |
| `clusters[].external_refs` | int | References between members and non-members. |
| `clusters[].cohesion` | float (4 dp) | `internal / max(1, internal + external)`. |
| `clusters[].files` | `[string, int][]` | File → member count, most first. |
| `clusters[].hubs` | `[string, int][]` | Top 12 members by total references. |
| `clusters[].interface` | `[string, int][]` | Top 12 members by references to other clusters. |
| `clusters[].neighbors` | `[int, int][]` | Other cluster id → references, most first. |
| `clusters[].members` | string[] | Member keys, sorted. |
| `between` | `[int, int, int][]` | `[a, b, refs]` with `a < b`, heaviest first. |
| `bridges` | object[] | Up to 40 nodes with ≥ 15 references, sorted by `participation × ln(refs)` descending. |
| `bridges[].item` | string | Node key. |
| `bridges[].cluster` | int | Its cluster. |
| `bridges[].refs` | int | Its total references. |
| `bridges[].clusters_touched` | int | Clusters its neighbours are in, including its own. |
| `bridges[].participation` | float (4 dp) | `1 − Σ_c (refs to cluster c / refs)²`. |
| `file_split` | object | File path (sorted) → `[cluster, node count][]`, most first. Every file with nodes is listed. |
| `tests` | object | Test document path (sorted) → `[cluster, refs][]`, most first. |
| `layout_comparison` | object | Partition name → score. `clusters` is the kept partition; then `files`, `dirs` (parent directory, `.` for top-level files) and `prefix@N` for `N = 1 …` maximum depth. Partitions with one group, or identical to an earlier one, are omitted. |
| `layout_comparison.*.groups` | int | Number of groups. |
| `layout_comparison.*.modularity` | float (4 dp) | Standard modularity of that partition. |
| `layout_comparison.*.internal` | float (4 dp) | Share of references inside groups. |

### `names.json`

Written by `names` and `analyze`; edited by people. Read by `report`,
`where` (`--names`) and as `OLD_NAMES` by `names --carry-from` /
`analyze --carry-names`.

```json
{
 "format": "refgraph.names/1",
 "clusters": {
  "0": {
   "name": "Node kind catalogue",
   "region": "Node definitions",
   "tags": ["nodes", "metadata", "sizing", "registry"],
   "summary": "",
   "_hint": {
    "size": 110,
    "cohesion": 0.5754,
    "files": ["src/nodes/basic.rs (62)", "src/nodes/resize.rs (18)"],
    "hubs": ["engine/geom/Size", "nodes/Category"],
    "closest_old": {"id": "0", "overlap": 0.973, "name": "Node kind catalogue"}
   }
  }
 }
}
```

| Key | Type | Written | Read by |
|---|---|---|---|
| `format` | string | `refgraph.names/1` | nobody |
| `clusters` | object | cluster id as a string → entry | all readers; a file without it labels nothing |
| `clusters.*.name` | string | `""` or carried | `report` (heading, neighbours, seams, bridges, tests), `where`, carrying |
| `clusters.*.region` | string | `""` or carried | carrying only; not shown in the report |
| `clusters.*.tags` | string[] | `[]` or carried | `report` (`Tags:` line), carrying |
| `clusters.*.summary` | string | `""` or carried | `report` (paragraph), carrying |
| `clusters.*._hint.size` | int | cluster size | nobody |
| `clusters.*._hint.cohesion` | float | cluster cohesion | nobody |
| `clusters.*._hint.files` | string[] | top 6 files as `"path (count)"` | nobody |
| `clusters.*._hint.hubs` | string[] | top 8 hub keys | nobody |
| `clusters.*._hint.closest_old` | object | only when carrying: `id` (string or `null`), `overlap` (float, 3 dp), `name` (string) | nobody |

Any JSON object with a `clusters` map of this shape works as a names file;
extra keys are ignored. For example, the Delos model's
`system-models/code-clusters/data/cluster-names.json` has extra
`resolution` and `weighting` keys and no `format`.

### `report.md`

Written by `report` and `analyze`. Not read by refgraph. Structure:

| Section | Content |
|---|---|
| `# Reference-graph clusters` | Bullets: graph size (nodes, undirected edges, references, granularity, packages); Louvain parameters; cluster count, modularity, stability (3 dp). |
| `## Partition vs. file layout` | Table `Partition \| Groups \| Modularity \| Refs kept inside` from `layout_comparison`. |
| `## Clusters` | One `### C<id> [name]` per cluster: optional `Tags:` line and summary paragraph from names; then bullets for size/lines/cohesion, `Files` (top 8, `src/` prefix stripped), `Hubs` (top 10), `Interface` (top 8, omitted if empty), `Neighbours` (top 6, omitted if none), `Members` (first `--members`, with `… (+N)`; omitted when 0). |
| `## Seams (references between clusters)` | Table `Between \| Refs`, top 15 pairs. |
| `## Bridge items (links spread over many clusters)` | Table `Item \| Home \| Refs \| Clusters touched \| Participation`, top 15. |
| `## Files split across clusters` | Table `File \| Items per cluster` for files in more than one cluster (`C<id> <count>` joined by ` · `; no names). |
| `## Test references per cluster` | Only when `tests` is non-empty. Table `Cluster \| Refs \| Test files` with the top 4 test files per cluster; `—` for none. |

## Environment and configuration

refgraph reads no environment variables and no configuration files. It
is affected indirectly by:

| Variable | Effect |
|---|---|
| `PATH` | Where `uv` (launcher) and `rust-analyzer` (`index`, `analyze`) are found. |
| `TMPDIR` | Where `index` writes its temporary rust-analyzer config file. |
| uv's variables (`UV_CACHE_DIR`, `UV_PYTHON`, …) | How the launcher's environment is created. |
| Cargo/rust-analyzer variables | Passed through to rust-analyzer unchanged. |
