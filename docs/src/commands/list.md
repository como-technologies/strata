# strata list

List the profile versions, clusters, and projects in the scope. The first
column of each line is a ref that the other commands accept.

The examples use the projects `dev` and `prod`, with `PALETTE_PROJECT=dev`.

## List the refs in the project

```text
{{#include ../output/list.txt}}
```

Profiles sort by name, then by version, in the order of Semantic Versioning, so
`1.9.0` comes before `1.10.0`, and `1.0.1-rc.1` before `1.0.1`. Clusters and
projects sort by name.

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
lists no virtual cluster unless a condition asks for it: see
[The virtual clusters](#the-virtual-clusters). `strata snapshot filter:NAME
--dir DIR` writes a record of each cluster: see
[Find the fleet from Palette](../use-cases/fleet.md).

## The virtual clusters

The cluster listing of Palette omits each virtual cluster. Its search
omits them too, unless a condition asks for the environment `nested`, the
cloud type of a virtual cluster. So strata asks the search for them, in the
scope of the listing. The row of a virtual cluster ends with its cluster group:

```text
{{#include ../output/group-list.txt}}
```

The JSON gives `virtual` on each virtual cluster: its cluster group, and the
cluster that hosts it. `includesVirtual` says that strata asked for them:

```text
{{#include ../output/group-list-json.txt}}
```

`--tag` and `--filter` send the search a second time, with the same condition
added. So a virtual cluster that carries the label is in the answer:

```text
{{#include ../output/group-list-tag.txt}}
```

`cluster:<name>` of a virtual cluster resolves through the same search:

```text
{{#include ../output/group-member-by-name.txt}}
```

With `--format markdown`, the column `virtual` names the group. When Palette
has no search, or its search takes no `environment`, the list holds no virtual
cluster, and the heading says so. `consumers` names a virtual cluster from the
consumer list of Palette and from the same search: see
[Cluster groups](../internals/palette/other-objects.md#cluster-groups).

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

## An imported cluster

Palette builds a cluster from a cluster profile. A person can also import a
cluster that runs elsewhere. An imported cluster runs add-on profiles alone,
and in read-only mode it runs none. With `--applied`, the row of an imported
cluster with no profile says why:

```text
{{#include ../output/list-imported.txt}}
```

With `--format markdown`, the `profiles` cell of the row holds the note. The
JSON gives `clusterType` on each cluster, as Palette lists it. `PureManage` is
a cluster that Palette built. `AlloyMonitor`, `AlloyAssist`, and `AlloyExtend`
are clusters that a person imported:

```text
{{#include ../output/list-imported-json.txt}}
```

The listing gives no import mode. The console of Palette shows it, `read-only`
or `full`, and strata does not read it. See
[An imported cluster](../internals/palette/clusters.md#an-imported-cluster).

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
can change before the publish. A live run shows the row, the JSON, and the
published version with a draft that waits: see
[draft.sh](../development/live-runs.md#draftsh). See also
[Drafts](../internals/palette/profiles.md#drafts).

## The consumers of a version

With `--packs`, the row of a version ends with the number of its consumers.
Strata counts what the profile document names: the clusters of its consumer
list, and the cluster templates that link the version. Palette refuses the
delete of the version while one of them uses it. Here the template
`aws-standard` links `aws-stack@1.0.0`, which no cluster runs:

```text
{{#include ../output/list-consumers.txt}}
```

`--json` gives the number in `consumers`, and the count of each kind in
`consumersByKind`:

```text
{{#include ../output/list-consumers-json.txt}}
```

`strata consumers --list-only` reads the same document, so it names the same
consumers. It also searches the edge hosts:

```text
{{#include ../output/list-consumers-same.txt}}
```

`list --packs` costs one request for each version. It reads no edge host, and
it does no second reading. So an edge host that the version is bound to, and a
cluster that the list of Palette omits, are not in the count.
`strata consumers` finds them: see [Edge hosts](./consumers.md#edge-hosts) and
[A cluster that the list does not name](./consumers.md#a-cluster-that-the-list-does-not-name).

## A profile of the tenant

A tenant administrator publishes a profile at tenant scope. Palette lists it
in each project, beside the profiles of the project. So a tenant profile and a
project profile can have one name and one version:

```text
{{#include ../output/scope-list.txt}}
```

The row of a version whose scope is not the scope of the listing ends with its
scope, here `tenant`. The two rows differ in the uid. The JSON gives `scope`
for each version: `project`, `tenant`, or `system`:

```text
{{#include ../output/scope-list-json.txt}}
```

Strata reads the scope from the profile search of Palette. It sends one request
for the tenant versions and one for the system versions, with 50 profile names
on a page. With `--packs` or `--pack`, it reads the scope in the document of each
version, and sends no search. When Palette has no profile search, the JSON has
no `scope`, and no row has the mark. See
[The scope of a version](../internals/palette/profiles.md#the-scope-of-a-version).

A `<name>@<version>` that two scopes answer is an error. `snapshot`, and each
other command that takes the ref, exits `2` with `profile-name-ambiguous`. The
error names each uid with its scope:

```text
{{#include ../output/scope-ambiguous.txt}}
```

Give the uid in place of the name:

```text
{{#include ../output/scope-by-uid.txt}}
```

With `--packs`, the mark ends the row after the consumers. A tenant version
that nobody published shows both marks: `tenant, draft`.

```text
{{#include ../output/scope-packs.txt}}
```

At tenant scope, `--project ''`, the listing holds the profiles of the tenant,
and no row has the mark:

```text
{{#include ../output/scope-tenant.txt}}
```

## What to expect

Name the kinds to list: `strata list profiles`, or `strata list clusters
projects`. With no kind, strata lists profiles and clusters. Palette lists each
kind differently by scope:

| Scope | Profiles | Clusters | Projects |
| --- | --- | --- | --- |
| A project | the profiles of the project and the profiles of the tenant | the clusters of the project | each project the key can see: strata reads the projects at tenant scope in every scope |
| Tenant scope | the profiles of the tenant only | the clusters of every project | each project that the key can see |

`--tag` and `--filter` always search at tenant scope, and list clusters alone.
`strata list filters` reads the saved tag filters, which belong to the tenant.
Each listed cluster carries its labels in the JSON. A list of clusters holds the
virtual clusters of the scope, from the search of Palette. That is one request
for the schema of the search, and one for the search.

Exit `0`, or `2` when strata cannot read a listing. `--json` writes
[`strata-list/1`](../reference/documents.md#strata-list1).

## Options

```text
{{#include ../output/help-list.txt}}
```

`--format`, `--json`, `--compact`, `--color`, `--no-color`, and `-q`
(`--quiet`) are the output options that each report command takes: see
[Output formats](./index.md#output-formats).
