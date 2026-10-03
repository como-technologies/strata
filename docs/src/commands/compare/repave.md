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

Two profile versions, or two clusters, are a usage error, because Palette
answers for one cluster. So are an edge host and a cluster group: Palette
answers for a cluster, and not for them:

```text
{{#include ../../output/repave-usage.txt}}
```

## A cluster in a cluster template

Palette does not answer on the route of the cluster when a cluster template
governs the cluster. The cluster document names the template in
`spec.clusterTemplate.uid`, and the answer of the route is empty. An empty
answer is not "no repave", so strata does not print it. For such a cluster,
`--repave` asks the route of the template,
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

[The lock of a cluster template](../../internals/palette/profiles.md#the-lock-of-a-cluster-template)
gives the rule of Palette.
