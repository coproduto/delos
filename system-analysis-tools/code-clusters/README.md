# refgraph: find the natural clusters in a codebase

`refgraph` answers one question: **which parts of this code actually belong
together?** It builds a reference graph of a project's items (functions,
types, methods, constants) from a compiler-grade semantic index. It then
splits the graph into **clusters**: groups whose members reference each
other a lot and the rest of the code little.

Clusters often cut across files and directories. That is the point. They
show what moves together, where the seams are, and which parts can be
reviewed as black boxes. The model it produces for Delos lives in
[`system-models/code-clusters/`](../../system-models/code-clusters/).

> Working with an agent? Point it at [`AGENTS.md`](AGENTS.md). Every command,
> option, exit code and file format is in [`REFERENCE.md`](REFERENCE.md).

## Contents

- [When to use it, and when not](#when-to-use-it-and-when-not)
- [Concepts and terminology](#concepts-and-terminology)
- [Requirements and installation](#requirements-and-installation)
- [Quick start](#quick-start)
- [Workflows](#workflows)
- [Interpreting results](#interpreting-results)
- [Limitations and accuracy](#limitations-and-accuracy)
- [Troubleshooting](#troubleshooting)
- [Development](#development)

## When to use it, and when not

Use it to:

- Explain a project's real structure to someone new, with numbers.
- Check whether the directory layout matches how the code is wired.
- Before a change, see which clusters a file or item belongs to, and whether
  the change crosses a heavy seam.
- Review a diff by cluster rather than by file.
- Find the "shared vocabulary" items (bridges) whose changes ripple widely.
- See which clusters the integration tests exercise.

It cannot tell you:

- **Runtime behaviour.** The graph is static. Calls through function
  pointers, closures stored in data, channels, message passing, or
  serialised data are invisible unless a symbol is referenced in source.
- **Code that the indexer did not see.** For Rust, that means code behind
  cargo features or `cfg`s that were off for the index.
- **Whether the code is good.** A cluster is a fact about references, not a
  verdict on design. Low cohesion may be fine; high cohesion may hide a mess.
- **Unit-test coverage.** Inline `#[cfg(test)] mod tests` blocks are
  dropped by default and are not counted as tests (see
  [Limitations](#limitations-and-accuracy)).
- **Names.** Clusters come out numbered. A human or an agent names them.

## Concepts and terminology

Every term that appears in the outputs.

| Term | Meaning |
|---|---|
| **SCIP index** | A file (`index.scip`) produced by a [SCIP](https://github.com/sourcegraph/scip) indexer. It lists every symbol definition and reference in the code, with source ranges. For Rust, `rust-analyzer scip` writes it. |
| **Item** | The unit refgraph puts in the graph: a free function, a type, a constant or static, a macro, or a method (`Type::method`). Fields, enum variants, nested items and impls of external traits are folded into their parent item. |
| **Key** | An item's name in the graph: module path joined with `/`, then the name. Examples: `engine/geom/Size`, `engine/gpu/Gpu::upload`. When the index holds several project packages, keys get a `package:` prefix. |
| **Node** | One vertex of the graph. At the default `item` granularity, a node is an item. At `type` granularity, a type with its methods folded in. At `file` granularity, one file. |
| **Edge** | A weighted link between two nodes. In `graph.json` edges are directed (`A` references `B`). Clustering treats them as undirected and adds both directions together. |
| **References** | The edge weight before transformation: the number of reference occurrences from inside the body of `A` to `B`, plus the structural weights below. |
| **Structural edges** | Two kinds of synthetic edges: method → its owning type (weight 2 by default) and trait-impl method → the trait's method declaration (weight 1 by default). They keep methods near their types and implementations near their traits. |
| **Package** | A SCIP package (`manager name version`, for example `cargo delos 0.1.0`). The packages that *define* symbols in the index are "the project". References into other packages (std, crates) are ignored. |
| **Test files** | Documents matching the test globs. They are kept out of the graph, but their references are recorded to show which clusters each test file exercises. |
| **Granularity** | The node unit: `item`, `type` or `file`. |
| **Weighting** | How references become the edge weight used by Louvain: `log` = `1 + ln(n)` (default), `raw` = `n`, `binary` = `1`. |
| **Louvain** | The community-detection algorithm used to find clusters (from networkx). It is randomised, so refgraph runs it several times. |
| **Resolution** | Louvain's knob. Lower values give fewer, larger clusters; higher values give more, smaller ones. |
| **Seeds** | How many Louvain runs (seeds `0` … `seeds-1`) are made per resolution. The run with the highest modularity is kept. |
| **Cluster** | One group in the kept partition. Clusters are numbered `C0`, `C1`, … from largest to smallest. **Ids are only valid for one `clusters.json`.** |
| **Modularity (Q)** | How much more weight falls inside groups than chance would predict. Always computed at resolution 1, so values compare across resolutions. About 0.3 is weak structure; 0.6–0.7 is strong. |
| **Stability** | Over all seeded runs, for each edge, the share of runs that agree with the majority on whether its two ends share a cluster; averaged over edges. 1.0 means every run agrees. |
| **Cohesion** | Per cluster: internal references ÷ (internal + external references). High means a black box with a narrow interface. Low means changes ripple out. |
| **Internal / external refs** | References between two members of the cluster / between a member and a non-member. |
| **Hubs** | The members with the most references in total (inside and outside the cluster). |
| **Interface** | The members with the most references to and from other clusters. |
| **Neighbours** | Other clusters, with the references between them and this one. |
| **Seam** | A pair of clusters and the references between them (`between` in JSON). Heavy seams are where decisions spread. |
| **Bridge** | An item with at least 15 references whose links spread over many clusters. Measured by the participation coefficient. |
| **Participation** | `1 − Σ (share of the item's references going to cluster c)²`, over all clusters including its own. 0 means all links stay in one cluster. |
| **Clusters touched** | How many clusters a bridge item links to, including its own. |
| **Home** | The cluster a bridge item was assigned to. |
| **Layout comparison** | The same scores (groups, modularity, share of references kept inside) for partitions implied by the file layout: `files` (one group per file), `dirs` (one group per directory) and `prefix@N` (one group per first `N` path components). Tells you whether clusters describe the code better than its folders. |
| **Refs kept inside** | In the layout comparison: the share of all references whose two ends are in the same group (`internal` in JSON). |
| **File split** | How many items of one file landed in each cluster. A file split across clusters is a candidate split point. |
| **Names file** | A JSON file that gives each cluster id a `name`, `region`, `tags` and `summary`. Written by hand (from a template). |
| **Carrying names** | Mapping names from an old run's clusters onto a new run's clusters by member overlap (Jaccard similarity). |
| **Jaccard overlap** | `|A ∩ B| ÷ |A ∪ B|` for two clusters' member sets. |
| **Suggested resolution** | The resolution with the highest modularity (to 3 decimals), ties broken towards 1.0. |

## Requirements and installation

| Requirement | Needed for | How to install |
|---|---|---|
| Python ≥ 3.10 | everything | your OS package manager, or `uv python install` |
| [uv](https://docs.astral.sh/uv/) | running the `refgraph` launcher as a script | `curl -LsSf https://astral.sh/uv/install.sh \| sh` or `brew install uv` |
| networkx ≥ 3.2 | `sweep`, `cluster`, `analyze` | nothing with uv (the launcher declares it); otherwise `pip install networkx` |
| rust-analyzer | `index`, and `analyze` without `--index` | `rustup component add rust-analyzer` |
| A Cargo toolchain | rust-analyzer loads the workspace through cargo | [rustup](https://rustup.rs) |
| Another SCIP indexer | non-Rust projects | e.g. `scip-typescript`, `scip-python`, `scip-java`, `scip-go` |

There is nothing to install for refgraph itself. The launcher is a
[uv script](https://docs.astral.sh/uv/guides/scripts/) whose header declares
`requires-python = ">=3.10"` and `networkx>=3.2`. The first run creates a
cached environment; later runs start immediately.

Without uv, run the launcher with a Python that has networkx:

```sh
python3 system-analysis-tools/code-clusters/refgraph --help
```

Without networkx, `index`, `graph`, `report`, `names` and `where` still work.
`sweep`, `cluster` and `analyze` stop with
`refgraph: clustering needs networkx. …` (exit 1). The SCIP decoder is
built in, so no protobuf package is needed.

## Quick start

From the project root. Set a shell variable for the launcher:

```sh
T=system-analysis-tools/code-clusters/refgraph
$T analyze --out-dir /tmp/refgraph-out
```

This indexes the crate with rust-analyzer (about 25 s for Delos on a warm
cargo cache), builds the graph, sweeps resolutions, clusters at the
suggested one and writes five files. Real output (paths shortened):

```text
running: /Users/…/.cargo/bin/rust-analyzer scip /Users/…/delos --output /…/refgraph-out/index.scip
wrote /tmp/refgraph-out/index.scip
wrote /tmp/refgraph-out/graph.json
wrote /tmp/refgraph-out/sweep.json
wrote /tmp/refgraph-out/clusters.json
wrote /tmp/refgraph-out/names.json
wrote /tmp/refgraph-out/report.md
resolution 0.8: 11 clusters, modularity 0.673, stability 0.965
```

`index.scip` is deleted at the end unless you pass `--keep-index`. Read
`report.md` first. Its top looks like this (Delos at `197e30c`, resolution 1.0):

```markdown
# Reference-graph clusters

- Graph: 778 nodes, 2879 undirected edges, 6981 references (item granularity; packages: cargo delos 0.1.0)
- Louvain: resolution 1.0, weighting `log`, best of 30 seeds
- 13 clusters, modularity 0.672, stability 0.960

## Partition vs. file layout

| Partition | Groups | Modularity | Refs kept inside |
|---|---:|---:|---:|
| clusters | 13 | 0.672 | 80% |
| files | 30 | 0.477 | 55% |
| dirs | 4 | 0.509 | 78% |
| prefix@2 | 9 | 0.528 | 75% |

## Clusters

### C0

- 110 items, ~419 lines, cohesion 0.58 (412 internal / 304 external refs)
- Files: nodes/basic.rs (62), nodes/resize.rs (18), nodes/mod.rs (13), nodes/bgremove.rs (9), engine/geom.rs (4), …
- Hubs: `engine/geom/Size`, `nodes/Category`, `nodes/NodeKind`, `nodes/basic/Clip`, …
- Interface (most referenced from outside): `engine/geom/Size` (92), `nodes/Category` (24), `nodes/NodeKind` (23), …
- Neighbours: C1 (110), C3 (83), C4 (66), C2 (29), C10 (13), C6 (2)
- Members: `engine/geom/Size`, `engine/geom/Size::min_dim`, … (+85)
```

Without `--out-dir`, the output goes to `./refgraph-out/`. That name is
gitignored only inside `system-analysis-tools/` (by its `.gitignore`), not
at the project root, so prefer an output directory outside the repository.

## Workflows

### First analysis, step by step

Use the separate commands when you want to look at each choice.

```sh
T=system-analysis-tools/code-clusters/refgraph
O=/tmp/clusters                                   # any scratch directory
mkdir -p $O
$T index . -o $O/index.scip                       # ~25 s on Delos
$T graph $O/index.scip -o $O/graph.json
$T sweep $O/graph.json                            # pick a resolution
$T cluster $O/graph.json -r 1.0 -o $O/clusters.json
$T names $O/clusters.json -o $O/names.json        # template to fill in
$T report $O/clusters.json --names $O/names.json -o $O/report.md
```

`graph` prints a one-line summary on stderr:

```text
778 nodes, 2883 edges, 6981 references; packages: cargo delos 0.1.0
```

`sweep` prints a table on stdout:

```text
resolution clusters modularity stability  sizes
       0.4        7      0.629     0.942  210 189 172 135 39 19 14
       0.6       11      0.662     0.965  142 130 104 88 86 73 44 39 30 28 14
       0.8       11      0.673     0.965  124 112 97 90 78 73 58 56 44 32 14
       1.0       13      0.672     0.960  110 101 90 89 75 56 55 54 43 43 29 19 …
       1.2       12      0.672     0.948  111 103 101 88 74 67 62 44 43 43 28 14
       1.5       14      0.671     0.945  101 87 82 81 65 63 60 55 53 48 27 23 …
       2.0       19      0.654     0.943  87 86 70 67 61 51 45 44 40 34 31 28 …
suggested resolution: 0.8
```

Here 0.6–1.5 is a plateau: modularity stays within 0.01 and stability
stays above 0.94. Any value on it is defensible. The Delos model uses 1.0
(13 clusters) for detail and 0.4 (7 groups) for coarse "regions".

Then fill in `names.json` (see [Naming clusters](#naming-clusters)) and
re-run `report` with `--names`.

### Naming clusters

Clusters come out numbered by size. `names` writes a template with one
entry per cluster id:

```json
"0": {"name": "", "region": "", "tags": [], "summary": "",
      "_hint": {"size": 110, "cohesion": 0.5754,
                "files": ["src/nodes/basic.rs (62)", "…"],
                "hubs": ["engine/geom/Size", "nodes/Category", "…"]}}
```

Fill in `name` (what the cluster does, in a few words), `region` (a coarse
grouping, for your own display; the report does not show it), `tags` and a
one-to-three-sentence `summary`. Base names on the hubs, the interface and
the module docs, not on directory names. The Delos model's names are in
`system-models/code-clusters/data/cluster-names.json`. `report`, `where`
and name carrying all read this file.

### Indexing one feature configuration

rust-analyzer indexes one cargo feature set at a time. Code behind features
that are off is not in the graph. For Delos, the default is `desktop`; the
web build is a different graph:

```sh
$T index . -o $O/web.scip --features web --no-default-features
$T graph $O/web.scip -o $O/web-graph.json
# 667 nodes, 2459 edges, 6015 references; packages: cargo delos 0.1.0
```

A new feature set can take minutes the first time (it took 2.5 min here),
because cargo has to build that configuration's build scripts and proc
macros.

### Answering "where does this live?"

With the Delos model's names carried onto the run (see
[Re-running after code changes](#re-running-after-code-changes)):

```sh
$T where $O/clusters.json --names $O/names.json src/expr.rs
# file src/expr.rs: C10 Expression parser & builder (27), C1 Graph document & graph editor (14), C3 Parameter schema & parametric kinds (1)
$T where $O/clusters.json --names $O/names.json Gpu::upload
# engine/gpu/Gpu::upload  ->  C7 GPU backend & engine services
```

The query is a case-insensitive substring of a file path or an item key.
`--limit N` (default 50) prints at most N file lines and at most N item
lines, each followed by `… K more files` / `… K more items (use --limit)`
when cut. `where` exits 1 when nothing matches.

### Reviewing a diff by cluster

```sh
git diff --name-only main... -- src | while read f; do
  $T where $O/clusters.json --names $O/names.json "$f"
done
```

Real output on Delos, with `HEAD~3 HEAD` in place of `main...`:

```text
file src/engine/gpu.rs: C7 GPU backend & engine services (20), C4 Region-of-interest render contract (1)
file src/engine/mod.rs: C4 Region-of-interest render contract (19), C7 GPU backend & engine services (9), C8 Worker, source images & projects (1)
file src/engine/pool.rs: C11 (19)
```

Integration test files are not graph nodes, so `where tests/x.rs` finds
nothing. Restrict the diff to source directories as above.

### Re-running after code changes

Cluster ids change between runs. Carry the old names onto the new clusters
by member overlap:

```sh
$T analyze --out-dir $O/new -r 1.0 \
  --carry-names system-models/code-clusters/data/clusters.json \
                system-models/code-clusters/data/cluster-names.json
```

Each entry in the new `names.json` records its closest old cluster in
`_hint.closest_old`. A name is copied when the overlap is at least 0.5.
Review every entry whose overlap is below about 0.7, and name the empty
ones. Against the Delos model, three of 13 clusters need review (output of
the [CI script](#ci-flag-drift-in-the-cluster-model) below):

```text
C8: closest old C8 'Worker, source images & projects' overlap 0.644
C9: closest old C10 'GPU residency' overlap 0.535
C11: closest old C5 'GPU backend & engine services' overlap 0.26    (left unnamed)
```

To carry with a different threshold, add `--threshold 0.6` (or use
`$T names $O/new/clusters.json --carry-from OLD_CLUSTERS OLD_NAMES --threshold 0.6 -o $O/new/names.json`).

`analyze` without `--carry-names` keeps an existing `names.json` in the
output directory untouched and labels `report.md` with it. Its cluster ids
belong to the run that wrote it, so after code changes use `--carry-names`
instead.

### CI: flag drift in the cluster model

refgraph has no "check" command. This script carries names onto a fresh
run and fails when a cluster has drifted. It needs rust-analyzer in the CI
image.

```sh
T=system-analysis-tools/code-clusters/refgraph
M=system-models/code-clusters/data
$T analyze --out-dir /tmp/cl -r 1.0 --carry-names $M/clusters.json $M/cluster-names.json
python3 - /tmp/cl/names.json <<'EOF'
import json, sys
names = json.load(open(sys.argv[1]))["clusters"]
weak = {cid: e["_hint"]["closest_old"] for cid, e in names.items()
        if e["_hint"]["closest_old"]["overlap"] < 0.7}
for cid, old in sorted(weak.items(), key=lambda kv: int(kv[0])):
    print(f"C{cid}: closest old C{old['id']} {old['name']!r} overlap {old['overlap']}")
sys.exit(1 if weak else 0)
EOF
```

Treat a failure as "the model needs a human look", not as a broken build.

## Interpreting results

Read `report.md` top to bottom. Each section answers one question.

**Header.** Check the node count and the packages. A tiny node count means
the wrong root, the wrong `--package`, or an indexer problem. Note the
modularity and stability; quote both whenever you quote the clusters.

| Modularity | Reading |
|---|---|
| < 0.3 | Little structure. Clusters are not meaningful. |
| 0.3–0.5 | Some structure; describe clusters loosely. |
| > 0.6 | Strong structure (Delos: 0.67). |

| Stability | Reading |
|---|---|
| ≥ 0.95 | Boundaries are firm. |
| 0.9–0.95 | Core is firm; boundary items may move. |
| < 0.9 | Treat boundaries as soft. Try a different resolution or `--weighting`. |

**Partition vs. file layout.** Compare the `clusters` row with `files`,
`dirs` and `prefix@N`. If clusters keep many more references inside than
the folders do, the folders do not describe the structure. For Delos,
clusters keep 80% of references inside, files 55%, directories 78%.
`prefix@N` rows identical to another row, or with a single group, are left
out.

**Clusters.** For each cluster: size, approximate lines, cohesion, files,
hubs, interface, neighbours and members.

- Cohesion ≥ about 0.8 with a short interface: a black box. Review it at
  the boundary.
- Cohesion around 0.5–0.6: a cluster that leans heavily on its
  neighbours. Read the interface list to see what crosses.
- One cluster spanning several directories (UI → worker → format) is a
  feature slice; reason about it as a unit.
- Name clusters from the hubs and interface, not from the file names.

**Seams.** The heaviest cluster pairs. A change touching both sides of a
heavy seam will spread. The items that carry a seam are in the clusters'
interface lists and in the bridge table.

**Bridge items.** Items linked to many clusters, such as `engine/geom/Size`
(193 references, 6 clusters) in Delos. They are assigned to *some* cluster
but belong to many. Do not describe a bridge item as "part of" its home
cluster without saying so. Changing one is expensive.

**Files split across clusters.** A file with many items in two or more
clusters is a natural split point (for example `engine/kernels.rs: C5 34 ·
C12 14 · C7 7 · C3 1`). One stray item is usually noise.

**Test references per cluster.** Which integration-test files reference
each cluster, and how much. A cluster with few or none deserves a note.
Check for inline unit tests before calling it untested; those are not
counted.

Pitfalls:

- Comparing cluster ids across runs. Ids are ordered by size and only mean
  something within one `clusters.json`. Use carried names.
- Reporting one resolution as "the" structure without the sweep. Show
  that it sits on a plateau.
- Reading `~lines` as exact. It sums item spans; types count only their
  definition, not their methods' bodies (the methods count separately).
- Reading "references" as pure source references. The totals include the
  structural owner and trait-impl weights.

## Limitations and accuracy

How the graph is built, and where that goes wrong:

- **Attribution.** Each reference is attributed to the innermost item
  definition whose enclosing range contains it. References outside any
  item (`use` lines, module-level attributes) are ignored. If the indexer
  does not emit `enclosing_range` on definitions, nothing can be
  attributed; refgraph warns.
- **Folding.** Fields, enum variants, items nested inside functions, and
  impls of *external* traits (`Display`, `PartialEq`, …) fold into their
  parent. Associated functions and constants in `impl` blocks become
  `Type::name` items. A reference to a field counts as a reference to its
  type. Impls of the project's own traits stay as separate methods.
- **Self-references** (an item referencing itself) are dropped.
- **Dynamic dispatch** shows up as a reference to the trait method. The
  trait-impl edges then link that declaration to every implementation, so
  trait-heavy designs pull implementations together.
- **Trait-impl matching is by simple name.** A method of trait `T` links
  to every node whose last component is `T::method`. Two project traits
  with the same name in different modules would be linked together (false
  positive).
- **Impls in another module.** Item keys for impl members use the module
  of the `impl` block (`y/Foo::new` for `impl Foo` in `y`). Their owner,
  and the type that impl blocks and external-trait impls fold into, is the
  type's definition in the same package (`x/Foo`), found by simple name.
- **Owner resolution.** If the package defines several types with that
  simple name, none in the impl's module, the impl's module is used
  instead: an external-trait impl then becomes a separate node (`y/Foo`
  next to `x/Foo`), and a method's owner edge is dropped unless `y/Foo`
  exists as a node (false negative). Delos has no impls outside their
  type's module.
- **Generics.** `impl Foo<T>` maps to `Foo`; type parameters and
  parameters are not nodes.
- **Features and cfg.** One index sees one configuration. On Delos the web
  build has 667 nodes against 778 for desktop.
- **Macros.** References are only as good as the indexer's view of macro
  calls. Code generated by macros without source tokens is not visible.
  Types generated by attribute macros can appear with `lines: 0` (in Delos,
  the Dioxus `#[component]` props structs such as
  `ui/inspector/NodePanelProps`).
- **Tests.** Test files (default globs) are outside the graph. Inline test
  modules (keys matching `(^|/)tests?(/|::|$)`) are dropped and their
  references are not recorded at all, so they appear neither as code nor
  in the test map. Test helpers under `src/` that do not match the regex
  appear as ordinary clusters.
- **Louvain is a heuristic.** Near-tied partitions exist. refgraph sorts
  nodes and uses fixed seeds, so results are deterministic for a given
  graph and networkx version. Small code changes can still move boundary
  items. Against the one-off scripts that built the Delos model, refgraph
  builds the same 2,883 edges; mapping each new cluster to the old cluster
  most of its members came from, 741 of 778 items agree.
- **Fixed thresholds.** Bridges need at least 15 references; the top 40
  are stored and the top 15 shown. Hubs and interface keep the top 12.
- **Other languages.** Only rust-analyzer output has been tested. The
  symbol mapping follows the SCIP grammar, but item rules (such as
  `impl#[Self][Trait]`) are rust-analyzer conventions.

## Troubleshooting

| Message or symptom | Cause | Fix |
|---|---|---|
| `refgraph index only drives rust-analyzer (no Cargo.toml found in …)` (exit 2) | `index`/`analyze` root has no `Cargo.toml` | Pass the Cargo root, or run your language's SCIP indexer and use `graph` or `analyze --index`. |
| ``rust-analyzer not found on PATH (try `rustup component add rust-analyzer`).`` (exit 2) | No rust-analyzer | Install it, or put it on `PATH`. |
| rust-analyzer output followed by a non-zero exit | rust-analyzer failed (bad workspace, unknown feature) | Read the output printed on stderr. Use `-v` to see it on success too. |
| `refgraph: clustering needs networkx. …` (exit 1) | Launched with a Python that lacks networkx | Run through uv (`./refgraph` or `uv run`), or `pip install networkx`. |
| `env: uv: No such file or directory` (exit 127) | Launcher run directly without uv | Install uv, or `python3 …/refgraph`. |
| `warning: N definitions had no enclosing range and were skipped; …` | The indexer does not emit `enclosing_range` | Use an indexer version that does. The graph is incomplete otherwise. |
| `0 nodes, 0 edges, 0 references; packages: ` | Wrong `--package` name, `--exclude` too broad, or an empty index | `--package` takes the package *name* (`delos`), not `cargo delos 0.1.0`. |
| Very few nodes | Wrong root, or most code behind features that are off | Check `meta.packages`; index with the right `--features`. |
| `no item or file matches 'X'` (exit 1) | `where` found nothing | Item keys use `/` and `::` and no `src/`; file paths start with `src/`. Test files are not nodes. |
| `kept existing …/names.json (ids may be stale; use --carry-names to remap)` | `analyze` found a names file from an earlier run | The report is labelled with it. If the code changed since, use `--carry-names` or delete it. |
| `refgraph: PATH: No such file or directory` (exit 2) | Wrong input path | Check the path and the argument order (`where CLUSTERS QUERY`). |
| Python traceback `json.decoder.JSONDecodeError` (exit 1) | Input is not a JSON file | Check the argument order. |
| `refgraph: invalid --exclude-symbols regex '…': …` (exit 2) | Invalid `--exclude-symbols` regex | Fix the regex; quote it for the shell. |
| `error: argument -r/--resolutions: invalid float value: 'graph.json'` (exit 2), or a root path taken as a feature | `-r/--resolutions` and `--features` take one or more values and swallow what follows | Put positionals first: `sweep graph.json -r 0.5 1`, `index . --features web`. |
| Test files show up as clusters | Project's test layout does not match the default globs | Pass `--tests GLOB` (repeatable; replaces the defaults). Check `meta.test_globs`. |
| Generated or vendored code dominates | It is in the index | `--exclude 'path/*'` (repeatable). |

## Development

### Module map

| Path | Role |
|---|---|
| `refgraph` | Launcher: uv script header (Python ≥ 3.10, networkx), adds its directory to `sys.path`, calls `cli.main`. |
| `refgraph_lib/cli.py` | argparse parser and one `cmd_*` function per subcommand; file I/O and logging. |
| `refgraph_lib/scip.py` | Dependency-free protobuf decoder for the SCIP fields refgraph needs. |
| `refgraph_lib/symbols.py` | SCIP symbol grammar (`split_symbol`, `parse_descriptors`) and the descriptor → item mapping (`item_for`). |
| `refgraph_lib/graph.py` | Index → reference graph: attribution, structural edges, test references, coarsening. |
| `refgraph_lib/cluster.py` | networkx conversion, seeded Louvain, sweep, cluster metrics, bridges, layout comparison. |
| `refgraph_lib/names.py` | Names template, carrying names by Jaccard overlap, name lookup. |
| `refgraph_lib/report.py` | Markdown report. |
| `tests/test_refgraph.py` | Unit tests over a synthetic SCIP index built in memory. |

### Running the tests

From the project root:

```sh
uv run --with networkx python -m unittest discover -s system-analysis-tools/code-clusters/tests
# Ran 12 tests … OK
python3 -m unittest discover -s system-analysis-tools/code-clusters/tests
# Ran 12 tests … OK (skipped=2)   (clustering tests need networkx)
```

The tests encode a tiny SCIP index by hand, so they need no indexer.

### Extending

- **Another language.** Run its SCIP indexer and feed the index to `graph`
  or `analyze --index`. If items come out wrong, extend `item_for` in
  `symbols.py`; it is the only place that knows rust-analyzer's descriptor
  shapes. To let `index` drive the indexer, add a branch in `cmd_index`
  (`cli.py`) keyed on a marker file, as it does for `Cargo.toml`. Add a test
  that builds a small index with the helpers in `tests/test_refgraph.py`.
- **A new command.** Add a `cmd_<name>(a)` function and a subparser in
  `parser()` with `set_defaults(fn=cmd_<name>)`. Return the exit code.
  Write data to stdout or `-o`, and progress to stderr through `_log`.
  Import networkx-dependent modules inside the function so other commands
  keep working without it.
- **New metrics.** Add fields to the dict returned by `cluster.cluster()`
  and render them in `report.markdown()`. Keep the existing keys stable;
  other tools and the model's explainer read them. Bump the `format`
  string if you change a key's meaning.
- **New SCIP fields.** Extend the decoders in `scip.py` (field numbers are
  in [`scip.proto`](https://github.com/sourcegraph/scip/blob/main/scip.proto)).

### Design decisions

- **SCIP, not a parser.** A compiler-grade index resolves names, macros
  and imports correctly. Text-based call graphs do not.
- **Own protobuf decoder.** Avoids a protobuf dependency and generated code;
  only five message fields are needed.
- **Items, not files.** Files are an author's choice; items are what
  reference each other. `--granularity file` exists to compare.
- **`1 + ln(n)` weighting.** Damps loops that call one function many times,
  so one hot call site does not dominate.
- **Standard modularity for choosing and reporting.** The best run is
  chosen and reported at resolution 1, so numbers compare across
  resolutions.
- **Many seeds, best kept, stability reported.** One Louvain run is not
  evidence; the agreement across runs is.
- **Names are separate from clusters.** Clusters are regenerated; names are
  human judgment. Carrying names by overlap keeps the judgment across runs
  and shows where it no longer fits.
- **Networkx imported lazily.** Graph building and reading the outputs work
  with a bare Python.
