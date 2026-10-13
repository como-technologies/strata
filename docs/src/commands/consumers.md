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
project reads as a name and a uid, and as the uid alone when strata could not
read the project listing.

## A version that no cluster uses

```text
{{#include ../output/retire-free.txt}}
```

Exit `0`. Use the exit status as a gate before a delete:

```text
{{#include ../output/retire-gate.txt}}
```

See [Retire a version](../use-cases/retire.md).

## Edge hosts

A profile bound to an edge host is a consumer too. The consumer list of a
profile names clusters only, so strata reads each edge host the key can list:

```text
{{#include ../output/edge-consumers.txt}}
```

An edge host line ends with `edge host`. The last line says how many edge hosts
strata read. See [A profile runs on edge hosts](../use-cases/edge-hosts.md).

## Cluster templates

A cluster template that links the version is a consumer too, also when no
cluster runs the version. In `dev`, the template `aws-standard` links
`aws-stack@1.1.1`:

```text
{{#include ../output/retire-template.txt}}
```

Exit `1`. A template line ends with `cluster template`. A template has no state,
and it keeps no copy of the version. The profile document names each template,
so a template costs no request.

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

The consumer list of the app profile stands alone: strata does no second
reading, and reads no edge host. The two last lines say why. See
[An app profile is updated in place](../use-cases/app-profiles.md#5-find-the-app-deployments-before-a-delete).

## A cluster that the list does not name

The consumer list of Palette can leave out a cluster that runs the version.
Here the virtual cluster `tenant-b` runs `platform-addons@1.0.0`, and the list
names `tenant-a` alone:

```text
{{#include ../output/group-consumers.txt}}
```

Exit `1`. The second reading asks the cluster search of Palette for the
clusters that run the profile. It sends one search with `clusterProfiles` equal
to the name of the profile. When the search takes `environment`, it sends a
second one that also has the environment `nested`, as `list clusters` does. The
search takes a name, and not a uid, so it names the
clusters that run any version of the profile.

The search goes to the scope of the profile: its project, or the tenant for a
profile of the tenant. The search leaves out each virtual cluster unless a
condition asks for it. Strata then reads each cluster that the search names and
the list does not. It keeps the clusters whose copy is the version, with the
state and the version of each. The cluster listing names no virtual cluster, so
a reading of the listing cannot find `tenant-b`.

The search does not name a virtual cluster that runs the version through its
cluster group either: here `tenant-b` runs `platform-addons` because `dev-group`
pins it. So strata also lists the cluster groups, keeps each group that pins the
version, and reads each of its virtual clusters that the list and the search do
not name. That is how strata finds `tenant-b`. See
[What the cluster listing answers](../internals/palette/clusters.md#what-the-cluster-listing-answers).

## The list of Palette alone

`--list-only` reads the consumer list that Palette computes, and no other
cluster. It costs one request for each consumer. The second reading costs one
request for each 50 clusters that the search names, one for each cluster that
the list does not name, two for the listings of the cluster groups, one for each
cluster group, and one for each 50 virtual clusters of a group that pins the
version. When the search of Palette takes no
`clusterProfiles`, the second reading costs one request for each cluster of the
fleet:

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
  key can read. So strata does a second reading. It asks the cluster search, in
  the scope of the profile, for the clusters that run a version of the profile
  by its name, the virtual clusters too. Then it lists the cluster groups of
  the tenant and of the scope of the key, and keeps each group that pins the
  version: the search does not name a virtual cluster that runs the version
  through its group. It reads each cluster that the search or such a group
  names and the list does not. A cluster that applies the version and is not
  in the list joins the answer, marked `not in the consumer list of the profile`.
- When the search of Palette takes no `clusterProfiles`, or Palette has no
  search, strata reads each cluster of the cluster listing for the profiles it
  applies. The listing names no virtual cluster.
- The last line says how far the second reading got:
  `second reading: the cluster search names 1 other cluster with the profile, 1 read`
  from the search, with `; 1 cluster group pins the version, 1 other virtual cluster, 1 read`
  when a group pins it, `second reading: 3 of 3 clusters read` from the listing, or
  `second reading: not done` with the reason.
- Strata then searches the edge hosts, at tenant scope, and at the scope of
  the key when it cannot read the tenant. An edge host that the version is
  bound to joins the answer with `kind: edgehost`. The line
  `edge hosts: 2 read` says how many it read, or
  `edge hosts: not read` with the reason. A Palette without the search has no
  edge hosts.
- The cluster templates come from `status.inUseClusterTemplates` of the
  profile, in every project, also the projects that the key cannot read. Each
  one joins the answer with `kind: template`, its uid, name, and project, and no
  state and no version.
- A cluster that Palette is deleting stays in the list with the state
  `Deleting`, and the exit is `1`. After the deletion, the list drops it. See
  [What the cluster listing answers](../internals/palette/clusters.md#what-the-cluster-listing-answers).
- When strata finds no consumer, and a reading failed, a consumer can be
  missing. The reading that failed is the second reading, a cluster group, a
  cluster that the search or the listing names, or the edge host search. The
  first line then says `no consumer found, and a reading failed`, and the exit
  is `2`. The last lines name the reading. Palette gives an empty consumer list
  on an error too, so an empty list alone is not "nothing uses it".
- For an app profile, the names and the projects come from
  `status.inUseApps` of the app profile. Strata then reads each app deployment
  in its own project, for its state and the version that its copy names. An app
  deployment that strata cannot read shows `unknown` and the error, and counts.
  Palette lists app deployments by name, cluster, and tag, and not by profile,
  so strata does no second reading. No edge host binds an app profile, so
  strata reads no edge host.

| Exit | Meaning |
| --- | --- |
| `0` | Nothing uses the version, and each reading succeeded. The profile lists no cluster and no cluster template. The second reading finds no cluster that applies it, and no edge host binds it. For an app profile: the app profile lists no app deployment. |
| `1` | A cluster, an edge host, a cluster template, or an app deployment uses the version, whatever else failed. |
| `2` | Strata could not answer: it found no consumer, and a reading failed. A consumer can be missing, so do not delete the version. |

`--format markdown` writes a table with the version that each cluster runs, and
marks each edge host, each cluster template, and each app deployment.
`--json` writes [`strata-consumers/1`](../reference/documents.md#strata-consumers1).

## Options

```text
{{#include ../output/help-consumers.txt}}
```

`--format`, `--json`, `--compact`, `--color`, `--no-color`, and `-q`
(`--quiet`) are the output options that each report command takes: see
[Output formats](./index.md#output-formats).
