# JSON documents

Each document has a `schema` id and a `tool` block with `name`, `version`, and
`impl`. `impl` is `rust`, or `bash` for a record of the bash reference
implementation. A record of the first bash release has no `impl`.

`strata-diff/1`, `strata-verify-summary/1`, and `strata-chain/1` also have a
`legend`. It explains their values in words, so a program or a language model
can read them without this book. `strata schema <kind>` prints the JSON Schema
of each document. A field of a closed set in a record or in `strata-behind/1` also
takes a value of a later release. See
[A value that strata does not know](compatibility.md#a-value-that-strata-does-not-know).

An `error` in a document is the error document of [Errors](./errors.md): `kind`,
`message`, and the fields of the kind. Each time in a document is a time in RFC
3339. A time that Palette gives is empty or absent when Palette gives none. The
schema states the shape of each open field: see
[The shape of each open field](../commands/schema.md#the-shape-of-each-open-field).

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
| `strata-set/1` | `snapshot --dir --json` for a `tag:` or `filter:` set | `set` |
| `strata-report/1` | `report --json` | `report` |
| `strata-preflight/1` | `preflight --json` | `preflight --schema report` |
| `strata-preflight-rules/1` | `preflight --rules --json` | `preflight --schema rules` (one section) |
| `strata-preflight-ledger/1` | the ledger of runs | `preflight --schema ledger` |
| `strata-preflight-promotion/1` | `preflight --promotion --json` | `preflight --schema promotion` |
| `strata-unknown/1` | the log of the unknown values, one line for each value | none |

`strata schema report` and `strata preflight --schema report` print two different schemas.

## strata-snapshot/1

A record of one profile version, cluster, edge host, cluster group, app
profile, or pack version.

- `root`: the sha256 digest of the release. See
  [The hashes and the root](../internals/hashes.md).
- `contentDigest`: the sha256 digest of what the version is, without the ids
  that Palette gives. Two publishes of one definition can have two roots and
  one content digest. One case is a definition that holds a manifest pack.
  Another case is a second publish that takes a pack from another registry. An
  older record has no such field, and the library calculates the digest from
  the pack hashes. See
  [The content digest](../internals/hashes.md#the-content-digest).
- `source`: where the record comes from.
  - `ref`: the ref as typed, a name or a uid, without an empty part after a
    separator: `cluster:<uid>/` records `cluster:<uid>`, and
    `cluster:<uid>@pending/` records `cluster:<uid>@pending`.
  - `uid`, `projectUid` (empty for tenant scope), `name`, `version`,
    `cloudType`, `type`, and `tags`. `type` is `cluster`, `infra`, `add-on`, or
    `system` for a cluster profile, as Palette gives it in `v1ProfileType`.
    Strata writes `app` for an app profile and `pack` for a pack record.
  - `template`: `published`, `draft`, `applied` for a cluster, `candidate`, or
    `registry` for a pack record. `draft` is a record of `<ref>@draft`, or of a
    version that was never published.
  - `consumers`, for a published or a draft version: `clusters[]`, the uids of
    the clusters that used the version, and `count`. A candidate, a cluster
    record, a pack record, and an app profile record have no `consumers`. The
    consumers of an app profile are app deployments, and not clusters.
  - A record of strata 0.18 or 0.19 can hold `clusterTemplates[]`. Strata
    ignores it. See
    [The cluster templates of a version](../commands/snapshot/profile-version.md#the-cluster-templates-of-a-version).
  - `variablesUnavailable`, only when true: Palette did not serve the variables
    endpoint, so the root is the root of a version with no variables. See
    [When Palette does not serve the variables](../internals/hashes.md#when-palette-does-not-serve-the-variables).
  - `fetchedAt`: the time of the read.
  - `paletteValidation`, only with `snapshot --validate`: `accepts`, and
    `errors[]` of `pack`, `code`, and `message`, only when it is not empty.
    `accepts` is `true` when Palette gives no error. Strata writes a record
    only when Palette takes the candidate or the packs, so a record holds
    `{"accepts": true}`. A library caller gets the errors too. Strata does not
    read a record that says `accepts` is `false` and gives no error, or that
    says `true` and gives an error. The field is not part of the root: see
    [The validate routes](../internals/palette/validate-routes.md#post-v1clusterprofilesvalidatepacks).
  - `macroValues`, only with `snapshot --macros` on a profile version: each
    user macro that the content uses. Each macro has `sha256:` of the value that
    Palette gives it at the scope of the version, or `unresolved`. A cluster
    record and a cluster group record hold them in `cluster.macroValues`. Not
    part of the root.
  - `pack`, for a pack record: the `name` and `version` of the pack, and the
    `registryUid` and `registryName` it came from. `packUid` is the uid of that
    version in the registry. `template` is `registry` and `type` is `pack`.
  - `cluster`, for a cluster record. None of its fields is part of the root.
    - `uid` and `name` of the cluster, the `profile` that
      `cluster:<cluster>/<profile>` selected, `profiles[]`, and `variables[]`.
    - `updatesPending` is `true` when Palette says that an update of a profile
      of the record waits. It is `false` when none waits. It is absent when
      strata could not read it. `errors.updatesPending` then says why.
      `pendingUpdates[]` has `profileUid`, `profile`, `message`, and `since`
      for each change that waits.
    - `repavePending` is `true` when Palette holds a repave of the cluster for
      approval, and `false` when none waits. It is absent for an edge host, a
      cluster group, and an older record. `pendingRepave`, only when a repave
      waits: `source`, `since`, and `reasons[]` with `code`, `message`, `pack`,
      and `keys`. `errors.pendingRepave` says why when strata could not read
      the repave. See
      [Whether a repave waits](../commands/snapshot/cluster.md#whether-a-repave-waits).
    - `errors`, only when strata could not read a part of the cluster that is
      beside the copy: the field and the error. The keys are `updatesPending`
      and `pendingRepave`. A key of a later release loads too. The tiers have
      `errors` in the same shape. A record of strata 0.24 and before holds
      `updatesPendingError` and `pendingRepaveError` in place of these two
      entries. Strata reads them into `errors`, and a value that is not text gives
      `snapshot-schema` with the older key in `field`, such as
      `source.cluster.pendingRepaveError`.
    - `macroValues`, only with `snapshot --macros`: each user macro that the
      content uses, with a digest. The digest is the `sha256:` of the value
      that Palette gives the macro for this cluster or cluster group. It is
      `unresolved` when Palette gives no value.
    - `labels`: the labels of the cluster, as a map. A label whose value is
      not text is not in the map: Palette gives a label as text.
    - `tiers`: each tier that the record carries, by name, each with `hash`
      and `fields`. The tiers are `applied`, `running`, `policy`, and, with
      `snapshot --shape`, `shape`. See
      [More of the cluster than its release](../commands/snapshot/cluster.md#more-of-the-cluster-than-its-release).
      The `fields` of `shape` are `cloudType`, `cloud`, and `pools[]`, by
      [The shape on each cloud](../commands/snapshot/cluster.md#the-shape-on-each-cloud).
      A record that an older version of strata wrote has no `cloudType`.
      - `pools[]` of `shape`: each pool holds `overrideClusterAPIConfig` and
        `overrideHealthCheckConfiguration` when it sets them. A record of
        strata 0.26.0 and before holds neither.
      - `errors`, only when strata could not read a field: the field and the
        error. `fields` then does not hold the field, so the hash does not
        cover it. `errors` is not part of the hash either. A record of strata
        0.19 holds `null` for the field.
      - The schema lists the keys of `fields` for each of these four tiers. It
        also takes the name of a tier of a later release, with any `fields`.
    - The `fields` of `policy` are in
      [The policy tier](../commands/snapshot/cluster.md#the-policy-tier). A
      record that an older version of strata wrote holds `resources` and `rbac`
      there, in place of `rbacs` and `namespaces`. A record that strata 0.18
      wrote holds `agentUpgrades` there, in place of
      `spectroComponentsUpgradeForbidden`.
    - A cluster group carries `policy` alone. Its `fields` are `endpointType`,
      `kubernetesDistroType`, `limitConfig`, `values`, `valuesHash`,
      `valuesParseError` when the values are not YAML, and `hostClusters[]`
      with `uid` and `endpointConfig`. `values` holds `********` over each
      value whose key ends with `password`, and `valuesHash` is the hash of
      the masked values. Values with no canonical form give `values` as `null`,
      and `valuesHash` is the hash of the masked text. See
      [The config that a cluster group gives](../commands/snapshot/other-objects.md#the-config-that-a-cluster-group-gives).
    - `pending`, only when true: the record is of `cluster:<cluster>@pending`,
      and holds no `updatesPending` and no `repavePending`.
    - `kind`, only for an object that is not a cluster: `edgeHost` or
      `clusterGroup`. `uid` and `name` are then those of the edge host or the
      group. `members[]`, for a cluster group: `uid`, `name`, and `projectUid`
      of each virtual cluster.
    - Each entry of `profiles[]` has `uid`, `name`, `version`, and `type`, in
      the order that
      [The order of the profiles of a cluster](../internals/palette/clusters.md#the-order-of-the-profiles-of-a-cluster)
      gives.
    - Each entry of `variables[]` has `profile`, `profileUid`, `name`,
      `value`, and `sensitive`. `verify` reads only the uids, so a rename does
      not stop it. A sensitive variable has `sensitive: true`, and no value or
      hash.
- `variables` and `variablesHash`: the profile variables, sorted by name, and
  the hash of the list. A variable has twelve fields: `name`, `displayName`,
  `description`, `format`, `defaultValue`, `regex`, `required`, `immutable`,
  `hidden`, `isSensitive`, `inputType`, and `options[]` with `label`, `value`,
  and `description`. `format` and `inputType` are as Palette gives them in
  `v1VariableFormat` and `v1VariableInputType`. A field that Palette does not
  give is `string` for `format`, `text` for `inputType`, `false` for a flag, and
  empty text for the rest. A variable with no `name` has an empty `name`. The
  schema requires `name` alone, so a candidate can give a variable with a
  `name` and a `defaultValue`.
- `packs[]`: one entry for each pack, in the order the profile holds them. The
  diff document sorts its packs by layer and then by name; the snapshot does
  not.
  - The identity fields: `key` (`<layer>/<name>`), `name`, `layer`, `type`,
    `tag`, `version`, `packUid`, and `registryUid`. `layer` is the layer name
    as Palette gives it in `v1PackLayer`, and `type` is the pack type as
    Palette gives it in `v1PackType`. Strata writes the layer `app` for each
    tier of an app profile. The `type` of such a tier is the type that Palette
    gives it in `v1AppTierType`.
  - `refDigest`, the digest on the pack reference, and `registryDigest`, the
    digest of the pack in the manifests document.
  - `registryProviderType`, only for a pack from an OCI registry: `pack`,
    `helm`, or `zarf`. It is the provider type of the registry, from the
    annotations of the pack reference. Palette follows a new digest in a
    different way for each:
    see [The digest of each registry kind](../internals/palette/packs.md#the-digest-of-each-registry-kind).
    It is outside the root.
  - `annotations`, only when the pack reference has the annotation `imageId`:
    that annotation alone, as Palette gives it. A bring-your-own-image OS pack
    names its machine image in `imageId`. A record keeps no other annotation,
    because an annotation can hold a secret. It is outside the root. A record
    written before this field has none. A record of strata 0.17 to 0.19 holds
    each annotation, and strata reads only `imageId` from it.
  - `invalid` and `invalidReason`, only when Palette marks the pack reference
    invalid: `true`, and the reason, as Palette gives it in `isInvalid` and
    `inValidReason`. Palette sets the flag when the registry deletes the tag of
    the pack. They are outside the root, and `snapshot` writes a warning on
    standard error for each such pack. A record written before these fields has
    neither.
  - `params`: the params of the pack on the profile, as a map of text. For an
    app tier, its properties: each name and its value. They also hold
    `installOrder`, the install order of the tier, as an integer, and
    `containerRegistryUid`, each only when Palette gives it. See
    [Edge hosts, cluster groups, and app profiles](../commands/snapshot/other-objects.md).
  - `values`: the raw values YAML. Palette writes `********` over each value
    whose schema format is `password`. Strata does the same for the packs of a
    draft: see [Secrets in the values](../internals/palette/profiles.md#secrets-in-the-values).
    `valuesCanon`: the values as JSON with
    sorted keys, or `null` when the values do not parse. `valuesHash`: the
    sha256 of the canonical text of `valuesCanon` (see
    [The canonical form](../internals/hashes.md#the-canonical-form)), or of the
    trimmed text when the values do not parse. `valuesParseError`, only then:
    why.
  - `manifests[]`, sorted by name: `name`, `uid`, `digest`, `content`,
    `contentHash`, and `contentParseError` when the content does not parse.
  - `images`, only when the values declare some: each image of
    `pack.content.images`, as a reference with a tag, sorted. `charts`, only
    when the values declare some: `name`, `version`, and `repo` of each chart of
    `pack.content.charts`. Both are outside the root: the values are in it. A
    record written before these fields gets them from its values when strata
    reads it.
  - `presetOptions[]`: `parameter`, `targetKey`, and `option` of each
    selection, sorted by parameter. Only a pack that strata reads from the
    registry, for a candidate or a `pack:` record, has it. The values of the
    pack must select an option of the pack template. Palette can write the
    presets of such an option into the values: see
    [Template options](candidate-file.md#template-options). It is outside the
    root.
  - No `masked`. A record of 0.25.0 and before holds it, for a pack whose
    values hold a mask. A new record does not write it, and strata ignores it
    when it reads a record. Strata finds the paths in `valuesCanon`: each value
    that is the text `********`, sorted, in the form of the value paths of
    `strata-diff/1`. Palette writes that text over a value whose schema format
    is `password`, so the root covers the mask and not the secret. See
    [A secret changes in a pack value](../use-cases/masked-value.md).
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
  [strata-snapshot/1](#strata-snapshot1) gives it. A `cluster:` ref that selects
  no profile can hold only the profiles that the other side asks about. Then
  `ref` names them, as `cluster:<cluster>/<profile>`: see
  [Compare a profile version with a cluster](../commands/compare/clusters.md#compare-a-profile-version-with-a-cluster).
  - `a.projectUid` and `b.projectUid`: the project of each side. Strata reads
    a side in the project of `--project-a` or `--project-b`, else of
    `--project`, else of the configuration. A side from a record file keeps
    the project of the record. Empty is tenant scope. See
    [Two sides in two projects](../commands/compare.md#two-sides-in-two-projects).
- `same`: the roots are equal. `rootA` and `rootB` are the roots.
- `sameContent`: the roots are equal, or differ only by the pack uids of
  manifest packs and the tag names of the same builds. The variables and the
  pack order must be unchanged. `same` stays `false` in the second case.
- `editedInPlace`: both sides name the same uid and version, and the roots
  differ.
- `overwrittenInPlace`: each uid that the sides name under different versions,
  with `uid`, `name`, `versionA`, `versionB`, and `nameB` when the name changed
  too. Absent when empty.
- `summary`: `packsAdded`, `packsRemoved`, `packsChanged`, `packsUnchanged`,
  `identityChanges`, `upstreamChanges`, `authoredChanges`, `variablesChanged`,
  `imagesChanged`, `chartsChanged`, `packsInvalid`, and `maxImpact`.
  - `imagesChanged` and `chartsChanged`: each image and chart added, removed,
    or moved, across the packs.
  - `packsInvalid`: the packs that Palette marks invalid on one side or on the
    two sides.
- `variables`: `changed`, the two hashes, `diff[]` with paths
  `<variable>.<field>`, and `byName` with `added[]`, `removed[]`, and
  `changed[]`. An added or removed variable has `name`, `required`,
  `sensitive`, `hidden`, and `immutable`. A changed variable has `name` and
  `fields[]`, whose paths are relative to the variable, such as `defaultValue`.
- `clusterVariables`, only between two clusters: `changed`, and `values[]` with
  `op`, `profile`, `name`, `old`, `new`, and `sensitive`. A sensitive variable
  has no `old` or `new`. The values of each side are in `a.cluster.variables` and
  `b.cluster.variables`.
- `versionMove`: `forward` when side B has the higher version, and `back` when it
  has the lower. Absent when a side has no version, and when the two versions have
  the same precedence.
- `orderChanged`: the two sides apply their packs in a different order.
- `packs[]`: one entry for each pack key on either side, sorted by layer and then
  by name.
  - `status`: `unchanged`, `changed`, `added`, or `removed`.
  - `identity`, `upstream`, `authored`, and `paramsChanged`.
  - `tagRenamed`: only the tag name differs, for the same build.
  - `renumbered`, only when true: only the version changed, and it kept its
    numbers. The impact is then `none (renumber only)`. See
    [A renumber](./impact.md#a-renumber).
  - `versionMove`: `forward` or `back`, as at the top of the document, for the
    versions of the pack. Absent when a side has no pack, and when the two
    versions have the same precedence.
  - `mintedUidOnly`: the only difference is a pack uid that Palette mints. This
    is the case for a manifest pack, and for a pack from a registry that Palette
    does not sync.
  - `impact`: `level` and `text`.
  - `a` and `b`: `tag`, `version`, `packUid`, `registryUid`, `upstream`,
    `upstreamSource`, `valuesHash`, and `node`. `invalid` and `invalidReason`,
    only when Palette marks the pack of that side invalid.
  - `values`: `changed`, `parseable`, `changedCount`, `addedCount`,
    `removedCount`, and `diff[]`. `unified`, only with `--full` and when
    `changed` is `true`: see [The field unified](#the-field-unified).
  - `manifests[]`: `op`, `name`, `hashA`, and `hashB`. `unified`, only with
    `--full`.
  - `images` and `charts`, only when true: the `IMAGES` and `CHARTS` flags.
    `imageChanges[]` and `chartChanges[]`, only when not empty: `op`, `name`
    (the image repository, or the chart name), `old`, and `new` (the tag or the
    version).
  - `paramChanges[]`, only when not empty: `op`, `name`, `old`, and `new` of
    each param that differs. `paramsChanged` is `true` then. For an app tier,
    the params are its properties, `installOrder`, and `containerRegistryUid`.
  - `upstreamUnknown`, only when true: a pack from a registry with no upstream
    digest on either side.
  - `masked`, only when it is not empty: the path of each value that Palette
    masks on either side, sorted. An equal root does not show an equal secret.
- `upstreamUnknown`, only when it is not empty: the key of each such pack. An
  equal root does not show an equal archive for it.
- `macros`, only when it is not empty: each user macro that either side uses.
- `macrosChanged`, only when it is not empty: between two records with the
  digests of their macro values, each macro whose value differs.
- `imageIdsChanged`, only when it is not empty: each pack that names an image
  in `imageId` on both sides, with two different images. Each entry has `key`,
  `a`, and `b`. `compare` exits `1` for it, also when `same` is `true`.
- `clusterTiers`, only between two cluster records of one kind that carry
  tiers. It has one entry for each tier that both carry, and one for `labels`.
  Each entry has `tier`, `same`, and `diff[]` with the fields that differ. A
  difference here does not change `same`.
- `runningBehind`, only when it is not empty: each pack whose reconciled
  version differs from its profile. Strata checks each cluster side with a
  `running` tier. Each entry has `side` (`a` or `b`), `pack`, `declared`, and
  `running`.
- `paletteRepave`, only with `compare --repave`: `clusterUid`, `required`
  (Palette's `isRepaveRequired`), `reasons[]` with `code`, `message`, `pack`, and
  `keys` (the config keys that differ), and `agrees`. `agrees` is `false` when
  the most severe level of the table and Palette's answer disagree.
  `clusterTemplate`, only for a cluster that a cluster template governs, names
  the template that Palette answered for, with `uid` and `projectUid`.
- `paletteValidation`, only with `compare --validate`: `clusterUid`, `accepts`,
  and `errors[]`. `accepts` is `true` when Palette gives no error. `errors[]`,
  only when it is not empty, holds `pack`, `code`, and `message`. An error of
  the whole request has no `pack`. Strata sorts the errors and keeps one copy of
  each. Strata does not read a document that says `accepts` is `false` and
  gives no error, or that says `true` and gives an error.
- `verify`, only after `verify`: `status` (`unchanged`, `drifted`, or
  `unverified`), `uid`, `recordedAt`, and `checkedAt`.
  - `upstreamUnknown`: for an `unverified` record, the packs that make it so.
  - `macrosChanged`: for a record with the digests of its macro values, each
    macro whose value moved. The status is `drifted`, also when the roots are equal.
  - `imageIdsChanged`: when the roots are equal, each pack that names another
    image in `imageId` now, with `key`, `a`, and `b`. The status is `drifted`.
  - `invalidPacks`, only when it is not empty: each pack that Palette marks
    invalid now, with `key` and `reason`. It does not change `status`: the
    content is the recorded content.
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
    state of the object now, as Palette gives it. It does not change `status`.
    - A cluster: `status.state`, such as `Running`, `Deleting`, or `Error`.
    - An edge host: `status.state`, which is `ready`, `unpaired`, or `in-use`.
    - A cluster group: `active` or `inactive`, from `status.isActive`.
  - `clusterValuesChanged`, only when it is not `0`: for a cluster record, the
    number of variable values that differ from the record.
  - `profilesOverwritten`: for a cluster record, each profile that the cluster
    runs whose uid names another version now, or no longer exists. Each entry
    has `uid`, `name`, `clusterVersion`, and `profileVersion` (empty for a uid
    that is gone). The status is `drifted`.
  - `registrySync`: for a pack record, the last sync of the registry in
    Palette. The verdict states the copy of the registry in Palette, and this
    field tells how old that copy is. It is absent when the registry does not
    sync, or when Palette does not answer. It has:
    - `lastSyncedTime`, the start of the last sync that completed. Empty when
      no sync completed.
    - `lastRunTime`, the start of the last sync.
    - `status`: `Completed`, `InProgress`, or `Failed`.
    - `message`, when Palette gives one.
    - `syncRecovery`, when a sync waits for a recovery: `syncRecoveryState`
      (`Interrupted` or `Recovering`) and `message`.
  - `registrySyncError`: for a pack record, why strata could not read the last
    sync of the registry. Palette answered with a 5xx, or the request did not
    complete. `registrySync` is then absent. It does not change `status`.
  - `who`, only with `--who` and for a drifted record that is not a pack
    record. It lists each write to the object of the record that the audit log
    of Palette holds since `recordedAt`, oldest first. For a cluster record, it
    also lists each write to a profile of `profilesOverwritten`. An empty list
    means the log holds no write since the record. Each entry has these fields:
    - `time`, `actor`, `actorUid` (for a user), and `auditUid`.
    - `actorType` and `action`, as Palette gives them in `v1AuditActor` and
      `v1AuditActionType`.
    - `summary`, when Palette gives one, and `userMsg`, the note of a person,
      when the entry has one.
    - `resource`, when Palette gives it: the `kind`, the `name`, and the `uid`
      of the object of the write.
  - `whoError`, only with `--who` and for a drifted record that is not a pack
    record: why strata could not read the audit log. Palette answered with an
    error, such as HTTP 403 or HTTP 500, or the request did not complete. `who`
    is then absent. The error does not change `status`. The text is
    `HTTP <status>: <body>`, or the message of the request that did not complete.
  - `lastModified`: when Palette last changed the object of the record, from
    `metadata.lastModifiedTimestamp` of its document. The document is the
    profile, the app profile, the cluster, the edge host, or the cluster group.
    It is absent when Palette gives no time, and for a gone object. It is also
    absent for a pack record: the pack listing gives the create time of the
    pack there. See
    [What a registry answers about a pack](../internals/palette/packs.md#what-a-registry-answers-about-a-pack).

Each changed value is a leaf with `op`, `path`, `segments`, `old`, and `new`. A
leaf is a scalar or an empty container. The path joins keys with a dot. A list
item is `[i]`. When each item of a list is an object with a unique `name`, the
item is `[name=<value>]`, so an inserted item shows as one addition.

`segments` is the path as a list. A key is a string, an index is an integer,
and a named item is an object `{"name": ...}`. The schema gives each segment an
`anyOf` of these three types.

### The field unified

`unified` is a string: the unified diff of the raw text, as `--full` prints it.
It has no color and three lines of context. It starts with the two lines
`--- a: <key> values` and `+++ b: <key> values`. For a manifest, the lines end
with `manifest <name>` in place of `values`.

`compare --json --full` writes the field. So does `verify --json --full`, for one
record. Without `--full`, the document has no `unified`.

- A pack with the `status` `changed` has `values.unified` when its values
  changed.
- Each entry of `manifests[]` of that pack has `unified`. An added manifest or
  a removed manifest has its text against an empty side.
- A pack with another `status` has no `unified`. `--full` prints no diff for it
  either.

The text is the text of the records. No value of a cluster variable is in it.
Palette masks a secret as `********`, and the mask stays in the text. A change
of the secret does not show, because the two sides hold the mask: see
[A secret changes in a pack value](../use-cases/masked-value.md).

## strata-verify-summary/1

- `legend.status`: `unchanged`, `drifted`, `unverified`, and `error`.
- `summary`: `records`, `unchanged`, `drifted`, `errors`, and `status`, which is
  the exit status in words. `unverified`, only when it is not `0`: the records
  whose content is the same and that hold a pack with no upstream digest.
- `records[]`: one entry for each file, in the order given. An entry has
  `path`, `status`, `name`, `version`, `uid`, `projectUid`, `recordedAt`,
  `rootRecorded`, and `rootLive`. `rootLive` is there when strata read the live
  profile. `recordedAt` is empty when strata could not read the file.
  - A drifted record adds `maxImpact`, `packsChanged`, and the full `diff`. An
    unverified record adds the `diff`, which names the packs. With `--full`, the
    `diff` of each such record holds `unified`: see
    [The field unified](#the-field-unified).
  - A cluster record with changed variable values adds `clusterVariables`,
    whatever its status.
  - A record of a cluster, an edge host, or a cluster group adds `state`, as
    in `verify.state`, whatever its status.
  - A record adds `lastModified`, as in `verify.lastModified`, whatever its
    status.
  - A record adds `invalidPacks`, as in `verify.invalidPacks`, whatever its
    status.
  - A failed record adds `error`. See [Errors](./errors.md).

## strata-chain/1

- `legend`, and `policy`: `exact` or `registry-move`. `allowNoDigest`, only
  when true: `--allow no-digest` was given. `allowRenumber`, only when true:
  `--allow renumber` was given.
- `same`: each pair passed.
- `summary`: `pairs`, `passed`, and `failed`.
- `records[]`: `label`, `name`, `version`, `uid`, `projectUid`, and `root`.
  `projectUid` is the project of the record, and empty is tenant scope. The
  first ref is side A of `--project-a`, and the second is side B of
  `--project-b`. The pair `pairs[n]` compares `records[n]` with
  `records[n+1]`, so these two give the project of each side of the pair. See
  [Two projects in one run](../commands/same-release.md#two-projects-in-one-run).
- `pairs[]`: `a`, `b`, `status` (`identical`, `same-content`, `registry-move`,
  `renumber`, `different`, or `unknown-upstream`), `rootA`, `rootB`,
  `packsRepinned`, `packsRetagged`, and `reasons[]`. A pack that names another
  image in `imageId` on each side makes the pair `different`, also when the
  roots are equal.
  - `renumbers[]`, only with `--allow renumber` and when it is not empty: each
    pack whose version alone changed, with `key`, `name`, `old`, and `new`.
    `preReleaseToFinal`, only when true: `new` is the final version of the
    pre-release `old`, in Semantic Versioning. A pair that fails for another
    reason lists its renumbers too. See
    [A release under new version numbers](../commands/same-release.md#a-release-under-new-version-numbers).
  - `archivesNotCompared`, only when it is not empty: the key of each pack of
    `renumbers[]` whose archive can differ. Its digest changed, or Palette gives
    none. Strata does not compare the files of the two archives.
    A manifest pack has no archive, so it is never in the list.
  - `upstreamUnknown`, only when it is not empty: the packs with no upstream
    digest, also when `--allow no-digest` accepts them.
  - `mintedUidOnly`, only when it is not empty: the packs whose only difference
    is a pack uid that Palette mints. This is the case for a manifest pack, and
    for a pack from a registry that Palette does not sync.
  - `masked[]`, only when it is not empty: `key` and `paths` of each pack with
    a value that Palette masks, on either side. It does not change `status`.
  - `macrosChanged`, only when it is not empty: when both records hold the
    digests of their macro values, each macro whose value differs. It does not
    change `status`.
  - `macrosNotCompared`, only when it is not empty: when a record holds no
    digests, each macro that the content of either record uses.

## strata-consumers/1

- `profile`: `uid`, `name`, and `version`. `type`, only for an app profile:
  `app`. The schema gives `type` the six profile types: `cluster`, `infra`,
  `add-on`, `system`, `app`, and `pack`.
- `count`: the number of consumers. Exit `1` when it is not `0`. Exit `0` when
  it is `0` and each reading succeeded. Exit `2` when it is `0` and a reading
  failed: `error`, `groupsError`, or an `unread` that is not `0` in
  `clustersRead`, or `error` in `edgeHostsRead`.
- `overwritten`: the number of consumers whose copy names another version.
- `consumers[]`: `kind`, `uid`, `name`, `projectUid`, `state`, `version`, and
  `error` when strata cannot read the cluster or the app deployment. `kind` is
  `cluster`, `edgehost`, `template`, or `app` for an app deployment.
  `projectName` joins them when strata could read the project listing. The
  names and the projects come from `status.inUseClusters` of the profile, from
  `status.inUseClusterTemplates` for a cluster template, and from
  `status.inUseApps` of an app profile. `unlisted`, only when true: the
  profile did not list the cluster, and the second reading found it.
  - `state`: `status.state` of the cluster, as Palette gives it in
    `v1ClusterState`, or `unknown` when strata cannot read the cluster. A
    cluster template has an empty `state` and an empty `version`.
- `clustersRead`: the second reading.
  - `resolved`: `search` when strata asked the cluster search for the clusters
    that run a version of the profile, by its name. It is `listing` when the
    search of Palette takes no `clusterProfiles` and strata read each cluster
    of the listing.
  - `read`: the cluster documents read, one for each cluster that the search or
    the listing names and the consumer list does not.
  - `unread`: the clusters of those that strata could not read.
  - `error`: why strata could not read the search or the cluster listing. Then
    `read` is `0`.
  - `groups`, only when not zero: the cluster groups that pin the version,
    whose virtual clusters strata read.
  - `members`, only when not zero: the virtual clusters of those groups that
    strata read, which also count in `read` or `unread`.
  - `membersUnread`, only when not zero: the virtual clusters of `members` that
    strata could not read, which also count in `unread`.
  - `groupsError`: why strata could not read the cluster groups, or one of
    them.
  - `skipped`, only when true: `--list-only` asked for no second reading, or
    the profile is an app profile.
  - `reason`, only for an app profile: why strata did no second reading.
- `edgeHostsRead`: `read`, the edge hosts listed, and `error`, why strata could
  not list them. Then no edge host counts. `skipped` and `reason`, only for an
  app profile: strata reads no edge host, and why.

## strata-list/1

- `projectUid` and `projectName`: the scope.
- `pack`: the `--pack` text, when given.
- `cluster`: the `--cluster` text, when given.
- `query`, with `--tag` or `--filter`: `tags[]` and `filter`, as given.
  `resolved`: `search` when the search of Palette found the clusters, and
  `listing` when strata matched the labels of the listing itself.
  `queryFilter`, with `--filter`: the saved filter, as `filters[]` gives it.
- `includesVirtual`, with clusters: `true` when strata asked the search of
  Palette for the virtual clusters, so `clusters[]` holds them. `false` when
  Palette has no search, or its search takes no `environment`: the list then
  holds no virtual cluster.
- `profiles[]`, `clusters[]`, `projects[]`, and `filters[]`: only the kinds that were listed.
  A kind that was listed and holds nothing is `[]`. Profiles come by name and
  then by version, clusters by name, and projects by name.
  - A profile: `ref`, `name`, `version`, `uid`, `cloudType`, and `scope`.
    `scope` is `project`, `tenant`, or `system`.
    - `scope` is absent when Palette has no profile search, or when strata
      could not read the document of the version with `--packs`.
    - With `--packs`, also `type`, `consumers`, `published`, and `packs[]` with
      `layer`, `name`, `tag`, and `version`, or `error`.
    - `consumers` is the number of consumers that the profile document names:
      its clusters, and the cluster templates that link the version.
      `consumersByKind` gives the count of each kind, `clusters` and
      `templates`. In strata 0.20 and before, `consumers` counted the clusters
      alone.
    - `published` is the `status.isPublished` of the version. When it is
      `false`, nobody published it, and `packs[]` holds the packs of its draft.
    - `type` and `packs[]` come from the template that a record of the version
      reads. This is the published template when it has packs, and the draft
      when it has none. `type` is a profile type, or the text of a type of a
      later release.
  - A cluster: `ref`, `name`, `uid`, `state`, `cloudType`, `clusterType`, and
    `projectUid`.
    - `state` is `status.state` of the cluster, as Palette gives it in
      `v1ClusterState`. `clusterType` is the type as Palette lists it.
      `PureManage` is a cluster that Palette built. `AlloyMonitor`,
      `AlloyAssist`, and `AlloyExtend` are clusters that a person imported.
      `clusterType` is absent when the listing gave none.
    - `projectName` joins the fields when strata could read the project
      listing. `labels`: the labels of the cluster, as a map; a tag with no
      value has the value `spectro__tag`.
    - `virtual`, for a virtual cluster alone: `clusterGroup` and `hostCluster`,
      each with `uid` and `name`, as the search names them. `name` is absent
      when Palette gives none, and `hostCluster` is absent for a group of the
      system.
    - With `--applied`, the entry also has `profiles[]`, or `error`. Each entry
      of `profiles[]` has the `uid`, `name`, `version`, and `type` of a profile
      that the cluster applies.
    - With `--applied`, the entry also has `updatesPending` and
      `pendingUpdates[]`, as a cluster record gives them. `updatesPending` is
      absent when strata could not read the notifications.
  - A project: `name` and `uid`.
  - A saved tag filter: `name`, `uid`, `conjunction` (`and` or `or`), and
    `conditions[]`. Each condition has `key`, `values[]`, and `negation` when
    true. `values[]` is empty when the key alone must be there.

## strata-packs/1

- `name`: the pack that was asked for, and `cloud` when `--cloud` was given.
- `registries[]`: one entry per registry that has the pack, with `registry`,
  `registryUid`, `layer`, and `versions[]`. `layer` is the layer that the
  listing gives a version, else the layer that the registry route gives the
  pack. A record of a version reads it in the same order. Palette does not
  serve a version that has no tag in the answer of the registry. That version
  is not in `versions[]`, unless Spectro Cloud retired it as `disabled` or
  `deleted`. See [The FIPS rule](../internals/palette/packs.md#the-fips-rule).
  - A version: `version`, `packUid`, `digest`, `type`, `cloudTypes[]`, `tags[]`
    with the exact tag first, and `latest`.
    - `latest` is true for the version that `pack:<name>@latest` records, of the
      versions the command listed. That is the highest version that is not a
      pre-release, by the order of Semantic Versioning. A registry that has no
      version that is not a pre-release gives its highest pre-release. With
      `--cloud`, `latest` is true for that version among the versions for that
      cloud. With `--cloud`, the ref can record another version: see
      [Which version is the newest](../commands/packs.md#which-version-is-the-newest).
    - `type` is `spectro`, `helm`, or `oci`. A version that the listing gives
      no type is `spectro`, as its record says.
    - `state` is `deprecated`, `disabled`, or `deleted` from `system_state` of
      the listing, or `disabled` for a version with the mark of Spectro Cloud.
      See [Pack states](../internals/palette/packs.md#pack-states). Strata
      omits the field when the listing gives none.
    - `digest` is empty when the registry gives none.
    - `paletteLatest`, only when true: the version that Palette calls the
      latest, from `metadata.annotations.latestVersion` of the listing.
      `--latest` keeps it too.
    - `listed`: when Palette first listed the version, from
      `metadata.creationTimestamp` of the listing. It is absent when the
      listing gives no time.
  - `versionsOffered`: how many versions of the pack the registry offers, before
    `--latest` or `--cloud` cut the list.
  - `tagsUnavailable`: `true` when Palette lists the pack under the registry and
    then answers 404 for its tags, also with the `states` query. Only the
    versions that Spectro Cloud retired as `disabled` or `deleted` come from
    the listing, with no tags. When it is `false`, strata omits the field.
  - `state`: the state that every version in the registry has, when they all
    have the same one. Strata omits the field otherwise.
  - `noDigests`: `true` when the registry gives no digest for each version of
    the pack, before `--latest` or `--cloud` cut the list. When it is `false`,
    strata omits the field.

## strata-behind/1

- `records[]`: the record files read, in the order given. `since`: with
  `--since`, when the previous report was written.
- `pins[]`: one entry for each pack, registry, pinned version, and tag, in the
  order of the pack name. With `--pin`, the list holds the entries of that pack
  only. Each entry has:
  - `name`, `layer`, `registry`, `registryUid`, `version`, and `tag`.
  - `floating`, only when true: the tag is a floating tag. `resolved`: the
    version that the tag names now. When it is above `version`, it is in
    `newer`, and the pin is behind.
  - `pinnedState`: `deprecated`, `disabled`, or `deleted`, as `state` of
    `strata-packs/1`. It is `absent` when the registry no longer lists the
    pinned version, or does not serve it in this tenant.
  - `pinnedBy[]`: `path` and `subject` of each record that pins it.
  - `newer[]`: the newer versions that a person can move to, newest first, by
    the order of Semantic Versioning. A pre-release is a newer version of a pin that
    is not a pre-release only with `--pre-release`. Each has:
    - `version`.
    - `state`, when the registry gives one.
    - `paletteLatest`, when Palette calls it the latest.
    - `listed`.
    - `drops[]`, the architectures of the pinned version that it does not
      support.
  - `behind`: `true` when `newer` is not empty.
  - `bump`, for a pin that is behind, unless `--no-impact`: `to`, the newest
    version, `impact`, the impact level, and `flags[]`. `flags[]` holds
    `IDENTITY`, `UPSTREAM`, `AUTHORED`, `IMAGES`, or `CHARTS` for each change.
    - `paletteValidation`: the answer of Palette about the move. The answer holds
      the errors for each record that pins the pack in the project of the pin.
      - `accepts` is `true` when Palette gives no error. `errors[]` is there
        only when it is not empty.
      - Each error has `code` and `message`, and `pack` when it names a pack. It
        has `clusters[]` when it is about clusters. For a cluster record, this
        is the cluster of the record. For a profile version, these are the
        clusters that run the version and that the message names.
      - It is absent when strata can ask no record of the pin. It is also
        absent when a request failed or strata did not ask a record of another
        project, and no answer holds an error.
      - Strata does not read a document that says `accepts` is `false` and
        gives no error, or that says `true` and gives an error.
    - `paletteValidationError`: the message, when a request to Palette failed or
      strata did not ask a record of another project. An example is
      `1 record of project prod was not asked`. The pin stays behind.
  - `arrived[]`, with `--since`: the newer versions that the previous report
    did not know. Absent when the previous report had no pin of the pack in
    that registry.
  - `error`, when strata could not check the pin.
  - `registrySync`, for a pin that is current whose registry did not complete
    its last sync: the last sync, with the fields of `verify.registrySync` in
    [strata-diff/1](#strata-diff1). The copy of the registry in Palette is
    then the copy of an earlier sync, and `current` is against that copy.
    Absent for a pin that is behind, when the last sync completed, when the
    registry does not sync, and when Palette does not answer.
  - `registrySyncError`, for a pin that is current: why strata could not read
    the last sync of the registry. `registrySync` is then absent, and the pin
    stays current.
- `summary`: `pins`, `behind`, `floating`, `errors`, and `arrived`.

## strata-set/1

The report of `snapshot tag:<key>=<value> --dir DIR` or
`snapshot filter:<name> --dir DIR`. See
[A set of clusters](../commands/snapshot/set.md).

- `query`: `tags[]` and `filter`, as `strata-list/1` gives them. `resolved`:
  `search` or `listing`, as in `strata-list/1`.
- `summary`: `clusters`, the clusters that the query selects, `recorded`,
  `failed`, and `removed`. The exit code is `2` when `failed` is not `0`.
- `records[]`: one entry for each cluster of the set, by name.
  - `cluster`: the cluster, as a cluster of `strata-list/1`.
  - `path`: the file of its record.
  - `root`: the root of the record that strata wrote. `error`, in place of
    `root`: why strata could not record the cluster. See [Errors](./errors.md).
- `removed[]`: with `--prune`, each record file that strata removed.

## strata-report/1

The report of `strata report`: the values that Palette gave and that this release of strata does
not know. See [strata report](../commands/report.md) and
[What strata can report](./report.md).

- `schema`: `strata-report/1`.
- `unknown[]`: one entry for each different line of the log, in the order of `set`, `value`, and
  `route`. An entry has the fields of [`strata-unknown/1`](#strata-unknown1) except `schema`, and
  the field `count`.
  - `strata`, `set`, `value`, and `route`: as in the line of the log.
  - `count`: how many lines of the log are this entry. The endpoint refuses a count above 1,000,000.

The report has no `tool` block and no time, because the log has none. A log with no line gives
`unknown: []`. An entry holds no name, no uid, no host, no free text, and no value of a pack.

The send of the report posts this document to the endpoint, and the endpoint takes this document
alone. It refuses a field that is not in the list, as `strata report` does for the log. A new field
takes a new id: see [The documents](./compatibility.md#the-documents). The page
[Send the report by itself](./report.md#send-the-report-by-itself) says more on the send.


## strata-preflight/1

The report of `strata preflight`: each rule that applies to the engine, and
what it answered.

- `subject`: `cloud`, `release` when given, `source` (a file, `cluster:<uid>`,
  or both with a change), `name` of the cluster, and `clusterUid` for an
  existing cluster.
- `options`: `tiers` that ran, `strict`, `offline`, and `ready` when given.
  `offline` is true when the run read nothing from Palette, as with `--offline`.
- `checks[]`: one entry for each rule, in the order of the catalog.
  - `id`, `title`, `section`, `tier`, `level`, `severity`, `status`.
  - `verdict`: `passed`, `failed`, `not-checked`, `not-run`, `skipped`, or
    `error`. `not-run` is a check that applies and did not run: a tier not asked
    for, no key, or `--no-exec`; it counts against the readiness.
  - `messages[]`: one for each failure, or the reason for another verdict.
  - `elapsedMs`, and `ran`, the request or the command the check ran, with
    `answer`, its HTTP status or exit code. A check with a second command shows
    both commands in `ran`, with `; ` between them. Then `answer` has the exit
    code of each command in the same order, such as `exit 0; exit 254`.
  - `requests[]`: one entry for each request, with `method`, `path`, and
    `answer`. A rule with `each` has one entry for each item, in the order of
    the list, and no `ran` and no `answer` of its own. A check of kind `exec`
    with `palette` has one entry, the read of Palette, beside `ran` and `answer`.
  - `reproduce[]`: the lines a person runs to see the same thing. Each line
    reads, and changes nothing, with one exception. At `validate/repave`,
    Palette can save the address of the console of a pack of virtual
    machines. A line that sends that route says so in a comment. A line that
    reads Palette sends the key in a config of `curl`, and in no argument: see
    [The reproduce lines](./preflight-rules.md#the-reproduce-lines).
  - `fix` with `owner` and `text`; `evidence[]` with `kind`, `ref`, `note`.
    `owner` is `palette`, `provider`, `lab`, `strata`, `operator`, or `none`.
    `kind` is `run`, `source`, `doc`, or `report`. An entry with an empty
    reference has no `ref`, and an entry with no note has no `note`.
  - `confidence`, when the ledger has runs of the rule on this engine: `catch`,
    `clean`, `bar`, `meetsBar`, `runs[]`, and `olderRuns`. `catch` and `clean` each have
    `correct`, `total`, `rate`, and `lowerBound`. `olderRuns` is the number of runs
    of older releases, and of runs with no release, that the counts omit. It is
    `0` when the run names no `--release`.
  - `countsAsFailure`: true when this entry makes the exit code `1`.
- `sections[]`: the scorecard, one line for each section with a row for the
  engine. A line has `rows`, `seen`, `withCheck`, `preflight`, `withRuns`,
  `weakestBound`, `ran`, `passed`, `failed`, `notRun`, `notChecked`, and
  `readiness`.
- `summary`: the counts, `notRunReasons` (each reason with its count),
  `readiness`, `exitCode`, and one line, `verdict`.
- `options`: `tiers`, `strict`, `offline`, `ready`, and `outDir` when `--out` was given.
- `legend`: what each verdict, level, tier, and exit code means.

[The rule format](./preflight-rules.md) states the section file, the ledger,
and the fixtures.

## strata-preflight-promotion/1

The report of `strata preflight --promotion`: the level of each rule against the
runs of the ledger. See [Confidence and promotion](../commands/preflight/promotion.md).

- `release`, when the run names `--release`: the release whose runs count. With no
  `release`, each run counts.
- `rules[]`: one entry for each rule with a check. With a `release`, a rule that
  does not apply to it has no entry.
  - `id`, `section`, `tier`, `level` (the level of the rule file), and `earned`
    (the level that the runs give).
  - `advice`: `promote`, `demote`, or empty when the level and the runs agree.
  - `confidence[]`: one entry for each engine that has a run of the rule. An entry has
    `engine`, `release`, `catch`, `clean`, `bar`, `meetsBar`, `runs[]`, and
    `olderRuns`, as in `strata-preflight/1`. With no `release`, one entry for
    each engine and release.
- `disagreements`: the rules with `advice`. The exit code is `1` when it is not `0`.

## strata-unknown/1

One line of the log of the unknown values. No command prints it. Strata appends it to
`strata-unknown.jsonl` in the state folder of the user. It writes one line for each value of a
closed set that Palette gives and this release does not know. See
[The note for a value that Palette gives](compatibility.md#the-note-for-a-value-that-palette-gives).

A line is one JSON object with only these fields:

- `schema`: `strata-unknown/1`.
- `strata`: the version of strata that read the value.
- `set`: the name of the closed set, as `strata schema` names its definition. Examples are
  `PackState`, `OciProvider`, and `ProfileType`.
- `value`: the text of the value. It is one word of printable ASCII, from `!` to `~`, with no space.
  Each space and each other character becomes `?`. Strata cuts a value of more than 64 characters to 61 characters and `...`.
- `route`: the template of the route that gave the value, such as `/v1/clusterprofiles/{uid}`. It
  never holds the uid of a real request.

A line holds no name, no uid, no host, no free text, and no value of a pack. A test fails on each
other field.
