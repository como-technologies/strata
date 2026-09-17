# Find the profiles that carry a pack

## The problem

A pack version has a security problem, for example a Cilium release. You need
each profile version that carries the pack, in each project, and the clusters
that run those versions. A pack is not a ref, so `compare` cannot find it.

## 1. Search one project

```text
{{#include ../output/list-pack.txt}}
```

The text matches a part of the pack name, in upper or lower case. Each version shows its
consumers and its packs.

## 2. Search every project

```text
{{#include ../output/list-pack-all.txt}}
```

`strata list projects` gives the names for the loop.

## 3. Get one line for each match

```text
{{#include ../output/list-pack-json.txt}}
```

## 4. Find the clusters

For each version with the pack, list its clusters:

```text
{{#include ../output/list-pack-consumers.txt}}
```

Exit `1` names a version that a cluster still uses. See
[strata consumers](../commands/consumers.md).

## What to do

- Publish a new version with a fixed pack, and move the clusters to it.
- Then [retire](./retire.md) the old versions.
