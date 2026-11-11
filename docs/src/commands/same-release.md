# strata same-release

Make sure that two or more records are one release. Strata compares each record
with the next one, in the order that you give.

The examples use the projects `dev` and `prod`, which each hold a copy of
`aws-stack@1.1.1`.

## Check a promotion

```text
{{#include ../output/promote-same.txt}}
```

Exit `0`: dev and prod have the same release. The line `project` names the
project of each record, by uid, when the two differ. Give the records in
promotion order, for example `dev.json test.json prod.json`.

## A pair that fails

```text
{{#include ../output/promote-different.txt}}
```

Exit `1`. Each reason names the pack. For an `AUTHORED` change, the reason also
names the first changed value, or the first changed manifest. It uses the words
of `compare`, for example `manifest extra-rules changed` or `values changed (not
parseable as YAML on one side)`. See
[Promote a release to another project](../use-cases/promote.md).

## Two projects in one run

Give a project for each side, and strata reads each ref in its own project:

```text
{{#include ../output/projects-same.txt}}
```

`--project-a` or `--project-name-a` gives the project of the first ref.
`--project-b` or `--project-name-b` gives the project of the second ref. The
flag of a side has priority over `--project`. These flags take two refs. For a
chain of three projects, give a record file for each ref, which
`strata snapshot --project-name` writes. See
[Environments in the projects of one tenant](../use-cases/projects.md).

## Which pairs pass

| Result | Cause | Passes |
| --- | --- | --- |
| `IDENTICAL` | The roots are equal, and no pack names another machine image. | yes |
| `SAME CONTENT` | The roots differ only by the tag name of a pack build, or by a pack uid that Palette mints. Palette mints the uid of each manifest pack, and the uid of a pack from a registry that it does not sync. A candidate does not have these uids. `mintedUidOnly` in the JSON names each such pack. | yes |
| `REGISTRY MOVE` | The roots differ only by packs from another registry, with the same tag, version, values, manifests, and params. The digest of each moved pack can differ. | only with `--allow registry-move` |
| `RENUMBER` | The roots differ only by the versions of packs, with the same values, manifests, params, and images. The tag, the pack uid, and the digest of each such pack can differ. | only with `--allow renumber` |
| `DIFFERENT` | Any other difference. | no |
| `NO DIGEST` | The pair passes on its roots. A pack from a registry has no upstream digest on one side, so an equal root does not show an equal archive. | only with `--allow no-digest` |

```text
{{#include ../output/tag-same.txt}}
{{#include ../output/manifest-same.txt}}
{{#include ../output/move-allow.txt}}
```

A registry move can change the pack type, because the type follows the
registry: `spectro`, `helm`, or `oci`. A `manifest` pack comes from no
registry, so it never moves. Without `--allow registry-move`, the reason names
the move, so you can decide to allow it. See
[A pack comes from another registry](../use-cases/registry-move.md).

A move from a registry that gives a digest to one that gives none is a loss of
what the root can show. A pair that differs for another reason stays
`DIFFERENT`, and its reasons name each pack with no digest too. `--allow` takes
more than one value: `--allow registry-move --allow no-digest`. See
[The content digest](../internals/hashes.md#the-content-digest).

## A release under new version numbers

A pipeline can publish a candidate, test it, and then publish the same content
under its final numbers. Each team names the candidate in its own way, such as
`-g<sha12>`, `-rc.1`, `-mr<iid>.<pipeline>`, or `+build.5`. Strata reads no
suffix. It asks one question: do the two releases hold the same content, when
only the version numbers differ?

Without an option, the pair fails, because the version of each pack is in the
root:

```text
{{#include ../output/renumber-exact.txt}}
```

`--allow renumber` passes it:

```text
{{#include ../output/renumber-allow.txt}}
```

Exit `0`. Strata pairs the packs of the two records by name and layer. Two
packs are a renumber when their versions differ, and their type, registry,
values, manifests, params, image list, and `imageId` are the same. The tag, the
pack uid, and the digest of the archive follow the version. The two profile
versions can differ too. Any other difference gives the result of the table
above, and the reason names it:

```text
{{#include ../output/renumber-edited.txt}}
```

The verdict lists each renumber as `<name>  <old> -> <new>`. It says
`pre-release to final` when the move is from a pre-release to its final version
in Semantic Versioning. That is a fact and not a condition, so a scheme of
names that is not Semantic Versioning passes too, such as `1.2.0-build5` to
`2026.11.0`. `pairs[].renumbers` in the JSON gives each renumber:

```text
{{#include ../output/renumber-json.txt}}
```

### The archives of the two builds

The `pack.json` of a new build holds the new version, so the build has a new
digest. The digest alone cannot show that only `pack.json` changed. So strata
reads the archive of each such pack from Palette, and compares the files of
the two archives. It does not compare `version` in `pack.json`. The line of
each renumber gives what strata found:

| Fact | What strata found |
| --- | --- |
| `archive: same files` | The two archives hold the same files, and `pack.json` differs only by its `version`. `pairs[].archivesCompared` names the pack. |
| `archive: other files` | Another file differs. The pair fails, and a reason names the file. `pairs[].archivesChanged` names the pack and each file. |
| `archive: not compared` | Palette serves no archive of the pack, or a record is not a published version. `pairs[].archivesNotCompared` names the pack. |

A new build of a pack can change a file that the profile does not show, such
as a default value of the pack. The values of the profile then stay the same,
and only the archive shows the change:

```text
{{#include ../output/renumber-archive.txt}}
```

```text
{{#include ../output/renumber-archive-json.txt}}
```

Strata reads the archives from the download of each profile version: see
[The download of a version](../internals/palette/profiles.md#the-download-of-a-version).
So `same-release --allow renumber` connects to Palette, also for two record
files. It reads each version in the project of its record, and reads it once.
It reads nothing when each renumber keeps its digest.

**Check such an archive in your pipeline.** A line says
`archive: not compared` when strata cannot read the two archives. Palette
serves no archive of a pack of a private registry. It serves none of a pack of
an OCI registry of Helm charts or of Zarf packages. A record of a candidate, a
draft, or a cluster is no published version, so strata reads no download for
it. A manifest pack has no archive. Its content is its manifests, and the
records hold them.

Palette takes one download of a version in 10 seconds. When another client
read the version less than 10 seconds before, Palette refuses with HTTP 429 and
the code `TooManyRequests`. Strata then waits 10 seconds before the next
download of the version, for each retry of `STRATA_RETRIES`. After the last
retry, it stops with the error `api`. Run the command again after 10 seconds.

`compare` reads no archive. It gives such a pack the impact `none` when its
version keeps its numbers: see [A renumber](../reference/impact.md#a-renumber).
See [A release gets new version numbers](../use-cases/renumber.md).

## A pack names another machine image

The root does not cover the annotations of a pack. So two records can have one
root, and their OS pack can name two machine images in `imageId`. Strata
compares the `imageId` of each pack that the two records hold. When the two
images differ, the pair is `DIFFERENT` under each policy, also when the roots
are equal. The reason names the pack and the two images:

```text
{{#include ../output/os-image-same-release.txt}}
```

Strata does not compare a side with no `imageId`. A record written before
strata kept the annotations has none. See
[The OS pack names a new machine image](../use-cases/os-image.md).

## A macro has another value in each project

The values and the manifests of a version can read a macro, as
`{{.spectro.macro.<name>}}`. Each project gives the macro its own value. The
root covers the name of the macro and not its value. So one release can run
with a different configuration in each project.

Record each side with `snapshot --macros`. Then each record holds the digests
of its macro values, and strata compares them. A macro with another value on
each side gets a line under its pair:

```text
{{#include ../output/promote-macro-changed.txt}}
```

The line does not change the result, and the exit stays `0`. The release is the
text, and each project gives the value. `pairs[].macrosChanged` in the JSON
names each macro:

```text
{{#include ../output/promote-macro-changed-json.txt}}
```

When a record holds no digests, strata cannot compare the values. The line
under the pair names each macro that the content uses.
`pairs[].macrosNotCompared` in the JSON gives the same names:

```text
{{#include ../output/promote-macro-not-compared.txt}}
```

See
[A macro has another value in each project](../use-cases/promote.md#a-macro-has-another-value-in-each-project).

## A masked value

A pack whose values hold `********` gets a line under its pair. The line is
`pack <key> has a masked value at <path>: an equal root does not show an equal secret`.
It does not change the result, and `pairs[].masked` in the JSON gives the same
fact. See [A secret changes in a pack value](../use-cases/masked-value.md).

## Refs

Each side is a ref: a snapshot file, a profile uid, `<name>@<version>`,
`<name>@latest`, a cluster, or `pack:<name>@<version>`. Strata reads a uid, a
name, or a cluster in the configured project, or in the project of a flag. To
compare two projects, give a project for each side, or two files.
[Refs](../reference/refs.md) lists each form.

## What to expect

| Exit | Meaning |
| --- | --- |
| `0` | Each pair passes. |
| `1` | A pair fails. |
| `2` | Strata could not read a record. |

`--json` writes [`strata-chain/1`](../reference/documents.md#strata-chain1).

## Options

```text
{{#include ../output/help-same-release.txt}}
```

`--project <uid>` or `--project-name <name>` reads each ref in that project, in
place of the configured project. `--project ''` is tenant scope. A record file
sends no request. `--project-a`, `--project-name-a`, `--project-b`, and
`--project-name-b` give the project of one side: see
[Two projects in one run](#two-projects-in-one-run).

`--format`, `--json`, `--compact`, `--color`, `--no-color`, and `-q`
(`--quiet`) are the output options that each report command takes: see
[Output formats](./index.md#output-formats).
