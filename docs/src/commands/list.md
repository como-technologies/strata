# strata list

List the profile versions, clusters, and projects in the scope. The first
column of each line is a ref that the other commands accept.

The examples use the projects `dev` and `prod`, with `PALETTE_PROJECT=dev`.

## List the refs in the project

```text
{{#include ../output/list.txt}}
```

Profiles sort by name, then by version in natural order, so `1.9.0` comes
before `1.10.0`. Clusters and projects sort by name.

## List the projects

```text
{{#include ../output/list-projects.txt}}
```

Strata always reads the projects at tenant scope, so the list holds each
project that the key can see.

## List another project

```text
{{#include ../output/list-prod.txt}}
```

`--project-name <name>` lists another project by its name, and `--project
<uid>` does the same with the uid. `--project ''` is tenant scope.

## List the clusters of every project

```text
{{#include ../output/list-tenant.txt}}
```

`--project ''` is tenant scope. A cluster line then ends with its project. A
cluster ref works only in the project of the cluster: set `PALETTE_PROJECT`
before you use it. See [The same names in two projects](../use-cases/same-names.md).

## Find the clusters of a label

```text
{{#include ../output/fleet-tag.txt}}
```

`--tag KEY=VALUE` keeps the clusters with that label, and `--tag KEY` the
clusters that have the key. Give it again for each label: each must hold. A tag
of the provider is a label: `env:prod` is `env=prod`. Palette searches every
project the key can read, so the listing is at tenant scope, and each row names
its project and ends with the labels of the cluster.

`--filter NAME` keeps the clusters that a saved tag filter selects, by its name
or uid. `strata list filters` names each saved tag filter of the tenant, and
what it selects:

```text
{{#include ../output/fleet-filters.txt}}
```

Strata reads the search schema of the tenant first. When the search takes no
`tags`, or Palette has no search, strata reads the cluster listing and matches
the labels itself, with the same rules, and the heading says so. The search
lists no virtual cluster. `strata snapshot filter:NAME --dir DIR` writes a record
of each cluster: see [Find the fleet from Palette](../use-cases/fleet.md).

## Find the profiles a cluster runs

`strata consumers <name>@<version>` names the clusters that use a profile
version. `--applied` answers the opposite question, for each cluster in the
scope:

```text
{{#include ../output/list-applied.txt}}
```

Each line under a cluster is one profile the cluster applies, with its uid and
its type, in the order the cluster document lists them. That order is no install
order: see
[The order of the profiles of a cluster](../internals/palette/clusters.md#the-order-of-the-profiles-of-a-cluster). A cluster with an add-on profile
shows each one: see
[A cluster does not run its release](../use-cases/cluster-release.md).

A profile whose line ends with `behind: an update waits` has a change that the
cluster has not taken: Palette says an update waits for it. See
[A cluster record says whether an update waits](./snapshot/cluster.md#whether-an-update-waits).
With `--format markdown`, the `behind` column says `yes` and names the
profiles, `no`, or `-` when Palette did not say.

`--applied` reads each cluster and its notifications, two requests for each.
Read the uid when you
need the identity. A published version can be updated after its publish, and
then one `<name>@<version>` means two contents at two times. A legacy version
bump can overwrite a uid, and then one uid names two versions. A cluster that strata cannot read
stays in the list with its error.

## Ask about one cluster

`--cluster <text>` keeps the clusters whose name or uid holds the text, in
upper or lower case. It reads only those clusters, and it lists clusters only,
unless you name the kinds:

```text
{{#include ../output/list-cluster.txt}}
```

`--cluster` sets `--applied`, as `--pack` sets `--packs`: a person who names one
cluster asks what that cluster runs.

At tenant scope the text can match a cluster in more than one project, and each
one is an answer:

```text
{{#include ../output/list-cluster-tenant.txt}}
```

The two clusters have one name. A name is not an identity: two clusters with
one name can also run one version and different content. See
[The same names in two projects](../use-cases/same-names.md).

## Find the profile versions that carry a pack

A pack is not a ref. `--pack <text>` keeps the profile versions with a pack
whose name contains the text, in upper or lower case:

```text
{{#include ../output/list-pack.txt}}
```

In a terminal, the part of each pack name that matched is marked, as a search
tool marks a match. A pipe, a file and `--no-color` get the plain name.

`--pack` reads each profile version, one request for each. It also lists only
profiles, unless you name the kinds.

`--packs` reads the same packs with no filter, so every profile version shows
what it carries:

```text
{{#include ../output/list-packs.txt}}
```

A pack under another tag name shows the tag: `24.04 (tag LTS__24.x)`. A version
that strata cannot read stays in the list with its error, because strata
cannot tell if it carries the pack.

[Find the profiles that carry a pack](../use-cases/find-a-pack.md) searches
every project.

## A version that nobody published

A version that nobody published has its packs in its draft. `--packs` and
`--pack` read the packs of the draft, and the row of the version ends with
`draft`:

```text
{{#include ../output/list-draft.txt}}
```

The JSON gives `published` for each version that strata read:

```text
{{#include ../output/list-draft-json.txt}}
```

With `--format markdown`, the ref cell of the version ends with `draft`. A
published version with a draft that waits shows its published packs, with no
mark. A record of a version that nobody published holds content that a person
can change before the publish. See
[Drafts](../internals/palette/profiles.md#drafts).

## What to expect

Name the kinds to list: `strata list profiles`, or `strata list clusters
projects`. With no kind, strata lists profiles and clusters. Palette lists each
kind differently by scope:

| Scope | Profiles | Clusters | Projects |
| --- | --- | --- | --- |
| A project | the profiles of the project | the clusters of the project | each project the key can see: strata reads the projects at tenant scope in every scope |
| Tenant scope | the profiles of the tenant only | the clusters of every project | each project that the key can see |

`--tag` and `--filter` always search at tenant scope, and list clusters alone.
`strata list filters` reads the saved tag filters, which belong to the tenant.
Each listed cluster carries its labels in the JSON.

Exit `0`, or `2` when strata cannot read a listing. `--json` writes
[`strata-list/1`](../reference/documents.md#strata-list1).

## Options

```text
{{#include ../output/help-list.txt}}
```

`--format`, `--json`, `--compact`, `--color`, `--no-color`, and `-q`
(`--quiet`) are the output options that each report command takes: see
[Output formats](./index.md#output-formats).
