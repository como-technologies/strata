# strata candidate

`strata candidate <ref>` writes a candidate file from a profile version that
Palette already has.

## What a candidate file is

A profile version is a stack of packs with their values, and the variables of
the profile. Palette holds a version after somebody publishes it. A candidate
file holds the same content as YAML, in your repository, before Palette has it.

A person reads and edits the file. A [record](./snapshot.md) holds the same
version as hashes, which a machine compares.
[The candidate file](../reference/candidate-file.md) gives each field.

Strata does not publish a candidate, and strata never writes to Palette. Your
pipeline publishes the version. Strata tells you what that version will be.

## Why you write one

- **To review a change before the version exists.** The change is a diff in a
  pull request. `strata snapshot --from-candidate` calculates the root of the
  version to come. `strata compare` then names each layer that changes, and the
  impact on a cluster. See
  [Review a change before you publish it](../use-cases/review.md).
- **To keep the version in git.** `--files-dir` writes the values of each layer
  to their own file. A change to one value is then a change to one line, with an
  author and a reason.
- **To prove that Palette has the release you approved.** Record the published
  version. Then run `strata same-release candidate.json published.json`. Exit
  `0` says that the two are one release.

This command writes the first file for you. Write the next one by hand, from the
version before it, or from what your repository already holds. A repository
that holds the export of Palette needs no candidate file: `snapshot
--from-candidate` reads the export. See
[A profile export of Palette](../reference/candidate-file.md#a-profile-export-of-palette).

## Write one from your own source

A team that publishes with a pipeline already describes each profile somewhere:
a YAML file of layers, a Terraform module, a chart of values. A candidate file is
that same description, in the form strata reads. A job writes the candidate file
beside the source. The file needs few fields:

| The candidate asks for | Your source usually calls it |
| --- | --- |
| `name`, `version` | the profile and the version to publish; the name is the name in Palette, with whatever prefix your pipeline adds |
| `type`, `cloudType` | the kind of profile and its cloud |
| `layers[].name`, `layers[].version` | each pack and its version |
| `layers[].registry` | the registry a layer comes from |
| `layers[].valuesFile` | the values file of a pack |
| `layers[].tag` | the tag, when it is not the version |
| `variables[]` | the profile variables |

[The candidate file](../reference/candidate-file.md#the-fields-of-a-layer) gives
each field, the ones that are required, and each default.

Four rules decide whether the roots agree, so a job that writes candidates must
follow them:

- **The tag is the version by default, when the registry lists that tag.** A
  pipeline can pin a pack under another tag. The candidate must then give that
  tag in `tag`.
- **Strata uses the names that Palette uses.** A variable field such as
  `display_name` is `displayName` in the candidate file.
- **Strata applies no pack preset.** The documents that strata builds for a
  candidate carry an empty `presets` list. When the values select an option of
  the pack template, Palette can write the presets of that option into the
  values, and strata warns: see
  [Template options](../reference/candidate-file.md#template-options).
- **A candidate pins a pack version, and not a tag.** A record of a profile
  pinned under a floating tag gives a candidate with that version and that tag.
  After the registry moves the tag to another version, `snapshot
  --from-candidate` stops with `pack-not-found`. Give the new version in the
  candidate.

Prove the mapping before a pipeline depends on it. Write a candidate file from a
version that Palette already has. Then check that the two are one release. The
steps below do this, and
[Build the snapshot of a candidate](#build-the-snapshot-of-a-candidate) shows
the answer. Run it again when the shape of your source changes.

The examples use the project `dev`, which holds `aws-stack@1.1.1`.

## Write a candidate file

```text
{{#include ../output/candidate-write.txt}}
{{#include ../output/candidate-files.txt}}
```

`--files-dir` puts the values of each layer and each manifest in a file. Keep
this form in a repository: a pull request then shows a change to a value as a
change to one line.

```text
{{#include ../output/candidate-yaml.txt}}
```

The file pins the tag and the registry uid of each layer. It carries the values,
the params, the manifests, and the variables. A manifest pack has no registry,
so its layer has `type: manifest` and no registry uid: see
[A manifest layer](../reference/candidate-file.md#a-manifest-layer).

## Build the snapshot of a candidate

```text
{{#include ../output/candidate-snapshot.txt}}
```

Strata finds each pack in its registry, and builds the documents that Palette
gives for a published profile with that content. Here the file comes from the
published 1.1.1, so its root equals the root of that version:

```text
{{#include ../output/candidate-published.txt}}
{{#include ../output/candidate-same.txt}}
```

The two roots stay equal when the registry still has the same pack versions.

After a person edits the published version in place, `same-release` between
the candidate and the version fails and names the edited value.

## Ask Palette before you publish

The roots tell you what the version will be. They do not tell you whether
Palette takes it. `snapshot --from-candidate --validate` asks Palette with
three routes of the create. No request changes anything, and Palette holds no
profile after them:

1. `GET /v1/clusterprofiles/validate/name` checks the name and the version.
2. `POST /v1/clusterprofiles/import/validate` checks the names of the packs and
   the manifests, a repeat of each, and a pack that Spectro Cloud disabled.
   Strata sends the candidate in the shape of a profile export, with the registry of each pack from
   `GET /v1/registries/metadata`.
3. `POST /v1/clusterprofiles/validate/packs` checks the values and the
   manifests of each pack, a pack that Spectro Cloud disabled, the
   dependencies of each pack, and the limits.

[The validate routes](../internals/palette/validate-routes.md#get-v1clusterprofilesvalidatename)
gives each check. No route checks the layers of the type, the core layers, the
cloud of each pack, or the tag of a pack that Palette makes. Strata checks them
before any request: see
[The gaps that strata fills](../internals/palette/profiles.md#the-gaps-that-strata-fills).

When Palette refuses, strata prints one line for each error, exits `2`, and
writes no record. The candidate here comes from the published 1.1.1, and the
project holds that version:

```text
{{#include ../output/candidate-validate-exists.txt}}
```

A version that Palette does not parse gets a refusal from the name route and
from the import validate route. Strata prints it once:

```text
{{#include ../output/candidate-validate-version.txt}}
```

With the new version 1.2.0, Palette takes the name, and the check of the packs
refuses a dependency:

```text
{{#include ../output/candidate-validate-refuses.txt}}
```

Spectro Cloud marks some versions as `disabled`. A profile must not hold such a
version. Palette serves it, so strata records it without `--validate`. With
`--validate`, the import validate route refuses the candidate, and the message
names the pack:

```text
{{#include ../output/candidate-validate-disabled.txt}}
```

A line names the code of Palette, the pack, and the message of Palette. A
refusal of the whole request, for example values that are not YAML, names no
pack. The API schema refuses two equal layers with HTTP 422, and the code is
the number `610`. Fix the candidate, and ask again.

When Palette takes the packs, strata writes the record. Then a sentence says
what Palette accepts, as the last line. The record holds the answer in
`source.paletteValidation`:

```text
{{#include ../output/candidate-validate-accepts.txt}}
```

The answer does not change the root. A pipeline that publishes from the
candidate can run this step first, and stop on exit `2`.

`strata snapshot <name>@<version> --validate` asks the same of a version that
Palette holds, as an update of itself, with
`POST /v1/clusterprofiles/{uid}/validate/packs`. Palette then also checks the
packs against the clusters that run the version:

```text
{{#include ../output/snapshot-validate-version.txt}}
```

Without `-o`, the record goes to standard output, and the sentence comes after
it. So on a terminal, the record does not hide the answer:

```text
{{#include ../output/snapshot-validate-stdout-lines.txt}}
```

The `-q` option does not drop the sentence, because the answer is what
`--validate` asks for.

`--validate` needs the network, so strata refuses it with `--from-raw`:

```text
{{#include ../output/snapshot-validate-usage.txt}}
```

## What to expect

- The ref is a profile uid, `<name>@<version>`, `<name>@latest`, or a profile
  record. Each of the first three takes `@draft`, which writes the draft of
  the version, before its publish: see
  [Review a draft before its publish](../use-cases/draft.md#3-record-the-draft-that-the-reviewer-approves). A cluster, edge host, cluster group, app profile, or pack ref is an
  error: `profile-ref-required`. A pack is one layer of a profile version, a
  cluster holds a copy of one, and an app profile has tiers and no packs.
- `--files-dir` needs `-o`.
- `snapshot --from-candidate --validate` exits `2` with `palette-refuses` when
  Palette refuses the name, the version, or the packs, and then writes no
  record. The key needs the permission to create a profile: the import validate
  route checks it.
- Exit `0`, or `2` on an error.

[The candidate file](../reference/candidate-file.md) describes each field,
and how to write a file by hand.

## Options

```text
{{#include ../output/help-candidate.txt}}
```

`--project <uid>` or `--project-name <name>` reads the version in that project,
in place of the configured project. `--project ''` is tenant scope. A record
file sends no request.

`-o` (`--output`) writes the candidate file, and without it the file goes to
standard output. `-q` (`--quiet`) drops the progress messages.
