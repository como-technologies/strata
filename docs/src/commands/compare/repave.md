# Ask Palette if the change repaves

The `repave` level comes from a table, and Palette does not use that table. For
a cluster, Palette answers the question itself. `--repave` sends it the
change from what the cluster runs to a profile version, and prints the answer
beside the level. One side must be a cluster, and the other a profile version
that Palette holds, in either order:

```text
{{#include ../../output/repave-kubeadm-lines.txt}}
```

The `palette` line gives Palette's reason: the code, the pack, and the config
keys that differ. Strata does not compare the values: Palette does. The key
`kubeadmconfig.postKubeadmCommands` is on its list, and a change of a key that
is not on it repaves nothing. A change of the CSI values does not:

```text
{{#include ../../output/repave-csi-lines.txt}}
```

Palette checks the OS layer and the Kubernetes layer. Each reason has one of
four codes:

| Code | The pack of the layer |
| --- | --- |
| `PackUpgraded` | has another version |
| `PackValuesUpdated` | has another value at a key on the list |
| `PackAdded` | is in the change, and not on the cluster |
| `PackRemoved` | is on the cluster, and not in the change |

Strata prints the code that Palette gives, and makes no code of its own. The
live runs show `PackValuesUpdated`. The other three codes come from the code of
Palette, and no live run shows them.

`--json` carries the answer as `paletteRepave`, with `required`, each reason,
and `agrees`:

```text
{{#include ../../output/repave-kubeadm-json.txt}}
```

`agrees` is `false` when the level says `repave` and Palette says no, or when the
level says `cluster-wide`, `in-place`, `sync`, or `none` and Palette says
repave. A level of `may-repave` says that a repave is possible, so it agrees
with both answers. When the two disagree, the report adds a `note`. Here the
Kubernetes pack moves to another version, and Palette answers as it does for an
Edge cluster, where that change repaves nothing:

```text
{{#include ../../output/repave-disagree-lines.txt}}
```

That note is the signal that the table is wrong for the environment, or for the
change. AWS and GCP, the two clouds that the runs used, have one upgrade
engine. One live run on AWS moved the OS pack to another registry, at one
version and with the same values. Palette said no repave, and the cluster kept
its nodes. The table takes that result: a new pack reference at one version
does not repave. See
[A pack that moves between registries](../../internals/palette/packs.md#a-pack-that-moves-between-registries).

The report compares the version with the profile of the cluster that the version
replaces, and not with each profile of the cluster. The request to Palette names
each profile that the cluster runs, and the version in the place of that one.
See
[Compare a profile version with a cluster](clusters.md#compare-a-profile-version-with-a-cluster).

`--repave` costs two requests, the cluster document and one `POST` to
`/v1/spectroclusters/<uid>/validate/repave`. The `POST` asks a question, and
the cluster keeps its instances. For a cluster or a change with the pack
`virtual-machine-orchestrator`, `spectro-vm-dashboard`, or
`vm-migration-assistant`, Palette also saves the address of the console of that
pack. Other values of the pack move that address until the agent of the cluster
reads the cluster again: see
[What the route saves](../../internals/palette/validate-routes.md#what-the-route-saves).

The comment of a pull request asks the same question. See
[Ask Palette if the change repaves](../../automation/pr-comments.md#ask-palette-if-the-change-repaves).

## Ask about the update that waits

When an update waits for a cluster, give `cluster:<cluster>@pending` as the
other side. Palette then says whether the cluster repaves when it takes the
update:

```text
{{#include ../../output/edited-repave-lines.txt}}
```

The update is an edit in place, so each profile keeps its uid. For each profile
that the update changes, strata sends each pack of the update by name, with its
values. It also sends each pack that is new to the cluster. Palette removes a
pack of such a profile when the request does not name it. For each other
profile, strata sends only the uid. See
[what Palette builds from the request](../../internals/palette/validate-routes.md#post-v1spectroclustersuidvalidaterepave).

When the record of the update holds a profile that the cluster does not run
now, the record is older than the cluster. The command is correct, so the error
is `stale-record` and not `usage`. Strata stops before the `POST`. Record the
update again:

```text
{{#include ../../output/edited-stale-record.txt}}
```

## A cluster in a cluster template

Palette compares nothing on the route of the cluster when a cluster template
governs the cluster. The cluster document names the template in
`spec.clusterTemplate.uid`. The answer of the route looks like "no repave", but
Palette did not compare the change, so strata does not send that request. For
such a cluster, `--repave` asks the route of the template,
`POST /v1/spectroclusters/clusterTemplates/<uid>/validate/repave`, and a first
`palette` line names the template:

```text
{{#include ../../output/repave-template-lines.txt}}
```

The request names the profiles by uid, with no pack values, and sends
`isUpgradeNow: false`. `--json` names the template in `clusterTemplate`:

```text
{{#include ../../output/repave-template-json.txt}}
```

The route of the template takes no pack values. For a version that the cluster
runs now, Palette compares the copy of the cluster with itself, and the answer
says nothing. So strata stops before the `POST` with the error
`cluster-template-governs`:

```text
{{#include ../../output/repave-template-governs.txt}}
```

The same applies to an update that waits and changes a profile: the update
keeps the uid of each profile. When the update changes nothing, strata asks
the route of the template, and Palette says no repave.

[The lock of a cluster template](../../internals/palette/profiles.md#the-lock-of-a-cluster-template)
gives the rule of Palette.

## Usage errors

Two profile versions are a usage error, because Palette answers for one
cluster. So are two clusters, other than a cluster and the update that waits
for it. So are an edge host and a cluster group: Palette answers for a
cluster, and not for them. Strata reads the refs alone and refuses the pair
before any request, so the run needs no key. A record file shows its side when
strata reads the file, and that read is no request:

```text
{{#include ../../output/repave-usage.txt}}
{{#include ../../output/edited-repave-usage.txt}}
```

A cluster and an update that waits, `cluster:<a>` and `cluster:<b>@pending`,
can name two clusters. A name and a uid can name one cluster, so only the reads
show it. Strata then gives the same `usage` error after the reads.
