# A macro value changes

## The problem

The values and the manifests of a profile can read a macro, as
`{{.spectro.macro.<name>}}`. A tenant or a project defines the macro, and
Palette gives it its value when it applies the profile. A record holds the text
with the name, so the root covers the name and not the value.

When the value of the macro changes, the published version does not change,
and the copy that a cluster holds does not change. `verify` of each record
reports `OK`. Palette still gives the cluster the new value.

In this example, `dev` has `aws-stack@1.1.4`, whose CSI values read the project
macro `region`, and the cluster `payments` runs it.

**Who runs this.** The scheduled verify job finds this when its records hold
the digests of the macro values. The job is
[Drift issues](../automation/drift-issues.md).

**Follow along.** `just playground macros-1` serves this tenant on this
machine, with no key and no cost. `just playground macros-2` serves it after the
macro changes. Each section names the seed it needs.

## 1. Record the version and the cluster, with the macro values

Record the version as usual. Record the cluster with `--macros`: strata reads
the value that Palette gives each macro the content uses, and keeps its digest.

```text
{{#include ../output/macros-record.txt}}
{{#include ../output/macros-digests.txt}}
```

The record holds `sha256:` of each value, and not the value, because a macro
can hold a secret. The digests are not part of the root. A record of the version
with `--macros` holds the digests of the values at the scope of the version.
See [Macros](../commands/snapshot/cluster.md#macros).

## 2. The macro gets another value

**Follow along.** `just playground macros-2` is the same tenant after a person
gave `region` another value. Stop the first seed, and export what this one
prints. Keep the records from step 1.

The record of the version still verifies. Its report names the macro, because
the root covers the name and not the value:

```text
{{#include ../output/macros-verify-release.txt}}
```

The cluster record with the digests reports the change, and names the macro:

```text
{{#include ../output/macros-verify-cluster.txt}}
```

`verify` exits `1`. The roots are equal: only the value of the macro moved. A
record without `--macros` reports `OK` here, as the version record does.

## What to do

- Record each version, cluster, and cluster group whose content reads a macro
  with `snapshot --macros`, and keep those records under the scheduled verify
  job.
- Find the versions that read a macro: the note `the content uses macro` names
  them in each report.
- Keep macro values out of the fields that repave a cluster, such as
  `kubeadmconfig.postKubeadmCommands`. See the Palette behavior below.
- The `immutable-clusterprofiles` flag of the Spectro Cloud provider covers
  `spectrocloud_cluster_profile` alone: it keeps a published version the same.
  A macro is a resource of its own, `spectrocloud_macros`, and it updates in
  place with the flag on or off. The digests of a record with `--macros` cover
  it.

## Palette behavior

A macro is a value with no version and no audit entry, and a change of it
opens no notification. Palette gives a cluster the new value at the next read
of its agent, in a live run 11 minutes after the change. When the macro is in
a field that repaves the cluster, Palette holds a repave for approval, and
refuses an update of the cluster profiles until a person approves the repave.
A cluster record holds that repave, and `verify` names it: see
[Whether a repave waits](../commands/snapshot/cluster.md#whether-a-repave-waits).
See
[Macros](../internals/palette/profiles.md#macros).
