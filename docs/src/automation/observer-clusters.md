# The observer of a set of clusters

One observer can watch a set of clusters. It runs on one cluster, and it reads
the content of each cluster of the set from the API of Palette. It makes no
connection to another cluster. With no `--clusters`, the observer watches the
cluster where it runs: see [The observer](./observer.md).

For each cluster of the set, the page answers "Does Palette still have the
version that I recorded?", "Does this cluster run its release?", and "Does the
registry have newer versions of what we pin?". The read of the pods answers
"Which digest does each tag run as on this cluster?" for the cluster where the
observer runs, and for no other cluster.

<div class="playground-demo">

`just observer-playground observer-clusters-1` serves the page of a set of three clusters in two
projects, on this machine, with no key and no cost. `just playground observer-clusters-1` serves the
same tenant alone.

</div>

## The selectors

`--clusters SELECTOR` names the set. Give it once or more. One value can hold one
selector on each line.

| Selector | It selects |
| --- | --- |
| `tag:KEY=VALUE` | the clusters with that label |
| `tag:KEY` | the clusters that have the key |
| `tag:A=B+C=D` | the clusters with each of the labels |
| `filter:NAME` | the clusters that a saved tag filter of Palette selects |

- **The set is the union of the selectors.** It holds each cluster once, by its uid.
- **Palette owns the language of a condition.** A condition that a selector cannot
  hold goes into a saved tag filter: see [Find the fleet from Palette](../use-cases/fleet.md).
- **`--cluster-uid` names the cluster where the observer runs.** It is always in
  the set. With `--clusters` it is optional, and `--read-pods` needs it.

## The keys

The Secret `strata-observer-key` holds one key for each project. The entry
`api-key` is the key of the project of the ConfigMap, `PALETTE_PROJECT_UID`. Each
other project has one entry, with the uid of the project as its name. Each key
belongs to a dedicated user with the role Project Viewer in its project.

```bash
kubectl -n strata-observer create secret generic strata-observer-key \
  --from-file=api-key=<(printf %s "$DEV_KEY") \
  --from-file=<prod project uid>=<(printf %s "$PROD_KEY")
```

- **Each run of strata gets the key of its project alone.** A record with no
  project was taken at tenant scope, and the key `api-key` checks it.
- **`filter:` needs a role of the tenant.** The saved tag filters belong to the
  tenant. Their two routes need `filter.list` and `filter.get` there, as Tenant
  Viewer gives them. The code of Palette shows it; no live run shows it yet. With
  Project Viewer alone, use `tag:`. See [The reach of a
  search](../internals/palette/clusters.md#the-reach-of-a-search).

> **Give the Secret to nobody who must not read each of its projects.** Whoever
> reads the Secret reads each project in it.

## Install with kubectl

Add the component and the key `clusters` of the ConfigMap to the steps of
[Install with kubectl](./observer.md#install-with-kubectl). The component adds the
flags `--clusters` and `--keys`, and the volume of the keys. It adds no role and no
egress rule.

```yaml
components:
  - deploy/observer/clusters
```

```bash
kubectl -n strata-observer create configmap strata-observer \
  --from-literal=cluster-uid=<cluster uid> \
  --from-literal=palette-api=https://api.spectrocloud.com \
  --from-literal=project-uid=<project uid> \
  --from-literal=clusters=$'tag:team=payments\ntag:team=search'
```

## The records

The pipeline records each profile version that a cluster of the set runs. Two
projects can hold one name and version with two uids, so `--set` keys each record
by its uid:

```text
{{#include ../output/observer-clusters-records.txt}}
```

The script refuses two records of one uid. The records fit in four Secrets of
900000 bytes. Above that, the script exits `2` with `the records do not fit in four
Secrets` and the key of the record. `--first-seen` records one cluster, and not a
set.

A record names the scope of its profile. A profile of the tenant that a request of
`dev` records keeps the uid of `dev`, and its scope is `tenant`:

```text
{{#include ../output/observer-clusters-scope.txt}}
```

## Run it once

The set of the chapter holds `payments` and `search` of `dev`, and a second
`payments` of `prod`. Each cluster runs `aws-stack@1.1.1` of the tenant. Each
`payments` also runs `platform-addons@1.0.0` of its own project:

```text
{{#include ../output/observer-clusters-once.txt}}
```

The summary also lists each profile version that a cluster of the set runs, by its
uid, with its copies:

```text
{{#include ../output/observer-clusters-shared.txt}}
```

The screen `#shared` of the page shows the same rows. A row of project scope also
names its project, as `project · dev`, so the two rows of `platform-addons@1.0.0` differ.

A person changes the copy of `prod` on its cluster. The row of that cluster says
`differs`, and the shared version names the cluster whose copy differs:

```text
{{#include ../output/observer-clusters-differs.txt}}
```

## What a cycle does

1. **Find the clusters.** Strata searches each selector in the project of each key,
   with `strata list clusters --project`. A key with a role in one project cannot
   search the tenant. The code of Palette shows it; no live run shows it yet.
2. **Check each record once.** `verify` checks the records of each project one time.
   A profile version that ten clusters run is one check.
3. **Read each cluster.** The observer reads the cluster, and compares each record
   that it runs, by its uid. The raw answers of a cluster go before the next read.

The versions cycle runs `behind` one time for each key. A click "Ask Palette" runs
`behind --pin` with the key of the project of the record, and it waits for one
project of a versions cycle at most.

## The status of a cluster

A row of the screen Clusters is `not-checked` only when the observer could not check
its cluster:

- the key of its project was refused,
- the last read of the cluster failed, or
- no read of the cluster ended well in two periods of the content cycle.

A cause of "Not checked" changes no status. The row of a cluster that the observer
checked has the status of its content, and the row names the count of its causes.
The screen does not say "Nothing changed" while a cluster has such a cause.

## When a read fails

- **A refused key** leaves the clusters of its project `not-checked`, with the status.
  The checks of the other projects continue.
- **The page is `lost`** when no check of any project ended well.
- **A selector that selects no cluster** shows "No cluster matches" with the
  selector. That state is not calm.
- **A cluster that leaves the set** leaves the page at the next cycle, with its
  findings. The alerts send no clear message for it, and the log has one line.

[The routes of a set](../reference/observer-api.md#the-routes-of-a-set) gives the
screens and the fields. The checks of
[observer-clusters.sh](../development/live-runs.md#observer-clusterssh) state what a
live run must show.
