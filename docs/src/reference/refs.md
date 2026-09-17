# Refs

A ref names the thing that a command reads.

| Form | Reads | Example |
| --- | --- | --- |
| `<uid>` | the profile version with this uid | `64e0000000000000000000c3` |
| `<name>@<version>` | the profile version with this name and version in the scope | `aws-pxk@1.1.1` |
| `<name>@latest` | the highest version of the name in the scope | `aws-pxk@latest` |
| `pack:<name>@<version>` | one pack version in a registry, with the values that the registry ships | `pack:csi-aws-ebs@1.65.0` |
| `pack:<registry>/<name>@<version>` | the same, from the registry of that name or uid | `pack:Public Repo/csi-aws-ebs@1.65.0` |
| `pack:<name>@latest` | the highest version of the pack in the registry | `pack:kubernetes@latest` |
| `cluster:<cluster>` | each profile that the cluster runs, in the order that Palette applies them | `cluster:payments` |
| `cluster:<cluster>/<profile>` | one profile that the cluster runs, by its name or uid | `cluster:payments/aws-pxk` |
| a file | a snapshot file from `strata snapshot` | `releases/1.1.1/dev.json` |

`strata list` prints the refs of a scope.

## Rules

- **Scope.** Strata reads a uid, a name, or a cluster in the configured project.
  See [Configure](../configure.md#the-project). A snapshot file carries its own
  project.
- **`@latest`.** The highest version by natural order: digit runs compare as
  numbers, other text as text. `22.04` is above `20.04`, and `1.30.6` is above
  `1.29.10`. The snapshot records the version that it found in
  `source.version`, and keeps `aws-pxk@latest` in `source.ref`.
- **`pack:`.** A pack version, read from the registry, not from a profile. Its
  snapshot holds that one pack with the values that the registry ships, and no
  manifests or params: those belong to a profile. Comparing two of them says
  what changed between two versions of a pack. See
  [What changed between two pack versions](../use-cases/pack-versions.md).
- **`<registry>`.** The name or the uid of a registry. Without one, strata uses
  the registry that has the pack version. When several do, the error
  `pack-in-several-registries` lists them, because registries hold copies of
  the same pack.
- **`<cluster>`.** A uid, or the name of one cluster in the scope. A name that
  two clusters have is the error `cluster-name-ambiguous`, which lists their
  uids. A name that no cluster has is `cluster-not-found`, which lists the
  names.
- **`<profile>`.** The name or the uid of a profile that the cluster runs. An
  unknown one is `cluster-profile-not-found`, which lists the profiles of the
  cluster.
- **A file.** Strata checks for a file first: a ref that names a file that
  exists is a file. A file that is not a strata snapshot is `not-snapshot`.

## Which commands take which refs

| Command | uid, `name@version`, `name@latest` | `pack:` | `cluster:` | file |
| --- | --- | --- | --- | --- |
| `compare` | yes | yes | yes | yes |
| `snapshot` | yes | yes | yes | no: `--from-raw DIR` or `--from-candidate FILE` |
| `candidate` | yes | no | no | a profile record |
| `same-release` | yes | yes | yes | yes |
| `verify` | no | no | no | yes: a profile, cluster, or pack record |
| `consumers` | yes | no | no | a profile record |
