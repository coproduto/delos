# Delos code clusters

A model of the project built from its **reference graph**. Items that
reference each other heavily, and the rest of the code rarely, are grouped
into clusters. The clusters are then named and tagged.

Open `index.html` for the explainer: a cluster map, findings, one card per
cluster, file splits, seams, bridge items and test coverage.

## Contents

| Path | What |
|---|---|
| `index.html` | The explainer |
| `data/graph.json` | Item-level reference graph: 778 nodes, 2,883 weighted edges, plus test-file references |
| `data/clusters.json` | Partition at resolution 1.0 (12 clusters), with metrics, interfaces, bridges, file splits and the test map |
| `data/regions.json` | Coarse partition at resolution 0.4 (7 regions) |
| `data/cluster-names.json` | Names, regions and tags assigned to each cluster id |
| `tools/scip.py` | Dependency-free decoder for rust-analyzer's SCIP protobuf output |
| `tools/build_graph.py` | SCIP index → `graph.json` |
| `tools/cluster.py` | Louvain clustering and metrics (needs `networkx`) |

## Reproduce

```sh
rust-analyzer scip . --output /tmp/delos.scip
python3 system-models/code-clusters/tools/build_graph.py /tmp/delos.scip graph.json
python3 -m venv /tmp/venv && /tmp/venv/bin/pip install networkx
/tmp/venv/bin/python system-models/code-clusters/tools/cluster.py graph.json 0   log            # resolution sweep
/tmp/venv/bin/python system-models/code-clusters/tools/cluster.py graph.json 1.0 log clusters.json
```

`build_graph.py` imports `scip.py` from its own directory. Louvain is
seeded (seeds 0–29, best modularity kept), so the results are deterministic for
a given graph. Cluster ids are ordered by size. If the code changes, the
ids and names in `cluster-names.json` may need to be re-assigned.

## Updating with `refgraph`

The scripts in `tools/` are the one-off originals and they reproduce this
model exactly. The maintained, general version is
[`system-analysis-tools/code-clusters/refgraph`](../../system-analysis-tools/code-clusters/README.md). To
refresh this model after code changes, keeping the cluster names where the
clusters survive:

```sh
system-analysis-tools/code-clusters/refgraph analyze --out-dir /tmp/clusters -r 1.0 \
  --carry-names system-models/code-clusters/data/clusters.json \
                system-models/code-clusters/data/cluster-names.json
```

On the same code, `refgraph` builds an identical graph. Its partition
matches this one on 93% of items (722 of 778): it orders nodes
deterministically, and Louvain's choice between near-tied partitions
depends on node order. The differences are at the soft boundaries
this model already names. The GPU texture `Pool` (`engine/pool.rs`) splits
from the GPU backend into its own cluster, and the `SourceImage` storage
items move next to GPU residency. In the carried names file, check every entry whose
`closest_old.overlap` is below about 0.7.
