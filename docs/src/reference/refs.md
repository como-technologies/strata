# Refs

A ref names the thing that a command reads.

| Form | Reads | Example |
| --- | --- | --- |
| `<uid>` | the profile version with this uid | `64e0000000000000000000c3` |
| `<name>@<version>` | the profile version with this name and version in the scope | `aws-stack@1.1.1` |
| `<name>@latest` | the highest version of the name in the scope that is not a pre-release | `aws-stack@latest` |
| `<name>@<version>@draft` | the draft of the profile version: what its publish gives | `aws-stack@1.1.1@draft` |
| `<uid>@draft` | the draft of the profile version with this uid | `64e0000000000000000000c3@draft` |
| `pack:<name>@<version>` | one pack version, when one registry has that name and version | `pack:kubernetes@1.36.2` |
| `pack:<registry>/<name>@<version>` | the same, from the registry of that name or uid, as `strata list registries` names them | `pack:Public Repo/csi-aws-ebs@1.65.0` |
| `pack:<name>@latest` | the highest version of the pack that the registry serves and that is not a pre-release | `pack:kubernetes@latest` |
| `cluster:<cluster>` | each profile that the cluster runs; beside a profile version in `compare`, the profile that the version replaces | `cluster:payments` |
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
- **`@latest`.** The highest version that is not a pre-release, by Semantic
  Versioning, as Palette orders versions. `22.04` is above `20.04`, and `1.30.6`
  is above `1.29.10`. A pre-release is below its version. A name that has only
  pre-releases gives its highest one. The snapshot records the version that it
  found in `source.version`, and keeps `aws-stack@latest` in `source.ref`.
  - `@latest` names the version that it takes in a note on standard error. A
    profile and an app profile get `@latest is` with the ref of that version,
    such as `appprofile:shop@1.0.0`, and a pack gets `recording pack`. When
    `@latest` skips a pre-release above that version, the note names the
    highest one. It says `a pre-release by SemVer` for a version that parses,
    such as `1.0.1-rc.1`. It says `a pre-release` for each other form, such as
    `1.2.develop`, because the order below, and not SemVer, makes it one. See
    [Record the newest version](../commands/snapshot/profile-version.md#record-the-newest-version).
  - A text that does not parse reads as its first three numbers, as Palette
    reads it: see [The order of versions](../internals/palette/profiles.md#the-order-of-versions).
    The numbers decide first.
  - Palette gives no order to two texts with one set of numbers, and it reads
    no version in a text with no three numbers. There, strata follows the order
    of the Rust crate `versions` 8.0.1. A test compares the two orders.
  - For one set of numbers, the forms go from the lowest to the highest:
    1. Text before the numbers, as in `chart-1.2.0`.
    2. One or two numbers and a tail of text, as in `1.2rc1` and `1.2.develop`.
       A missing number is `0`, so `1.2.develop` is above `1.1.10`. Text right
       after the third number is such a tail too, as the crate reads it:
       `1.2.0rc1b` is `1.2` and the tail `0rc1b`.
    3. A tail of text after a dot after the three numbers, as in
       `1.2.0.develop`.
    4. A pre-release, as in `1.2.0-rc.1`.
    5. The version, `1.2.0`.
    6. A fourth number, as in `1.2.0.1`. So `10.4.0.1` is above `10.4.0` and
       below `10.5.0`. Text right after the fourth number puts the version below
       it: `1.2.0.1rc1` is below `1.2.0.1`. A tail after `-` or a dot puts the
       version above it, as the crate orders it: `1.2.0.1` is below `1.2.0.1-rc1`.
  - A part between dots is a number when it holds only digits, as the crate
    reads it. In the last part, the digits in front of letters are a number too,
    as in `2rc1`. So `1.2rc1` holds two numbers, and `1.2a1b` holds one number
    and the tail `2a1b`. The three numbers that Palette reads still decide
    first, so `1.2.3rc1b` is above `1.2.develop`.
  - Of two texts with one form, the crate orders the tails. For each count of
    numbers, text right after the last number is below a tail after a dot. So
    `1.2rc1` is below `1.2.develop`, and `1.2.3rc1b` is below `1.2.3.develop`.
    Digits with letters right after them are below the digits alone, so
    `1.2.0.1rc1` is below `1.2.0.1`. Build data after `+` does not count.
  - The tail `0rc1b` of `1.2.0rc1b` is a tail after a dot, as the crate reads
    it. So `1.2rc1` is below `1.2.0rc1b`, and `1.2.0rc1b` is below
    `1.2.develop`.
  - Each form with text is a pre-release. So `1.1.8.develop` is below `1.1.8`,
    and `@latest` gives `1.1.10` over `1.2.0.develop`. `@latest` on
    `1.2.develop`, `1.10.0`, and `1.9.0` gives `1.10.0`. A tail of numbers, as
    in `10.4.0.1`, is no pre-release.
  - Palette reads `1.1.8.develop` as `1.1.8`, above `1.1.8-alpha`. Strata puts
    it below `1.1.8-alpha`, as the crate does.
  - The rules of Palette stay where the crate orders in another way. A `v` in
    front does not count, so `v1.1.8.1` equals `1.1.8.1`. A missing number is
    `0`, so `1.2` equals `1.2.0`. Palette finds the numbers at any place, so
    `chart-1.2.3` is above `1.2.2`.
  - Palette also ignores the text before the numbers. Of two texts with the
    same text before the numbers, the rest decides by these rules. So
    `chart-1.2.3.develop` is below `chart-1.2.3-alpha`, and `chart-1.2.3` is
    above both. The crate reads `1.2.3-alpha` there as a pre-release of the text
    `chart`, and puts `chart-1.2.3` below both.
  - Two different texts before the numbers keep the natural order of that text,
    so `app-1.2.3` is below `chart-1.2.3`.
  - The crate gives no total order to `1.2.3rc1` and `1.0.0-a_b`: it puts
    `1.2.3rc1` equal to `1.2.3` and to `1.2.3-rc.1`. Strata puts such a text
    below each pre-release of its numbers.
  - The crate gives no total order to digits and text in a part that is not the
    last, as in `1.2rc1.x` and `1.2.3.4rc1.x`. Strata reads such a part as text,
    so `1.2.3.4rc1.x` is below each pre-release of `1.2.3`.
  - The crate gives `1.2rc` the same rank as `1.2rc1` and `1.2rc2`, but it puts
    `1.2rc1` below `1.2rc2`. Strata puts letters with no number after them
    below the same letters with a number: `1.2rc` is below `1.2rc1`.
  - A text with no number at its start and no three numbers, such as `develop`,
    `LTS__24.x`, or `1a1b`, holds no version. It is below each version.
    `@latest` gives such a text only when the name has no version.
  - Of two texts with no version, digit runs compare as numbers and other text
    as text. So `LTS__24.x` is above `LTS__22.04`.
- **`@draft`.** The draft template of a profile version, and not its published
  template. A person who edits a published version and does not publish makes
  a draft. The record holds the values and the manifests of the draft, and
  `source.template` is `draft`. A version with no draft is the error
  `no-draft`.
  - `@latest@draft` reads the draft of the highest version. Put `@draft` last.
  - `verify` of the record compares it with the published version, so it
    verifies after the publish.
  - See [Review a draft before its publish](../use-cases/draft.md) and
    [Drafts](../internals/palette/profiles.md#drafts).
- **`pack:`.** A pack version, read from the registry, not from a profile. Its
  snapshot holds that one pack with the values that the registry ships, and no
  manifests or params: those belong to a profile. Comparing two of them says
  what changed between two versions of a pack. See
  [What changed between two pack versions](../use-cases/pack-versions.md).
- **`<registry>`.** The name or the uid of a registry. Without one, strata uses
  the registry that has the pack version. When several do, the error
  `pack-in-several-registries` lists them. Two registries can hold two builds
  under one version, so the ref must name one. `strata list --pack <name>`
  names the registry that each profile version takes the pack from, and a
  record names it in `packs[].registryName`.
- **`<cluster>`.** A uid, or the name of one cluster in the scope. A name that
  two clusters have is the error `cluster-name-ambiguous`, which lists their
  uids. A name that no cluster has is `cluster-not-found`, which lists the
  names. A record lists the profiles of a cluster in one order, whatever order
  Palette gives them: see
  [The order of the profiles of a cluster](../internals/palette/clusters.md#the-order-of-the-profiles-of-a-cluster).
- **`<profile>`.** The name or the uid of a profile that the cluster runs. An
  unknown one is `cluster-profile-not-found`, which lists the profiles of the
  cluster. `+` joins two or more: the record holds those profiles.
  - `compare` selects the profile when the ref names none. Beside a profile
    version, `cluster:<cluster>` holds only the profile that the version
    replaces, as `cluster:<cluster>/<profile>` does. Beside a cluster group, it
    holds the profiles that the group pins. See
    [Compare a profile version with a cluster](../commands/compare/clusters.md#compare-a-profile-version-with-a-cluster).
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
- **A file.** Strata checks for a file first. A ref that names a file that
  exists is a file. A file that is not a strata snapshot is `not-snapshot`.
  - A ref that is shaped like a path is a file too, whether or not the file is
    there. A path has one of these shapes:
    - It holds a `/` or a `\`.
    - It starts with `.`, `..`, or `~`.
    - It ends with `.json`, `.yaml`, or `.yml`.
  - A `pack:`, `cluster:`, `edgehost:`, `tag:`, or `filter:` ref can hold a
    `/`, and each one stays what its prefix names.
  - A uid, a name, and a `name@version` hold none of these shapes, so they
    reach Palette.

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

`snapshot --raw-dir` takes a profile or a cluster. A record that strata builds
again from the directory does not say `@pending`, `@draft`, edge host, cluster
group, or app profile.
