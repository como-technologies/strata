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
| `strata-behind/1` | `behind --json` | `behind` |
| `strata-preflight/1` | `preflight --json` | `preflight --schema report` |
| `strata-preflight-rules/1` | `preflight --rules --json` | `preflight --schema rules` (one section) |
| `strata-preflight-ledger/1` | the ledger of runs | `preflight --schema ledger` |
| `strata-preflight-promotion/1` | `preflight --promotion --json` | `preflight --schema promotion` |

## strata-snapshot/1

A record of one profile version, cluster, edge host, cluster group, app
profile, or pack version.

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
    `cloudType`, `type`, and `tags`. `type` is `app` for an app profile.
  - `template`: `published`, `draft`, `applied` for a cluster, `candidate`, or
    `registry` for a pack record.
  - `consumers`, for a published or a draft version: `clusters[]`, the uids of
    the clusters that used the version, and `count`. A candidate, a cluster
    record, a pack record, and an app profile record have no `consumers`. The
    consumers of an app profile are app deployments, and not clusters.
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
    `unresolved`. `labels`: the labels of the cluster. `tiers`: each tier that
    the record carries, by name (`applied`, `running`, `policy`, and, with
    `snapshot --shape`, `shape`), each with `hash` and `fields`: see
    [More of the cluster than its release](../commands/snapshot/cluster.md#more-of-the-cluster-than-its-release).
    `pending`, only when true: the record is of `cluster:<cluster>@pending`,
    and holds no `updatesPending`. `kind`, only for an object that is not a
    cluster: `edgeHost` or `clusterGroup`; `uid` and `name` are then those of
    the edge host or the group. `members[]`, for a cluster group: `uid`, `name`,
    and `projectUid` of each virtual cluster.
    None of these is part of the root.
    Each entry of `profiles[]` has `uid`, `name`, `version`, and `type`, in the
    order [The order of the profiles of a cluster](../internals/palette/clusters.md#the-order-of-the-profiles-of-a-cluster)
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
  - `registryProviderType`, only for a pack from an OCI registry: `pack`,
    `helm`, or `zarf`, the provider type of the registry, from the annotations of
    the pack reference. Palette follows a new digest in a different way for each:
    see [The digest of each registry kind](../internals/palette/packs.md#the-digest-of-each-registry-kind).
    It is outside the root.
  - `params`: the params of the pack on the profile, as a map. For an app tier,
    its properties: each name and its value.
  - `values`: the raw values YAML. `valuesCanon`: the values as JSON with
    sorted keys, or `null` when the values do not parse. `valuesHash`: the
    sha256 of `valuesCanon`, or of the trimmed text when the values do not
    parse. `valuesParseError`, only then: why.
  - `manifests[]`, sorted by name: `name`, `uid`, `digest`, `content`,
    `contentHash`, and `contentParseError` when the content does not parse.
  - `images`, only when the values declare some: each image of
    `pack.content.images`, as a reference with a tag, sorted. `charts`, only
    when the values declare some: `name`, `version`, and `repo` of each chart of
    `pack.content.charts`. Both are outside the root: the values are in it. A
    record written before these fields gets them from its values when strata
    reads it.
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
  `imagesChanged` and `chartsChanged` (each image and chart added, removed, or
  moved, across the packs), and `maxImpact`.
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
  - `images` and `charts`, only when true: the `IMAGES` and `CHARTS` flags.
    `imageChanges[]` and `chartChanges[]`, only when not empty: `op`, `name`
    (the image repository, or the chart name), `old`, and `new` (the tag or the
    version).
  - `paramChanges[]`, only when not empty: `op`, `name`, `old`, and `new` of
    each param that differs. `paramsChanged` is `true` then. For an app tier,
    the params are its properties.
  - `upstreamUnknown`, only when true: a pack from a registry with no upstream
    digest on either side.
- `upstreamUnknown`, only when it is not empty: the key of each such pack. An
  equal root does not show an equal archive for it.
- `macros`, only when it is not empty: each user macro that either side uses.
- `macrosChanged`, only when it is not empty: between two cluster records with
  `macroValues`, each macro whose value differs.
- `clusterTiers`, only between two cluster records that carry tiers: for each
  tier that both carry, and for `labels`, `tier`, `same`, and `diff[]` with the
  fields that differ. A difference here does not change `same`.
- `runningBehind`, only when it is not empty: for each cluster side with a
  `running` tier, each pack whose reconciled version differs from its profile,
  with `side` (`a` or `b`), `pack`, `declared`, and `running`.
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
  - `renamedTo`: for a profile record, the `name@version` that the uid names
    now, when the version is the same and the name is not. The version was
    renamed. The status is `drifted`, also when the roots are equal.
  - `gone`, only when true: Palette no longer has the object of the record. For
    a profile version, an app profile, an edge host, or a cluster group, the uid
    is gone. For a cluster, the uid is gone, or the document has the state
    `Deleted` or `metadata.isDeleted`. The status is `drifted`, and each pack of
    the record is `removed`.
  - `deletedAt`: for a gone object whose document Palette keeps, the time of
    the delete, from `metadata.deletionTimestamp`.
  - `state`: for a record of a cluster, an edge host, or a cluster group, the
    state of the object now, as Palette gives it: `status.state` of a cluster
    (such as `Running`, `Deleting`, or `Error`) or of an edge host (`ready`,
    `unpaired`, or `in-use`), and `active` or `inactive` from `status.isActive`
    of a cluster group. It does not change `status`.
  - `clusterValuesChanged`, only when it is not `0`: for a cluster record, the
    number of variable values that differ from the record.
  - `profilesOverwritten`: for a cluster record, each profile that the cluster
    runs whose uid names another version now, or no longer exists: `uid`,
    `name`, `clusterVersion`, and `profileVersion` (empty for a uid that is
    gone). The status is `drifted`.
  - `registrySync`: for a pack record, the last sync of the registry in
    Palette: `lastSyncedTime` (empty when no sync completed), `status`
    (`Completed`, `InProgress`, or `Failed`), and `message` when Palette gives
    one. The verdict states the copy of the registry in Palette, and this field
    tells how old that copy is. It is absent when the registry does not sync,
    or when Palette does not answer.
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
  - A record of a cluster, an edge host, or a cluster group adds `state`, as
    in `verify.state`, whatever its status.
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
- `consumers[]`: `kind`, `cluster`, `edgehost`, or `template`, `uid`, `name`,
  `projectUid`, `state`, `version`, and `error` when strata cannot read the
  cluster. `projectName` joins them when strata could read the project listing.
  The names and the projects come from `status.inUseClusters` of the profile,
  and from `status.inUseClusterTemplates` for a cluster template. A cluster
  template has an empty `state` and an empty `version`. `unlisted`, only when true: the profile
  did not list the cluster, and the second reading found it.
- `clustersRead`: the second reading. `read`: the cluster documents read.
  `unread`: the clusters the listing names that strata could not read. `error`:
  why strata could not read the cluster listing. Then `read` is `0`.
  `skipped`, only when true: `--list-only` asked for no second reading.
- `edgeHostsRead`: `read`, the edge hosts listed, and `error`, why strata could
  not list them. Then no edge host counts.

## strata-list/1

- `projectUid` and `projectName`: the scope.
- `pack`: the `--pack` text, when given.
- `cluster`: the `--cluster` text, when given.
- `query`, with `--tag` or `--filter`: `tags[]` and `filter`, as given.
  `resolved`: `search` when the search of Palette found the clusters, and
  `listing` when strata matched the labels of the listing itself.
  `queryFilter`, with `--filter`: the saved filter, as `filters[]` gives it.
- `profiles[]`, `clusters[]`, `projects[]`, and `filters[]`: only the kinds that were listed.
  A kind that was listed and holds nothing is `[]`. Profiles come by name and
  then by version, clusters by name, and projects by name.
  - A profile: `ref`, `name`, `version`, `uid`, and `cloudType`. With `--packs`,
    also `type`, `consumers`, and `packs[]` with `layer`, `name`, `tag`, and
    `version`, or `error`.
  - A cluster: `ref`, `name`, `uid`, `state`, `cloudType`, and `projectUid`.
    `projectName` joins them when strata could read the project listing.
    `labels`: the labels of the cluster, as a map; a tag with no value has the
    value `spectro__tag`.
    With `--applied`, also `profiles[]` with the `uid`, `name`, `version`, and
    `type` of each profile the cluster applies, or `error`, and
    `updatesPending` and `pendingUpdates[]` as a cluster record gives them.
    `updatesPending` is absent when strata could not read the notifications.
  - A project: `name` and `uid`.
  - A saved tag filter: `name`, `uid`, `conjunction` (`and` or `or`), and
    `conditions[]`, each with `key`, `values[]` (empty when the key alone must be
    there), and `negation` when true.

## strata-packs/1

- `name`: the pack that was asked for, and `cloud` when `--cloud` was given.
- `registries[]`: one entry per registry that has the pack, with `registry`,
  `registryUid`, `layer`, and `versions[]`.
  - A version: `version`, `packUid`, `digest`, `type`, `cloudTypes[]`, `tags[]`
    with the exact tag first, and `latest`, which is true for the newest version
    the command listed. With `--cloud`, that is the newest version for that
    cloud, and not the newest the registry holds. `state` is `deprecated`,
    `disabled`, or `deleted` when the listing gives one, and strata leaves out
    the field when it gives none. `listed`: when Palette first listed the
    version, from `metadata.creationTimestamp` of the listing; absent when the
    listing gives no time.
  - `versionsOffered`: how many versions of the pack the registry holds, before
    `--latest` or `--cloud` cut the list.
  - `tagsUnavailable`: `true` when Palette lists the pack under the registry and
    then answers 404 for it. The versions come from the listing, with no tags.
    When it is `false`, strata leaves out the field.
  - `state`: the state that every version in the registry has, when they all
    have the same one. Strata leaves out the field otherwise.

## strata-behind/1

- `records[]`: the record files read, in the order given. `since`: with
  `--since`, when the previous report was written.
- `pins[]`: one entry for each pack, registry, pinned version, and tag, in the
  order of the pack name:
  - `name`, `layer`, `registry`, `registryUid`, `version`, and `tag`.
  - `floating`, only when true: the tag is a floating tag. `resolved`: the
    version that the tag names now.
  - `pinnedState`: `deprecated`, `disabled`, or `deleted` from the registry, or
    `absent` when the registry no longer lists the pinned version.
  - `pinnedBy[]`: `path` and `subject` of each record that pins it.
  - `newer[]`: the newer versions that a person can move to, newest first, each
    with `version`, `state` when the registry gives one, and `listed`.
  - `behind`: `true` when `newer` is not empty.
  - `bump`, for a pin that is behind, unless `--no-impact`: `to`, the newest
    version, `impact`, the impact level, and `flags[]`.
  - `arrived[]`, with `--since`: the newer versions that the previous report
    did not know. Absent when the previous report had no pin of the pack in
    that registry.
  - `error`, when strata could not check the pin.
- `summary`: `pins`, `behind`, `floating`, `errors`, and `arrived`.


## strata-preflight/1

The report of `strata preflight`: each rule that applies to the engine, and
what it answered.

- `subject`: `cloud`, `release` when given, `source` (a file, `cluster:<uid>`,
  or both with a change), `name` of the cluster, and `clusterUid` for an
  existing cluster.
- `options`: `tiers` that ran, `strict`, and `ready` when given.
- `checks[]`: one entry for each rule, in the order of the catalog.
  - `id`, `title`, `section`, `tier`, `level`, `severity`, `status`.
  - `verdict`: `passed`, `failed`, `not-checked`, `not-run`, `skipped`, or
    `error`. `not-run` is a check that applies and did not run: a tier not asked
    for, no key, or `--no-exec`; it counts against the readiness.
  - `messages[]`: one for each failure, or the reason for another verdict.
  - `elapsedMs`, and `ran`, the request or the command the check ran, with
    `answer`, its HTTP status or exit code.
  - `reproduce[]`: the lines a person runs to see the same thing.
  - `fix` with `owner` and `text`; `evidence[]` with `kind`, `ref`, `note`.
  - `confidence`, when the ledger has runs of the rule on this engine: `catch`
    and `clean`, each with `correct`, `total`, `rate`, and `lowerBound`; `bar`;
    `meetsBar`; and `runs[]`.
  - `countsAsFailure`: true when this entry makes the exit code `1`.
- `sections[]`: the scorecard, one line for each section with a row for the
  engine: `rows`, `seen`, `withCheck`, `preflight`, `withRuns`, `weakestBound`, `ran`,
  `passed`, `failed`, `notRun`, `notChecked`, `readiness`.
- `summary`: the counts, `notRunReasons` (each reason with its count),
  `readiness`, `exitCode`, and one line, `verdict`.
- `options`: `tiers`, `strict`, `ready`, and `outDir` when `--out` was given.
- `legend`: what each verdict, level, tier, and exit code means.

[The rule format](./preflight-rules.md) states the section file, the ledger,
and the fixtures.
