# A profile version or a pack version

`<name>@<version>` records one profile version, and `pack:<name>@<version>`
one pack version from a registry. [strata snapshot](../snapshot.md) gives the
other subjects, what to expect, and the options.

## Record a profile version

```text
{{#include ../../output/snapshot-profile.txt}}
```

Keep the file with the release, for example at
`releases/<version>/<project>.json` in a repository. The name is your choice.

The file holds the root and one hash for each pack. It also holds the values in
their canonical form and as Palette gave them, the manifests, the variables, and
the clusters that used the version. [The hashes and the root](../../internals/hashes.md)
explains the hashes.

```text
{{#include ../../output/snapshot-record.txt}}
```

`source.projectUid` is the project. `verify` checks the record there, also
after a rename of the project. [JSON documents](../../reference/documents.md#strata-snapshot1)
lists each field.

When Palette marks a pack of the version invalid, `snapshot` writes a warning
on standard error. The registry deleted the tag of that pack. The record keeps
`invalid` and `invalidReason` on the pack, outside the root. See
[A registry deletes the tag of a pack](../../use-cases/invalid-pack.md).

With `--macros`, the record also holds the digest of the value that Palette
gives each macro at the scope of the version. See [Macros](cluster.md#macros).

## Record the newest version

```text
{{#include ../../output/snapshot-latest.txt}}
{{#include ../../output/snapshot-latest-version.txt}}
```

`<name>@latest` is the highest version that is not a pre-release: `22.04` is
above `20.04`, and `1.30.6` is above `1.29.10`. A pre-release is below its
version, so `1.0.1` is above `1.0.1-g0123456789ab`. A name that has only
pre-releases gives its highest one. See
[The order of versions](../../reference/compatibility.md#the-order-of-versions).

The note `@latest is` names the version that the ref takes. When `@latest`
skips a pre-release above that version, the note names the highest one. So a
pipeline that publishes a build on each merge gets one line. In
[the sample pipeline](../../internals/sample-pipeline.md#the-pre-release-of-a-merge),
`aws-stack` has pre-releases above `1.1.1`:

```text
{{#include ../../output/pipeline-latest.txt}}
```

`-q` removes the note, as it removes each progress note. An app profile gets the
same note with its ref, such as `@latest is appprofile:shop@1.0.0`. `2.2.8-hotfix`
is a pre-release by SemVer, below `2.2.8`, also when a team means a later build.
Give a later build a higher number, such as `2.2.9`.

A develop build such as `1.1.8.develop` does not parse as a semantic version.
Palette reads it as `1.1.8`. Strata puts it below `1.1.8`, and `@latest` skips
it as a pre-release. A develop build such as `1.2.develop` holds no three
numbers, so Palette reads no version in it. Strata reads it as a pre-release of
`1.2.0`, and `@latest` skips it too. Here the name has `1.1.8`,
`1.1.8.develop`, `1.1.10`, and `1.2.develop`:

```text
{{#include ../../output/latest-develop-list.txt}}
```

`strata list` gives `1.2.develop` last, because it is the highest. The record
takes `1.1.10`, and the note names `1.2.develop`:

```text
{{#include ../../output/latest-develop.txt}}
{{#include ../../output/latest-develop-version.txt}}
```

`1.2.develop` does not parse, so the note does not say `by SemVer`. See
[the rule of `@latest`](../../reference/refs.md#rules).

<div class="playground-demo">

`just playground latest-develop` serves this tenant on this machine, with no key and no cost.

</div>

A version with four numbers reads as its first three, and the fourth number puts
it above them. So `10.4.0.1` is above `10.4.0`, and `@latest` gives `10.5.0` over
it. See [the rule of `@latest`](../../reference/refs.md#rules).

## The cluster templates of a version

A cluster template links profile versions and applies them to the clusters
that it governs. While a template links a version, Palette refuses a `PUT` of
the whole version and refuses its delete. See
[The lock of a cluster template](../../internals/palette/profiles.md#the-lock-of-a-cluster-template).
Palette shows each template that links the version, so a record does not keep
the templates. A template that links the version does not change the root.
`strata consumers` reads the templates from Palette and counts each one:

```text
{{#include ../../output/retire-template.txt}}
```

See [strata consumers](../consumers.md). A record of strata 0.18 or 0.19 can
hold `source.clusterTemplates`. Strata reads that record and ignores the field.

## The draft of a version

A person who edits a published version and does not publish makes a draft.
`<name>@<version>@draft` and `<uid>@draft` record the draft, and not the
published template:

```text
{{#include ../../output/draft-record.txt}}
```

The record says `source.template` `draft`, and keeps `@draft` in `source.ref`.
It holds the values of the draft, and the draft content of each manifest that
the draft changed. A version with no draft exits `2` with `no-draft`:

```text
{{#include ../../output/draft-none.txt}}
```

`compare <ref> <ref>@draft` shows what the publish changes. See
[Review a draft before its publish](../../use-cases/draft.md).

A version that nobody published has only its draft. `<name>@<version>` and
`<uid>` record that draft with no `@draft`, and the record says
`source.template` `draft` too. `list --packs` marks the version `draft`: see
[strata list](../list.md).

## The registry of each pack

A record names the registry of each pack in `registryName`, beside
`registryUid`. Two registries can have the same pack version, and the record
says which one the version takes:

```text
{{#include ../../output/pack-record-registry.txt}}
```

The pack ref of a profile names its registry by the uid alone. So `snapshot`
reads the names from the registry listing of Palette, with one more request. It
reads the listing only when a pack names a registry. A registry that the key
cannot see gets no `registryName`. The name is outside the root, so a new name
of a registry does not change the root.

When the registry listing answers HTTP 404 or 405, the record holds no names.
Each other error, such as HTTP 403, stops the record, and strata gives the
error. The record of each other subject names its registries in the same way.
`--raw-dir` saves the listing: see [A record with no network](no-network.md).
`strata list --pack <name>` names the registry of a pack in each version: see
[The registry of each pack](../list.md#the-registry-of-each-pack).

## Record a pack version

`pack:<name>@<version>` records one pack version from a registry, with the
values that the registry ships. `verify` on that record reads the registry
again, so it says whether a published pack version still serves the content it
served. See [What changed between two pack versions](../../use-cases/pack-versions.md).

The note `recording pack` names the version that the record takes. For
`pack:<name>@latest`, it also names the highest pre-release that the ref skips:
see [Which version is the newest](../packs.md#which-version-is-the-newest).

## Where to keep a record

A record is a file. Strata reads a record and writes a record. Strata does not
decide where you keep a record. Two places are usual, and each one answers a
different question:

| Where | What it gives | What it costs |
| --- | --- | --- |
| The repository, beside the code that publishes | A review, an author, and a date for each release. `git log` on the file says which commit pinned a release. | The job that publishes must push. |
| An object store, beside the state of the pipeline | Every job that publishes already writes there, and nothing new needs a token that pushes. | The history is the store's, not the repository's. |

A record holds each pack's values, params, and manifests in full. Palette
writes `********` over each value whose schema format is `password`. Strata
does the same for a profile with a draft. See
[Secrets in the values](../../internals/palette/profiles.md#secrets-in-the-values).
`same-release`, `compare`, and `verify` name each masked value. See
[A secret changes in a pack value](../../use-cases/masked-value.md).

A record does not hold the value of a sensitive profile variable. Each other
secret stays: a secret in a value that the schema does not mark, or in a
manifest. So a record of a profile whose pack values or manifests hold such a
secret carries that secret. Keep that record in the object store, and not in the repository.

The book uses two layouts. The use cases keep one record for each version, at
`releases/<version>/<project>.json`, and one for each cluster, at
`runs/<cluster>.json`. [A team pins each environment](../../use-cases/fan-out.md)
keeps one record for each environment, at `releases/<environment>.json`. Strata
reads both.

`verify` takes a pattern, so a directory of records is one command:
`strata verify "releases/*.json" --summary`. Read them from the store into a
directory first when they live outside the repository.
