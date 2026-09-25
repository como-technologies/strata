# JSON documents

Each document has a `schema` id and a `tool` block with `name`, `version`, and
`impl`. A record of the first bash release has no `impl`. `strata-diff/1`,
`strata-verify-summary/1`, and `strata-chain/1` also have a `legend` that
explains their values in words, so a program or a language model can read them
without this book. `strata schema <kind>` prints the JSON Schema of each
document.

| Schema id | Written by | `strata schema` |
| --- | --- | --- |
| `strata-snapshot/1` | `snapshot` | `snapshot` |
| `strata-diff/1` | `compare --json`, and `verify --json` for one file | `diff` |
| `strata-verify-summary/1` | `verify --json` for many files, or with `--summary` | `verify-summary` |
| `strata-chain/1` | `same-release --json` | `chain` |
| `strata-consumers/1` | `consumers --json` | `consumers` |
| `strata-list/1` | `list --json` | `list` |
| `strata-packs/1` | `packs --json` | `packs` |

## strata-snapshot/1

A record of one profile version or cluster.

- `root`: the sha256 digest of the release. See
  [The hashes and the root](../internals/hashes.md).
- `contentDigest`: the sha256 digest of what the version is, without the ids
  that Palette gives. Two publishes of one definition can have two roots and
  one content digest: when the definition holds a manifest pack, or when the
  second publish takes a pack from another registry. An older record has no
  such field, and the library calculates the digest from the pack hashes. See
  [The content digest](../internals/hashes.md#the-content-digest).
- `source`: where the record comes from.
  - `ref`: the ref as typed, a name or a uid.
  - `uid`, `projectUid` (empty for tenant scope), `name`, `version`,
    `cloudType`, `type`, and `tags`.
  - `template`: `published`, `draft`, `applied` for a cluster, `candidate`, or
    `registry` for a pack record.
  - `consumers`, for a published or a draft version: `clusters[]`, the uids of
    the clusters that used the version, and `count`. A candidate, a cluster
    record, and a pack record have no `consumers`, because no request asked.
  - `variablesUnavailable`, only when true: Palette did not serve the variables
    endpoint, so the root is the root of a version with no variables. See
    [When Palette does not serve the variables](../internals/hashes.md#when-palette-does-not-serve-the-variables).
  - `fetchedAt`: the time of the read.
  - `pack`, for a pack record: the pack's `name` and `version`, the
    `registryUid` and `registryName` it came from, and `packUid`, the uid of
    that version in the registry. `template` is `registry` and `type` is
    `pack`.
  - `cluster`, for a cluster record: its `uid` and `name`, the `profile` that
    `cluster:<cluster>/<profile>` selected, `profiles[]`, and `variables[]`.
    `updatesPending` is `true` when Palette says an update of a profile the
    record covers waits, `false` when none waits, and absent when strata could
    not read it; `updatesPendingError` then says why. `pendingUpdates[]` has
    `profileUid`, `profile`, `message`, and `since` for each change that waits.
    `macroValues`, only with `snapshot --macros`: each user macro the content
    uses, and `sha256:` of the value Palette gives it for this cluster, or
    `unresolved`. None of these is part of the root.
    Each entry of `profiles[]` has `uid`, `name`, `version`, and `type`, in the
    order [The order of the profiles of a cluster](../internals/palette.md#the-order-of-the-profiles-of-a-cluster)
    gives. Each entry of `variables[]` has `profile`, `profileUid`, `name`,
    `value`, and `sensitive`. `verify` reads only the uids, so a rename does
    not stop it. `variables` is not part of the root. A sensitive variable has
    `sensitive: true`, and no value or hash.
- `variables` and `variablesHash`: the profile variables.
- `packs[]`: one entry for each pack, in the order the profile holds them. The
  diff document sorts its packs by layer and then by name; the snapshot does
  not.
  - The identity fields: `key` (`<layer>/<name>`), `name`, `layer`, `type`,
    `tag`, `version`, `packUid`, and `registryUid`.
  - `refDigest`, the digest on the pack reference, and `registryDigest`, the
    digest of the pack in the manifests document.
  - `params`: the params of the pack on the profile, as a map.
  - `values`: the raw values YAML. `valuesCanon`: the values as JSON with
    sorted keys, or `null` when the values do not parse. `valuesHash`: the
    sha256 of `valuesCanon`, or of the trimmed text when the values do not
    parse. `valuesParseError`, only then: why.
  - `manifests[]`, sorted by name: `name`, `uid`, `digest`, `content`,
    `contentHash`, and `contentParseError` when the content does not parse.
  - `hash`: `identity`, `upstream`, `upstreamRaw`, `upstreamSource`,
    `authored`, and `node`. `upstreamSource` is the field that gave `UPSTREAM`:
    `packRef.digest`, `packSummary.digest`, or `none`.

A cluster record that covers two or more profiles joins their names and versions
with `+`, for example `aws-stack+platform-addons@1.1.0+1.0.0`, and its `source.uid`
is empty. A record of one profile keeps the name, version, and uid of that
profile.

## strata-diff/1

A comparison of two sides, `a` and `b`.

- `generatedAt`: the time of the comparison.
- `legend`: the flags, each `status`, each `op`, and each impact level, in
  words.
- `a` and `b`: the `source` block of each side, as
  [strata-snapshot/1](#strata-snapshot1) gives it.
- `same`: the roots are equal. `rootA` and `rootB` are the roots.
- `sameContent`: the roots are equal, or differ only by the pack uids of
  manifest packs and the tag names of the same builds, with the variables and
  the pack order unchanged. `same` stays `false` in the second case.
- `editedInPlace`: both sides name the same uid and version, and the roots
  differ.
- `overwrittenInPlace`: each uid that the sides name under different versions,
  with `uid`, `name`, `versionA`, `versionB`, and `nameB` when the name changed
  too. Absent when empty.
- `summary`: `packsAdded`, `packsRemoved`, `packsChanged`, `packsUnchanged`,
  `identityChanges`, `upstreamChanges`, `authoredChanges`, `variablesChanged`,
  and `maxImpact`.
- `variables`: `changed`, the two hashes, `diff[]` with paths
  `<variable>.<field>`, and `byName` with `added[]`, `removed[]`, and
  `changed[]`. An added or removed variable has `name`, `required`,
  `sensitive`, `hidden`, and `immutable`. A changed variable has `name` and
  `fields[]`, whose paths are relative to the variable, such as `defaultValue`.
- `clusterVariables`, only between two clusters: `changed`, and `values[]` with
  `op`, `profile`, `name`, `old`, `new`, and `sensitive`. A sensitive variable
  has no `old` or `new`. The values of each side are in `a.cluster.variables` and
  `b.cluster.variables`.
- `orderChanged`: the two sides apply their packs in a different order.
- `packs[]`: one entry for each pack key on either side, sorted by layer and then
  by name.
  - `status`: `unchanged`, `changed`, `added`, or `removed`.
  - `identity`, `upstream`, `authored`, and `paramsChanged`.
  - `tagRenamed`: only the tag name differs, for the same build.
  - `mintedUidOnly`: a manifest pack whose only difference is its pack uid.
  - `impact`: `level` and `text`.
  - `a` and `b`: `tag`, `version`, `packUid`, `registryUid`, `upstream`,
    `upstreamSource`, `valuesHash`, and `node`.
  - `values`: `changed`, `parseable`, `changedCount`, `addedCount`,
    `removedCount`, and `diff[]`.
  - `manifests[]`: `op`, `name`, `hashA`, and `hashB`.
  - `upstreamUnknown`, only when true: a pack from a registry with no upstream
    digest on either side.
- `upstreamUnknown`, only when it is not empty: the key of each such pack. An
  equal root does not show an equal archive for it.
- `macros`, only when it is not empty: each user macro that either side uses.
- `macrosChanged`, only when it is not empty: between two cluster records with
  `macroValues`, each macro whose value differs.
- `paletteRepave`, only with `compare --repave`: `clusterUid`, `required`
  (Palette's `isRepaveRequired`), `reasons[]` with `code`, `message`, `pack`, and
  `keys` (the config keys that differ), and `agrees`. `agrees` is `false` when
  the most severe level of the table and Palette's answer disagree.
- `verify`, only after `verify`: `status` (`unchanged`, `drifted`, or
  `unverified`), `uid`, `recordedAt`, and `checkedAt`.
  - `upstreamUnknown`: for an `unverified` record, the packs that make it so.
  - `macrosChanged`: for a cluster record with `macroValues`, each macro whose
    value moved. The status is `drifted`, also when the roots are equal.
  - `overwrittenBy`: for a profile record, the `name@version` that the uid names
    now. The status is `drifted`, also when the roots are equal.
  - `clusterValuesChanged`, only when it is not `0`: for a cluster record, the
    number of variable values that differ from the record.
  - `profilesOverwritten`: for a cluster record, each profile that the cluster
    runs whose uid names another version now, or no longer exists: `uid`,
    `name`, `clusterVersion`, and `profileVersion` (empty for a uid that is
    gone). The status is `drifted`.
  - `who`, only with `--who` and for a drifted profile or cluster record: each
    write to the profile, or to the cluster, that the audit log of Palette
    holds since `recordedAt`, oldest first: `time`, `actor`, `actorType`
    (`user` or `service`), `actorUid` (for a user), `action`, `summary` (when
    Palette gives one), and `auditUid`. An empty list means the log holds no
    write since the record.

Each changed value is a leaf with `op`, `path`, `segments`, `old`, and `new`. A
leaf is a scalar or an empty container. The path joins keys with a dot. A list
item is `[i]`. When each item of a list is an object with a unique `name`, the
item is `[name=<value>]`, so an inserted item shows as one addition. `segments`
is the path as a list: strings for keys, numbers for indexes, and
`{"name": ...}` for named items.

## strata-verify-summary/1

- `legend.status`: `unchanged`, `drifted`, `unverified`, and `error`.
- `summary`: `records`, `unchanged`, `drifted`, `errors`, and `status`, which is
  the exit status in words. `unverified`, only when it is not `0`: the records
  whose content is the same and that hold a pack with no upstream digest.
- `records[]`: one entry for each file, in the order given: `path`, `status`,
  `name`, `version`, `uid`, `projectUid`, `recordedAt`, `rootRecorded`, and
  `rootLive` when strata read the live profile.
  - A drifted record adds `maxImpact`, `packsChanged`, and the full `diff`. An
    unverified record adds the `diff`, which names the packs.
  - A cluster record with changed variable values adds `clusterVariables`,
    whatever its status.
  - A failed record adds `error`. See [Errors](./errors.md).

## strata-chain/1

- `legend`, and `policy`: `exact` or `registry-move`. `allowNoDigest`, only
  when true: `--allow no-digest` was given.
- `same`: each pair passed.
- `summary`: `pairs`, `passed`, and `failed`.
- `records[]`: `label`, `name`, `version`, `uid`, `projectUid`, and `root`.
- `pairs[]`: `a`, `b`, `status` (`identical`, `same-content`, `registry-move`,
  `different`, or `unknown-upstream`), `rootA`, `rootB`, `packsRepinned`,
  `packsRetagged`, and `reasons[]`. `upstreamUnknown`, only when it is not
  empty: the packs with no upstream digest, also when they are allowed.

## strata-consumers/1

- `profile`: `uid`, `name`, and `version`.
- `count`: the number of consumers. Exit `0` when it is `0`.
- `overwritten`: the number of consumers whose copy names another version.
- `consumers[]`: `uid`, `name`, `projectUid`, `state`, `version`, and `error`
  when strata cannot read the cluster. `projectName` joins them when strata
  could read the project listing. The names and the projects come from
  `status.inUseClusters` of the profile. `unlisted`, only when true: the profile
  did not list the cluster, and the second reading found it.
- `clustersRead`: the second reading. `read`: the cluster documents read.
  `unread`: the clusters the listing names that strata could not read. `error`:
  why strata could not read the cluster listing. Then `read` is `0`.

## strata-list/1

- `projectUid` and `projectName`: the scope.
- `pack`: the `--pack` text, when given.
- `cluster`: the `--cluster` text, when given.
- `profiles[]`, `clusters[]`, and `projects[]`: only the kinds that were listed.
  A kind that was listed and holds nothing is `[]`. Profiles come by name and
  then by version, clusters by name, and projects by name.
  - A profile: `ref`, `name`, `version`, `uid`, and `cloudType`. With `--packs`,
    also `type`, `consumers`, and `packs[]` with `layer`, `name`, `tag`, and
    `version`, or `error`.
  - A cluster: `ref`, `name`, `uid`, `state`, `cloudType`, and `projectUid`.
    `projectName` joins them when strata could read the project listing.
    With `--applied`, also `profiles[]` with the `uid`, `name`, `version`, and
    `type` of each profile the cluster applies, or `error`, and
    `updatesPending` and `pendingUpdates[]` as a cluster record gives them.
    `updatesPending` is absent when strata could not read the notifications.
  - A project: `name` and `uid`.

## strata-packs/1

- `name`: the pack that was asked for, and `cloud` when `--cloud` was given.
- `registries[]`: one entry per registry that has the pack, with `registry`,
  `registryUid`, `layer`, and `versions[]`.
  - A version: `version`, `packUid`, `digest`, `type`, `cloudTypes[]`, `tags[]`
    with the exact tag first, and `latest`, which is true for the newest version
    the command listed. With `--cloud`, that is the newest version for that
    cloud, and not the newest the registry holds. `state` is `deprecated`,
    `disabled`, or `deleted` when the listing gives one, and strata leaves out
    the field when it gives none.
  - `versionsOffered`: how many versions of the pack the registry holds, before
    `--latest` or `--cloud` cut the list.
  - `tagsUnavailable`: `true` when Palette lists the pack under the registry and
    then answers 404 for it. The versions come from the listing, with no tags.
    When it is `false`, strata leaves out the field.
  - `state`: the state that every version in the registry has, when they all
    have the same one. Strata leaves out the field otherwise.

