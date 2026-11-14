# strata consumers

List the clusters, the edge hosts, and the cluster templates that use a profile
version, or the app deployments that use an app profile version. Strata always
reads the list from Palette. Palette refuses the delete of a version while one
of them uses it: see
[What the delete of a version checks](../internals/palette/profiles.md#what-the-delete-of-a-version-checks),
and [App profiles](../internals/palette/other-objects.md#app-profiles).

The first examples use the project `dev`, which holds `aws-stack@1.0.0` and
`aws-stack@1.1.0`. The clusters `payments` and `search` run 1.1.0.

## A version in use

```text
{{#include ../output/retire-in-use.txt}}
```

Exit `1`. Each line gives the name, uid, project, and state of a cluster. The
project reads as its name, and as its uid when strata could not read the project
listing. Strata reads the names from the project listing of the tenant, so a
consumer in another project shows the name of its project too. `--json` gives
the uid in `projectUid` and the name in `projectName`.

## What strata searched

The block `searched:` has one row for each reading, with what it found. A row
gives counts, and no name of a cluster. So no row reads as a consumer: each
consumer is a line above the block.

| Row | What strata read | What the row gives |
| --- | --- | --- |
| `consumer list` | The consumer list that Palette computes for the version. | The clusters and the cluster templates that the list names. |
| `cluster search` | The cluster search, with the name of the profile. | The other clusters that the search names: how many run this version, how many run other versions, and how many strata could not read. |
| `cluster listing` | Each cluster of the cluster listing, in place of the search when the search of Palette takes no `clusterProfiles`. | The other clusters of the listing, and how many run this version. |
| `cluster groups` | The cluster groups that pin the version, and their virtual clusters. | The groups, their other virtual clusters, and how many run this version. |
| `edge hosts` | The edge host search. | `none found` when the search names no edge host, or how many of its edge hosts are bound to the version. |

A row that says `not done` or `not read` gives the reason. The line `scope:`
follows the block: see [A profile of the tenant](#a-profile-of-the-tenant).

## A version that no cluster uses

```text
{{#include ../output/retire-free.txt}}
```

Exit `0`. The cluster search names two other clusters: they run other versions
of `aws-stack`, and not 1.0.0. The search takes the name of the profile and not
its uid, so it names the clusters of each version of the name.

Use the exit status as a gate before a delete:

```text
{{#include ../output/retire-gate.txt}}
```

See [Retire a version](../use-cases/retire.md).

## A profile of the tenant

Only the clusters of its own project can use a profile of a project, as the
line `scope:` says. Each project of the tenant can use a profile of the tenant
or of the system. The searches at tenant scope give only the projects where a
role of the key gives the listing. So for such a profile, strata also searches
each project of the tenant listing for its clusters and its edge hosts.

Here `aws-stack@2.0.0` is a profile of the tenant, which has the projects `dev`,
`prod`, and `staging`:

```text
{{#include ../output/retire-tenant.txt}}
```

Exit `0`. The line `scope:` counts the projects that strata searched. When the
role of the key gives no listing in `staging`, Palette refuses the search of
that project with HTTP 500 and the code `ResourceAccessDenied`:

```text
{{#include ../output/retire-tenant-denied.txt}}
```

Exit `2`. A consumer in `staging` can be missing, so the answer is not "nothing
uses it". The line `NOT CHECKED` names each project that strata could not
search. Palette refuses the delete of a version that a cluster of any project
uses, also of a project that the key cannot read. Give the gate a key that reads
each project of the tenant.

The line can also give two other cases:

- `each project that this key cannot list`: Palette refused the search at tenant
  scope. Then the key has no `project.list` in the tenant, and Palette lists only
  the projects of its roles.
- `each project of the tenant`, with the error: strata could not read the
  project listing, so it searched no project one by one.

`--json` gives the counts in `projectsRead`. `unread` names each project that
strata could not search, with `denied` and the error:

```text
{{#include ../output/retire-tenant-json.txt}}
```

When a cluster of `prod` runs the version, the consumer list names it. The row
names its project by the name, also when the key reads the profile in `dev`:

```text
{{#include ../output/retire-tenant-in-use.txt}}
```

Exit `1`: a consumer decides the exit, also when a project is not checked.

`--list-only` searches no project one by one. See
[The reach of a search](../internals/palette/clusters.md#the-reach-of-a-search).

## Edge hosts

A profile bound to an edge host is a consumer too. The consumer list of a
profile names clusters only, so strata reads each edge host the key can list:

```text
{{#include ../output/edge-consumers.txt}}
```

An edge host line ends with `edge host`. The row `edge hosts` gives how many of
the edge hosts of the search are bound to the version. `none found` says that
the search names no edge host. See [A profile runs on edge hosts](../use-cases/edge-hosts.md).

## Cluster templates

A cluster template that links the version is a consumer too, also when no
cluster runs the version. In `dev`, the template `aws-standard` links
`aws-stack@1.1.1`:

```text
{{#include ../output/retire-template.txt}}
```

Exit `1`. A template line ends with `cluster template`, and the row
`consumer list` counts it. A template has no state, and it keeps no copy of the
version. The profile document names each template, so a template costs no
request.

## App profiles

The consumers of an app profile version are its app deployments. Give the app
profile as `appprofile:<name>@<version>`, `appprofile:<name>@latest`,
`appprofile:<uid>`, or a record of an app profile. When no app deployment uses
the version:

```text
{{#include ../output/app-consumers-free.txt}}
```

Exit `0`. When an app deployment uses it:

```text
{{#include ../output/app-consumers.txt}}
```

Exit `1`. Palette refuses the delete of an app profile while an app deployment
uses it. An app deployment line ends with `app deployment`, and the state is
the state of the deployment, for example `Deployed`. In `--json`, the kind of
the consumer is `app`:

```text
{{#include ../output/app-consumers-json.txt}}
```

The consumer list of the app profile stands alone: strata searches no app
deployment, and reads no edge host. The two last rows say why. See
[An app profile is updated in place](../use-cases/app-profiles.md#5-find-the-app-deployments-before-a-delete).

## A cluster that the list does not name

The consumer list of Palette can omit a cluster that runs the version.
Here the virtual cluster `tenant-b` runs `platform-addons@1.0.0`, and the list
names `tenant-a` alone:

```text
{{#include ../output/group-consumers.txt}}
```

Exit `1`. The row `consumer list` counts `tenant-a` alone. Beside the list,
strata asks the cluster search of Palette for the clusters that run the profile. It sends one search with `clusterProfiles` equal
to the name of the profile. When the search takes `environment`, it sends a
second one that also has the environment `nested`, as `list clusters` does. The
search takes a name, and not a uid, so it names the
clusters that run any version of the profile.

The search goes to the scope of the profile: its project, or the tenant for a
profile of the tenant. The search names no virtual cluster unless a condition
asks for it. Strata then reads each cluster that the search names and
the list does not. It keeps the clusters whose copy is the version, with the
state and the version of each. The cluster listing names no virtual cluster, so
a reading of the listing cannot find `tenant-b`.

The search does not name a virtual cluster that runs the version through its
cluster group either: here `tenant-b` runs `platform-addons` because `dev-group`
pins it. So strata also lists the cluster groups and keeps each group that pins
the version. Then it reads each of the virtual clusters of those groups that the
list and the search do not name. That is how strata finds `tenant-b`, and the
row `cluster groups` counts it. See
[What the cluster listing answers](../internals/palette/clusters.md#what-the-cluster-listing-answers).

## The list of Palette alone

`--list-only` reads the consumer list that Palette computes, and no other
cluster. It costs one request for each consumer.

The search beside the list costs these requests:

- One for each 50 clusters that the search names.
- One for each cluster that the list does not name.
- Two for the listings of the cluster groups.
- One for each cluster group.
- One for each 50 virtual clusters of a group that pins the version.
- For a profile of the tenant or of the system, one for the project listing.
  Then each project gets one search of the clusters and one of the edge hosts.
  When the search takes `environment`, each project gets one more search for
  the virtual clusters.

When the search of Palette takes no `clusterProfiles`, the listing costs one
request for each cluster of the fleet. With `--list-only`, the rows of the
search and of the groups say `not done` and `not read`:

```text
{{#include ../output/consumers-list-only.txt}}
```

A consumer that the list does not name is then not found, and the list is
empty when the read fails. An empty list exits `0` with `--list-only`: no
reading failed, because strata did none. So a gate before a delete runs without
`--list-only`.
The edge hosts are read with it too: the consumer list names clusters alone.
The cluster templates are in it too: the profile document names them.
`strata list --packs` counts the same clusters and templates for each version
of the scope: see
[The consumers of a version](./list.md#the-consumers-of-a-version).

## A cluster that runs an overwritten version

In another project, a version bump overwrote the version that `payments` runs:

```text
{{#include ../output/overwritten-consumers.txt}}
```

The copy of the cluster names `1.1.1`, but the uid names `1.2.0` now. A legacy
version bump or a rename overwrote the version that the cluster runs. See
[A version bump overwrites a version](../use-cases/overwritten.md).

## What to expect

- The ref is a profile uid, `<name>@<version>`, `<name>@latest`, a profile
  record, an app profile ref, or an app profile record. Strata uses the uid and
  the project of a record. A cluster, edge host, cluster group, or pack ref, or
  a record of one, is an error: `profile-ref-required`.
- The names and the projects come from `status.inUseClusters` of the profile.
  Strata then reads each cluster in its own project, for its state and the
  version that its copy names.
- A cluster that strata cannot read shows `unknown` and the error. Strata counts
  it as a consumer, so the exit status is `1`.
- Palette computes `status.inUseClusters` when it is asked, in the projects the
  key can read. It gives an empty list on an error too. So strata also asks the
  cluster search for the clusters that run a version of the profile by its name.
  The search goes to the scope of the profile, and asks for the virtual clusters
  too.
- Strata then lists the cluster groups of the tenant and of the scope of the
  key. It keeps each group that pins the version, because the search does not
  name a virtual cluster that runs the version through its group. It reads each
  cluster that the search or such a group names and the list does not. A
  cluster that applies the version and is not in the list joins the answer,
  marked `not in the consumer list of the profile`.
- When Palette has no search, or its search takes no `clusterProfiles`, strata
  reads each cluster of the cluster listing for the profiles it applies. The
  listing names no virtual cluster.
- Strata then searches the edge hosts, at tenant scope, and at the scope of
  the key when it cannot read the tenant. An edge host that the version is
  bound to joins the answer with `kind: edgehost`. A Palette without the search
  has no edge hosts.
- Each project can use a profile of the tenant or of the system. So strata also
  searches each project of the tenant listing for such a profile. A project
  whose search fails is in the line `NOT CHECKED`.
- The cluster templates come from `status.inUseClusterTemplates` of the
  profile, in every project, also the projects that the key cannot read. Each
  one joins the answer with `kind: template`, its uid, name, and project, and no
  state and no version.
- The block `searched:` gives a row for each reading, and the line `scope:`
  gives the scope of the profile: see [What strata searched](#what-strata-searched).
- A cluster that Palette is deleting stays in the list with the state
  `Deleting`, and the exit is `1`. After the deletion, the list drops it. See
  [What the cluster listing answers](../internals/palette/clusters.md#what-the-cluster-listing-answers).
- When strata finds no consumer, and a reading failed, a consumer can be
  missing. The reading that failed can be the search, the listing, or a cluster
  group. It can also be a cluster that one of them names, the edge host search,
  or the search of a project. The first line then says `no consumer found, and a reading
  failed`, and the exit is `2`. The rows and the line `NOT CHECKED` name the
  reading. An empty consumer list alone is not "nothing uses it".
- For an app profile, the names and the projects come from
  `status.inUseApps` of the app profile. Strata then reads each app deployment
  in its own project, for its state and the version that its copy names. An app
  deployment that strata cannot read shows `unknown` and the error, and counts.
  Palette lists app deployments by name, cluster, and tag, and not by profile,
  so strata searches no app deployment beside the list. No edge host binds an
  app profile, so strata reads no edge host. Strata gives no scope for an app
  profile.

| Exit | Meaning |
| --- | --- |
| `0` | Nothing uses the version, and each reading succeeded. The profile lists no cluster and no cluster template. The search beside the list finds no cluster that applies it, and no edge host binds it. For a profile of the tenant, strata searched each project. For an app profile: the app profile lists no app deployment. |
| `1` | A cluster, an edge host, a cluster template, or an app deployment uses the version, whatever else failed. |
| `2` | Strata could not answer: it found no consumer, and a reading failed or a project was not checked. A consumer can be missing, so do not delete the version. |

`--format markdown` writes a table with the version that each cluster runs, and
marks each edge host, each cluster template, and each app deployment. A second
table gives the rows of `searched:`. The scope is a sentence, and `NOT CHECKED`
is bold.
`--json` writes [`strata-consumers/1`](../reference/documents.md#strata-consumers1).

## Options

```text
{{#include ../output/help-consumers.txt}}
```

`--project <uid>` or `--project-name <name>` reads the version in that project,
in place of the configured project. `--project ''` is tenant scope. Strata
reads a record that names a project in that project. See
[Environments in the projects of one tenant](../use-cases/projects.md#7-can-i-delete-this-version).

`--format`, `--json`, `--compact`, `--color`, `--no-color`, and `-q`
(`--quiet`) are the output options that each report command takes: see
[Output formats](./index.md#output-formats).
