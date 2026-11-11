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
value. The pack uids and the digests of the pre-releases are stand-ins.

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
values, the manifests, the params, and the images must be the same. Each line
names a pack, its two versions, and two facts. `pre-release to final` says that
the move is from a pre-release to its final version. `archive: not compared`
says that strata did not read the archive of the new build.

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

## What to do

- Run `same-release --allow renumber` when your pipeline publishes the final
  version. Give the record of the version that passed the tests first.
- Make sure in the pipeline that the archive of each new build differs only by
  `version` in `pack.json`. Strata does not compare the archives:
  [#751](https://github.com/como-technologies/strata/issues/751) adds that
  compare.
- A move to other numbers, such as `20.04` to `24.04`, keeps the impact of its
  layer in `compare`, also with the same values. See
  [A renumber](../reference/impact.md#a-renumber).

[Bring your own pipeline](../internals/sample-pipeline.md#a-pre-release-and-its-final-version)
shows the same check in one sample pipeline.
