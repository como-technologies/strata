# Find the profiles that carry a pack

## The problem

A pack version has a security problem, for example a Cilium release. You need
each profile version that carries the pack, in each project, and the clusters
that run those versions. `pack:<name>@<version>` reads the pack itself.
[What changed between two pack versions](./pack-versions.md) compares two pack
versions. Neither says which profiles carry the pack. `list` answers that.

**Who runs this.** A person at a terminal, answering an incident. Nothing here writes.

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

The loop names each project. `strata list projects` gives the names, one for
each line of its own report, and a script reads them from
`strata list projects --json`.

The projects are not the whole tenant. A profile at tenant scope belongs to no
project, so no project listing holds it. A search of the projects alone does not
find it. Read the tenant scope too:

```text
{{#include ../output/list-pack-tenant.txt}}
```

This example keeps its profiles in projects, so the tenant scope holds none. A
profile that belongs to no project is in this listing only.

## 3. Get one line for each match

```text
{{#include ../output/list-pack-json.txt}}
```

## 4. Find the clusters

For each version with the pack, list its clusters:

```text
{{#include ../output/list-pack-consumers.txt}}
```

Exit `1` means that a cluster still uses the version. The rows name each cluster. `consumers` takes no project
option, so the second command names the project in the environment.
`PALETTE_PROJECT_UID` has priority over `PALETTE_PROJECT`, so unset the uid
before you name a project by name. See
[strata consumers](../commands/consumers.md) and [Configure](../configure.md).

## What to do

- Publish a new version with a fixed pack, and move the clusters to it.
- Then [retire](./retire.md) the old versions.
