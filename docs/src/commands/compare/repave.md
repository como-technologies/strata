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

That note is the signal that the table is wrong for the environment. No live
run has produced it: AWS and GCP, the two clouds that the runs used, have one
upgrade engine, and give the answer of the table each time. It costs two requests, the cluster document and one `POST` to
`/v1/spectroclusters/<uid>/validate/repave`. The `POST` asks a question and
changes nothing: the cluster keeps its instances.

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
now, the record is older than the cluster. Strata stops with `usage` before the
`POST`. Record the update again.

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
says nothing. So strata stops with a usage error, before the `POST`:

```text
{{#include ../../output/repave-template-usage.txt}}
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
cluster, and not for them:

```text
{{#include ../../output/repave-usage.txt}}
{{#include ../../output/edited-repave-usage.txt}}
```
