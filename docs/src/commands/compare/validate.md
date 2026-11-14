# Ask Palette if it accepts the packs

A profile version can pass all checks on its own and fail on a cluster. For
example, a pack of the version can need a newer version of a pack that another
profile of the cluster holds. Palette checks the packs of a change of the
profiles of a cluster before it starts the update. `--validate` sends it the
change from what the cluster runs to a profile version, and prints the answer.
The sides follow the rules of [`--repave`](repave.md): one side must be a
cluster, and the other a profile version that Palette holds, in either order.

Here the Kubernetes pack moves to 1.36.3, and that version needs a newer CNI
pack than the cluster runs:

```text
{{#include ../../output/validate-refuses-lines.txt}}
```

Each `palette    refuses` line gives one error of Palette: the code, the pack,
and the sentence of Palette. Strata does not check the packs: Palette does.
The answer does not change the result of the comparison: here it exits `1`
because the roots differ. When Palette gives no error, the line is `accepts`:

```text
{{#include ../../output/validate-accepts-lines.txt}}
```

`accepts` covers the packs of the move, and not each rule of the update: see
[What the route does not check](#what-the-route-does-not-check).

`--json` carries the answer as `paletteValidation`, with `accepts` and each
error:

```text
{{#include ../../output/validate-refuses-json.txt}}
```

Palette can also refuse the whole request, for example for a pack that the
cloud of the cluster does not support. It answers HTTP `500` with a `code`,
such as `PackSpectroIntegrationNotSupported`, and a `message`. Strata does not
send that request again. It prints the refusal as one `refuses` line with no
pack: `palette    refuses: <code>: <message>`. Each code is in
[Refusals that strata does not send again](../../reference/errors.md#refusals-that-strata-does-not-send-again).
One refusal comes as HTTP `403`, `PackValuesOverrideForbidden`, and strata
prints it the same way. Another HTTP error, for example a `403` with another
code, or a `500` with another code, is an error of strata and exits `2`.

It costs two requests, the cluster document and one `POST` to
`/v1/spectroclusters/<uid>/validate/packs`, with the body of `--repave`. The
`POST` asks a question and changes nothing.
[The validate routes](../../internals/palette/validate-routes.md#post-v1spectroclustersuidvalidatepacks)
states what Palette checks.

A profile version that the cluster runs goes to another route. For a profile that
the cluster runs, the route of the cluster keeps the tag of the copy that the
cluster holds, and it checks no sent tag: see
[The sent tag](../../internals/palette/validate-routes.md#the-sent-tag). So
`--validate` sends the packs of the version to
`POST /v1/clusterprofiles/<uid>/validate/packs`. Palette checks them as an
update of the version, against each cluster that runs it. Here the cluster
runs the version, so the comparison is `IDENTICAL`, and Palette still refuses
its packs:

```text
{{#include ../../output/validate-running-lines.txt}}
```

The answer holds the errors for each cluster that runs the version. The message
of an error names the clusters. `clusterUid` in `--json` names the cluster that
you asked about. The cost is the same: the cluster document and one `POST`.

Two profile versions, or two clusters, are a usage error, because Palette
answers for one cluster. So are an edge host and a cluster group. Strata refuses
the pair before any request, as `--repave` does:

```text
{{#include ../../output/validate-usage.txt}}
```

For two records with no cluster, the error names the commands to use: see
[Usage errors](repave.md#usage-errors) of `--repave`.

One command can take `--repave` and `--validate`: strata prints the two
answers.

## What the route does not check

The route checks the packs. Palette checks two more rules only at the update,
and no validate route checks them. So strata checks them for the move. It prints
a `note` line under the answer of Palette for each one that it finds. Palette is
the owner of each rule: see
[The gaps that strata fills](../../internals/palette/profiles.md#the-checks-of-an-update).

- **A required variable with no default value.** The body of the route holds no
  value of a variable. Palette refuses an update that gives such a variable no
  value, with `InvalidVariable` and HTTP 500. Strata names each such variable
  that the profile version adds. The move of the cluster `payments` to
  `aws-stack@1.1.1` adds `platform_owner`: see
  [Compare a profile version with a cluster](./clusters.md#compare-a-profile-version-with-a-cluster).
- **A move of Kubernetes that skips a minor version.** Palette refuses the
  update with `K8sUpgradeNotSupported` and HTTP 400. Here the cluster `search`
  runs Kubernetes 1.34.9, and `aws-stack@1.1.1` pins 1.36.2:

```text
{{#include ../../output/validate-skip-lines.txt}}
```

The line names the next minor version. `compare` reads no registry, so it names
no version of that minor version. `strata behind` on a record of the cluster
names one: see
[A move that skips a minor version](../behind.md#a-move-that-skips-a-minor-version).

`--json` gives the two checks in `updateChecks`: see
[`strata-diff/1`](../../reference/documents.md#strata-diff1). The checks send no
request, and they do not change the exit.
