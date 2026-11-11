# A release gets new version numbers

## The problem

Some pipelines publish a candidate first, test it, and then publish the same
content under its final numbers. The pipeline builds each pack again, and only
the version in `pack.json` changes. Then it publishes the final profile version.
Each team names its candidates in its own way, such as `-g<sha12>`, `-rc.1`,
`-mr<iid>.<pipeline>`, or `+build.5`.

The pipeline must make sure that the final version holds the content that
passed its tests. But the version of each pack is in the root, so the roots
differ, and `same-release` fails. `compare` then gives the move of each pin the
impact of a new pack version. For the Kubernetes pack, that is `repave`.

In this example, the pipeline published `aws-stack@1.2.0-rc.1`. It pins the
pre-releases `kubernetes` `1.36.2-rc.1` and `csi-aws-ebs` `1.65.0-rc.1`. The
final version `aws-stack@1.2.0` pins `1.36.2` and `1.65.0`, with the same
values. `aws-stack@1.2.1` is the final version with one more change of a CSI
value. `aws-stack@1.2.2` pins `1.65.1`, a build of the CSI pack from another
commit, with one more replica in its default values. The pack uids and the
digests of the pre-releases are stand-ins.

**Who runs this.** The job of the pipeline that publishes the final version, or
a person at a terminal.

<div class="playground-demo">

`just playground renumber` serves this tenant on this machine, with no key and
no cost.

</div>

## 1. same-release fails without the option

```text
{{#include ../output/renumber-exact.txt}}
```

The reasons name the version of each pack. No value changed.

## 2. Accept the new numbers, and nothing else

```text
{{#include ../output/renumber-allow.txt}}
```

`--allow renumber` passes a pair whose packs differ only by their versions. The
values, the manifests, the params, and the images must be the same. Strata
reads the archive of each new build from Palette, and compares its files: see
[The download of a version](../internals/palette/profiles.md#the-download-of-a-version).
Each line names a pack, its two versions, and two facts. `pre-release to final` says
that the move is from a pre-release to its final version. `archive: same files`
says that the two archives differ only by the version in `pack.json` or in
`Chart.yaml`.

## 3. See the impact

```text
{{#include ../output/renumber-compare.txt}}
```

The impact of each pack is `none (renumber only)`, also on the Kubernetes
layer, because the versions keep their numbers. The values, the manifests, and
the params of the profile stay the same.

## 4. A renumber with a changed value still fails

```text
{{#include ../output/renumber-edited.txt}}
```

The Kubernetes pack is a renumber. The CSI pack also changed a value, so the
pair fails, and the reason names the value.

## 5. A new build with another file fails

```text
{{#include ../output/renumber-archive.txt}}
```

The profile gives the CSI pack the same values, so the record shows no change
of a value. The archive of the new build holds another default value. So the
pair fails, and the reason names the file of the archive.

## 6. The download holds no chart

```text
{{#include ../output/renumber-not-readable.txt}}
```

The add-on profile moves the chart `nginx` of Bitnami to another version, with
the same values. The host of the charts refuses the file, so the download holds
its error answer in place of each chart. Palette answers HTTP 200: the live
check `download-chart` of
[export.sh](../development/live-runs.md#exportsh) shows it. Strata cannot read
the two builds, so the pair fails, and each reason names the version and what
the file holds.

## What to do

- Run `same-release --allow renumber` when your pipeline publishes the final
  version. Give the record of the version that passed the tests first.
- Give the key of Palette to the job. Strata reads the archives from the
  download of each version, also for two record files.
- When a line says `archive: not compared`, make sure in the pipeline that the
  new build changes only its version. The version is in `pack.json`, or in
  `Chart.yaml` for a chart. Palette
  serves no archive of a pack of a private registry: see
  [The archives of the two builds](../commands/same-release.md#the-archives-of-the-two-builds).
- When a line says `archive: not readable`, read the reason. An XML error is
  the answer of the host of the chart. Pin the chart from a registry whose host
  serves it, or make sure of the files in the pipeline.
- A move to other numbers, such as `20.04` to `24.04`, keeps the impact of its
  layer in `compare`, also with the same values. See
  [A renumber](../reference/impact.md#a-renumber).

[Bring your own pipeline](../internals/sample-pipeline.md#a-pre-release-and-its-final-version)
shows the same check in one sample pipeline.
