# A set of clusters

`tag:<key>=<value>` and `filter:<name>` name a set of clusters: the clusters
with a label, or the clusters that a saved tag filter selects, in each project
the key can read. `--dir DIR` writes one record for each:

```text
{{#include ../../output/fleet-snapshot.txt}}
```

Each record is `<cluster name>.json`, or `<name>.<uid>.json` when two clusters of
the set have one name, and each cluster is read in its own project. `--prune`
removes each cluster record of the directory whose cluster the query no longer
selects, and leaves every other file:

```text
{{#include ../../output/fleet-snapshot-again.txt}}
```

Exit `0`, or `2` when strata could not record a cluster: the line of that
cluster says why, and the other records are written. See
[Find the fleet from Palette](../../use-cases/fleet.md).
