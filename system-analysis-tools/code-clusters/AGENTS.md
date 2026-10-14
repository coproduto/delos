# refgraph: instructions for agents

`refgraph` finds the natural **clusters** in a codebase: sets of items
(functions, types, methods) that reference each other heavily and the rest
of the code rarely. Use it to explain a project's structure to a human, to
see which parts a change will touch, and to point out where decisions will
ripple. The human makes the decisions. Your job is to give them an accurate
map.

Terms are defined in [`README.md`](README.md#concepts-and-terminology).
Every option, default, exit code and JSON field is in
[`REFERENCE.md`](REFERENCE.md). Check it before using an option not shown
here.

## Setup

Run everything from the project root through the launcher:

```sh
T=system-analysis-tools/code-clusters/refgraph
O=/tmp/refgraph          # scratch directory outside the repo
mkdir -p $O
```

- With `uv` on `PATH`, nothing else is needed. Without it, use
  `python3 $T …` with networkx installed (`sweep`, `cluster` and `analyze`
  need it; the rest do not).
- Rust projects need `rust-analyzer` (`rustup component add rust-analyzer`).
- Progress goes to stderr; data goes to stdout or to the `-o` file.
- Exit codes: 0 success; 1 for `where` with no match, a missing
  dependency, or a Python traceback (malformed input); 2 for usage errors,
  a file that cannot be opened, an invalid `--exclude-symbols` regex, and
  `index` without `Cargo.toml` or rust-analyzer. See
  [REFERENCE.md › Exit codes](REFERENCE.md#exit-codes).
- Put positional arguments before `--features` and `-r/--resolutions`;
  those options consume every following word.
- Write scratch output outside the repository. The default `--out-dir`
  (`refgraph-out`) is not gitignored at the project root.

## Procedure: build a cluster model

1. **Index.**
   `$T index . -o $O/index.scip`
   About 25 s for Delos on a warm cache; a new feature set can take minutes.
   If cargo features change which code exists (Delos: `desktop` default,
   `web`, `mobile`), ask the human which configuration matters, or index
   each one: `$T index . -o $O/web.scip --features web --no-default-features`.
   For non-Rust projects, run that language's SCIP indexer instead; `index`
   exits 2 when there is no `Cargo.toml`.

2. **Graph.**
   `$T graph $O/index.scip -o $O/graph.json`
   Check the stderr summary (Delos: `778 nodes, 2883 edges, 6981
   references; packages: cargo delos 0.1.0`).
   - A tiny node count means the wrong root, the wrong `--package` (it takes
     the package *name*, e.g. `delos`), or code behind disabled features.
   - A `warning: … had no enclosing range` line means references cannot be
     attributed. The analysis is invalid for that index.
   - Check `meta.test_globs` in `graph.json` against the project's test
     layout. If tests live elsewhere, pass `--tests GLOB` (repeatable; it
     *replaces* the defaults). Remove generated or vendored code with
     `--exclude 'path/*'`.

3. **Sweep.**
   `$T sweep $O/graph.json --json`
   Pick a resolution on the plateau where modularity is near its maximum
   and stability is high. `suggested` is a fine default. Prefer a cluster
   count a human can hold in their head (about 6–15). Delos: 0.6–1.5 is a
   plateau (modularity 0.66–0.67, stability ≥ 0.945); the model uses 1.0.

4. **Cluster.**
   `$T cluster $O/graph.json -r <res> -o $O/clusters.json`
   Record the stderr line (`13 clusters, modularity 0.672, stability 0.960`);
   you will report both numbers.

5. **Read.**
   `$T report $O/clusters.json --members 40 -o $O/report.md`
   Then read the source and module doc comments behind each cluster's hubs
   and interface before naming anything.

6. **Name.**
   `$T names $O/clusters.json -o $O/names.json`, then fill in every entry:
   - `name`: what the cluster *does*, in 2–5 words ("Codecs & export", not
     "formats.rs stuff").
   - `region`: a coarse grouping for display. A low-resolution run
     (`cluster -r 0.4`) gives a data-driven one. The report does not show
     regions.
   - `tags`: short lowercase slugs: layer, domain, and traits such as
     `leaf` or `feature-slice`.
   - `summary`: 1–3 sentences grounded in the members.
   - `_hint` blocks are ignored by every command; keep or delete them.

   Then `$T report $O/clusters.json --names $O/names.json -o $O/report.md`.

7. **Write the model** if asked, for example under `system-models/<name>/`.
   Lead with findings a human can act on (below), not with the method. Keep
   `graph.json`, `clusters.json`, the names file and the exact commands
   next to it so it can be regenerated.

Shortcut: `$T analyze --out-dir $O --keep-index` runs steps 1–5 (and writes
a names template). It takes the same options as `index`, `graph`, `cluster`
and `names --threshold`; `-r` picks the resolution and is added to the
sweep. If `$O/names.json` already exists it is kept and applied to
`report.md` (its ids may be stale after code changes; use `--carry-names`).

## What to look for (the findings humans need)

All in `clusters.json`; field meanings are in
[REFERENCE.md › clusters.json](REFERENCE.md#clustersjson).

- **Clusters vs layout.** In `layout_comparison`, if `clusters.internal` is
  well above `files` and `dirs`, the folders do not describe the structure.
  Say so with the numbers (Delos: 0.80 vs 0.55 and 0.78).
- **Cross-cutting seams.** A trait whose `Trait::method` keys land in
  several clusters means "adding one more X" changes many clusters at once.
  Use `where` on the trait name.
- **Feature slices.** One cluster spanning several files and layers
  (UI → worker → format) can be reasoned about as a unit.
- **Black boxes.** Cohesion ≥ about 0.8 with a short `interface` can be
  reviewed at the boundary.
- **Ripple zones.** The lowest-cohesion clusters and the heaviest `between`
  pairs are where decisions spread. Name the items that carry the seam
  (`interface`, `bridges`).
- **Files that split.** In `file_split`, a file with many items in two or
  more clusters is a natural split point. Say what each part is.
- **Test coverage by cluster.** In `tests`, clusters with few or no test
  references deserve a note. Inline `mod tests` blocks are dropped and not
  counted; check for them before calling anything untested.

Verify every claim before you write it. Grep the code; do not infer from
names alone. State what the analysis cannot see: features that were not
indexed, macro-generated code, dynamic dispatch, runtime wiring.

## Using a model during other work

- **Before changing code:**
  `$T where $O/clusters.json --names $O/names.json <file-or-item>`
  shows which clusters you are entering. File paths look like
  `src/engine/gpu.rs`; item keys look like `engine/gpu/Gpu::upload`. If a
  change touches several clusters or a heavy seam, tell the human before
  you proceed.
- **Reviewing a diff:**
  ```sh
  git diff --name-only main... -- src | while read f; do
    $T where $O/clusters.json --names $O/names.json "$f"
  done
  ```
  Summarise the diff per cluster and call out cluster-crossing changes.
  Test files are not graph nodes, so leave them out of the loop.
- **After significant code changes:** re-run and carry names over:
  ```sh
  $T analyze --out-dir $O/new -r <res> --carry-names OLD/clusters.json OLD/names.json
  ```
  For a threshold other than 0.5, add `--threshold X`.
  Then review each entry's `_hint.closest_old.overlap`. Below about 0.7,
  check whether the old name still fits. Empty names are new or heavily
  changed clusters; name them. For Delos the model's files are
  `system-models/code-clusters/data/clusters.json` and
  `system-models/code-clusters/data/cluster-names.json`.

## Pitfalls

- Cluster ids are not stable across runs. Map them with `names --carry-from`
  or `analyze --carry-names`.
- Do not publish a resolution off the plateau, or a partition without its
  stability, as "the" structure. When stability is below about 0.9, call
  the boundaries soft.
- Totals and "refs" include the structural owner (2) and trait-impl (1)
  weights, not only source references.
- `~lines` is approximate: a type's lines cover its definition only, and
  macro-generated items can show 0.
- Hub types such as `engine/geom/Size` or `ui/state/App` sit in *some*
  cluster but belong to many. Look them up in `bridges` before describing
  one as "part of" a cluster.
- Test helpers under `src/` that do not match the inline-test regex show up
  as ordinary clusters.
