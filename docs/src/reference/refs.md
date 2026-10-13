# Refs

A ref names the thing that a command reads.

| Form | Reads | Example |
| --- | --- | --- |
| `<uid>` | the profile version with this uid | `64e0000000000000000000c3` |
| `<name>@<version>` | the profile version with this name and version in the scope | `aws-stack@1.1.1` |
| `<name>@latest` | the highest version of the name in the scope | `aws-stack@latest` |
| `<name>@<version>@draft` | the draft of the profile version: what its publish gives | `aws-stack@1.1.1@draft` |
| `<uid>@draft` | the draft of the profile version with this uid | `64e0000000000000000000c3@draft` |
| `pack:<name>@<version>` | one pack version, when one registry has that name and version | `pack:kubernetes@1.36.2` |
| `pack:<registry>/<name>@<version>` | the same, from the registry of that name or uid | `pack:Public Repo/csi-aws-ebs@1.65.0` |
| `pack:<name>@latest` | the highest version of the pack in the registry | `pack:kubernetes@latest` |
| `cluster:<cluster>` | each profile that the cluster runs | `cluster:payments` |
| `cluster:<cluster>/<profile>` | one profile that the cluster runs, by its name or uid | `cluster:payments/aws-stack` |
| `cluster:<cluster>/<profile>+<profile>` | some of the profiles that the cluster runs | `cluster:payments/aws-stack+platform-addons` |
| `cluster:<cluster>@pending` | the profiles that the cluster runs after it takes the update that waits | `cluster:payments@pending` |
| `edgehost:<host>` | each profile bound to an edge host | `edgehost:store-1` |
| `edgehost:<host>/<profile>` | one profile bound to the edge host | `edgehost:store-1/platform-addons` |
| `clustergroup:<group>` | the profiles that a cluster group pins for its virtual clusters | `clustergroup:dev-group` |
| `appprofile:<uid>` | the app profile with this uid | `appprofile:64f6000000000000000000a1` |
| `appprofile:<name>@<version>` | the app profile with this name and version, or `@latest` | `appprofile:shop@1.0.0` |
| `tag:<key>=<value>` | the clusters with the label, each in its own project; `+` joins labels that must all hold | `tag:env=prod` |
| `filter:<name>` | the clusters that a saved tag filter selects, by its name or uid | `filter:production` |
| a file | a snapshot file from `strata snapshot` | `releases/1.1.1/dev.json` |

`strata list` prints the refs of a scope.

## Rules

- **Scope.** Strata reads a uid, a name, or a cluster in the configured project.
  See [Configure](../configure.md#the-project). A snapshot file carries its own
  project.
- **`@latest`.** The highest version by natural order: digit runs compare as
  numbers, other text as text. `22.04` is above `20.04`, and `1.30.6` is above
  `1.29.10`. The snapshot records the version that it found in
  `source.version`, and keeps `aws-stack@latest` in `source.ref`.
- **`@draft`.** The draft template of a profile version, and not its published
  template. A person who edits a published version and does not publish makes
  a draft. The record holds the values and the manifests of the draft, and
  `source.template` is `draft`. A version with no draft is the error
  `no-draft`. `@latest@draft` reads the draft of the highest version. Put it
  last. `verify` of the record compares it with the published version, so it
  verifies after the publish. See
  [Review a draft before its publish](../use-cases/draft.md) and
  [Drafts](../internals/palette/profiles.md#drafts).
- **`pack:`.** A pack version, read from the registry, not from a profile. Its
  snapshot holds that one pack with the values that the registry ships, and no
  manifests or params: those belong to a profile. Comparing two of them says
  what changed between two versions of a pack. See
  [What changed between two pack versions](../use-cases/pack-versions.md).
- **`<registry>`.** The name or the uid of a registry. Without one, strata uses
  the registry that has the pack version. When several do, the error
  `pack-in-several-registries` lists them. Two registries can hold two builds
  under one version, so the ref must name one.
- **`<cluster>`.** A uid, or the name of one cluster in the scope. A name that
  two clusters have is the error `cluster-name-ambiguous`, which lists their
  uids. A name that no cluster has is `cluster-not-found`, which lists the
  names. A record lists the profiles of a cluster in one order, whatever order
  Palette gives them: see
  [The order of the profiles of a cluster](../internals/palette/clusters.md#the-order-of-the-profiles-of-a-cluster).
- **`<profile>`.** The name or the uid of a profile that the cluster runs. An
  unknown one is `cluster-profile-not-found`, which lists the profiles of the
  cluster. `+` joins two or more: the record holds those profiles.
- **`@pending`.** The profiles that Palette gives for the update that waits for
  the cluster, and not the copies that the cluster runs. When no update waits,
  they are the same. Put it last: `cluster:payments/aws-stack@pending`. See
  [What a waiting update changes](../commands/snapshot/cluster.md#what-a-waiting-update-changes).
- **`<host>`.** A uid, or the name of one edge host in the scope. A name that no
  edge host has is `edge-host-not-found`, which lists the names. A name that two
  edge hosts have is `edge-host-name-ambiguous`. See
  [A profile runs on edge hosts](../use-cases/edge-hosts.md).
- **`<group>`.** A uid, or the name of one cluster group in the scope. A name
  that no group has is `cluster-group-not-found`, which lists the names. A name
  that two groups have is `cluster-group-name-ambiguous`. See
  [A cluster group pins a profile](../use-cases/cluster-groups.md).
- **`tag:` and `filter:`.** A set of clusters, and not one object. Only
  `snapshot --dir DIR` takes one: it writes a record of each cluster. Another
  command refuses it with `usage`. A saved filter that no filter of the tenant
  names is `filter-not-found`, which lists the filters. A name that two filters
  have is `filter-name-ambiguous`, which lists their uids. See
  [Find the fleet from Palette](../use-cases/fleet.md).
- **`appprofile:`.** An app profile. Its tiers are the packs of the record. A
  name and version that no app profile has is `app-profile-not-found`, which
  lists the app profiles. A name and version that two app profiles have is
  `app-profile-name-ambiguous`, which lists their uids. See
  [An app profile is updated in place](../use-cases/app-profiles.md).
- **A file.** Strata checks for a file first: a ref that names a file that
  exists is a file. A file that is not a strata snapshot is `not-snapshot`.
  A ref that is shaped like a path is a file too, whether or not the file is
  there. A path holds a `/` or a `\`. A path starts with `.`, `..`, or `~`. A
  path ends with `.json`, `.yaml`, or `.yml`. A `pack:`, `cluster:`,
  `edgehost:`, `tag:`, or `filter:` ref can hold a `/`, and each one stays what
  its prefix names. A uid, a name,
  and a `name@version` hold none of these shapes, so they reach Palette.

A record that is missing or misspelled is the error `io`, which names the file.
Strata asks Palette nothing:

```text
{{#include ../output/refs-missing-file.txt}}
```

## Which commands take which refs

| Command | uid, `name@version`, `name@latest`, and each with `@draft` | `pack:` | `cluster:`, `edgehost:`, `clustergroup:`, `appprofile:` | `tag:`, `filter:` | file |
| --- | --- | --- | --- | --- | --- |
| `compare` | yes | yes | yes | no | yes |
| `snapshot` | yes | yes | yes | with `--dir` | no: `--from-raw DIR` or `--from-candidate FILE` |
| `candidate` | yes | no | no | no | a profile record |
| `same-release` | yes | yes | yes | no | yes |
| `verify` | no | no | no | no | yes: a record of each kind |
| `consumers` | yes | no | no | no | a profile record |
| `behind` | no | no | no | no | yes: a record of each kind |

`snapshot --raw-dir` takes a profile or a cluster: a record read back from the
directory does not say `@pending`, `@draft`, edge host, cluster group, or app
profile.
