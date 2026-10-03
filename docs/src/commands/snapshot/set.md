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

## The report as JSON

`--json` writes the report to standard output as a
[`strata-set/1`](../../reference/documents.md#strata-set1) document. It has one
entry for each cluster of the set, with its file and its root or its error, and
each removed file:

```text
{{#include ../../output/fleet-set-json.txt}}
```

`--format markdown` writes a table for a pull request comment, and `--compact`
writes the JSON on one line. The exit code is the same in each format. An
error of the query itself, such as a filter that does not exist, writes no
document. With `--json --quiet`, standard error then gets one JSON line: see
[Errors](../../reference/errors.md).
