# JSON documents

Each document has a `schema` id and a `tool` block. `strata-diff/1`,
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
- `source`: where the record comes from.
  - `ref`: the ref as typed, a name or a uid.
  - `uid`, `projectUid` (empty for tenant scope), `name`, `version`,
    `cloudType`, `type`, and `tags`.
  - `template`: `published`, `draft`, `applied` for a cluster, or `candidate`.
  - `consumers`: the uids of the clusters that used the version, and the count.
  - `fetchedAt`: the time of the read.
  - `pack`, for a pack record: the pack's `name` and `version`, the
    `registryUid` and `registryName` it came from, and `packUid`, the uid of
    that version in the registry. `template` is `registry` and `type` is
    `pack`.
  - `cluster`, for a cluster record: its `uid` and `name`, the `profile` that
    `cluster:<cluster>/<profile>` selected, the applied `profiles`, and
    `variables`, the value that the cluster gives each variable. `verify` reads
    only the uids, so a rename does not stop it. `variables` is not part of the
    root. A sensitive variable has `sensitive: true`, and no value or hash.
- `variables` and `variablesHash`: the profile variables.
- `packs[]`: one entry for each pack, in layer order.
  - The identity fields: `key` (`<layer>/<name>`), `name`, `layer`, `type`,
    `tag`, `version`, `packUid`, and `registryUid`.
  - The two digests, the raw values, the canonical values, and the values hash.
  - The manifests, with their content and content hash.
  - `hash`: `identity`, `upstream`, `upstreamRaw`, `upstreamSource`,
    `authored`, and `node`. `upstreamSource` is the field that gave `UPSTREAM`:
    `packRef.digest`, `packSummary.digest`, or `none`.

A cluster record that covers two or more profiles joins their names and versions
with `+`, for example `aws-stack+platform-addons@1.1.0+1.0.0`, and its `source.uid`
is empty. A record of one profile keeps the name, version, and uid of that
profile.

## strata-diff/1

A comparison of two sides, `a` and `b`.

- `legend`: the flags, each `status`, each `op`, and each impact level, in
  words. `generatedAt` is the time of the comparison.
- `a` and `b`: the two sources, with uid, version, consumers, and fetch time.
- `same`: the roots are equal. `rootA` and `rootB` are the roots.
- `sameContent`: the roots are equal, or differ only by the pack uids of
  manifest packs and the tag names of the same builds. `same` stays `false` in
  the second case.
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
  - `manifests[]`: the manifest changes, with their content hashes.
- `verify`, only after `verify`: `status`, `uid`, `recordedAt`, and `checkedAt`.
  - `overwrittenBy`: for a profile record, the `name@version` that the uid names
    now. The status is `drifted`, also when the roots are equal.
  - `clusterValuesChanged`: for a cluster record, the number of variable values
    that differ from the record.
  - `profilesOverwritten`: for a cluster record, each profile that the cluster
    runs whose uid names another version now, or no longer exists: `uid`,
    `name`, `clusterVersion`, and `profileVersion` (empty for a uid that is
    gone). The status is `drifted`.

Each changed value is a leaf with `op`, `path`, `segments`, `old`, and `new`. A
leaf is a scalar or an empty container. The path joins keys with a dot. A list
item is `[i]`. When each item of a list is an object with a unique `name`, the
item is `[name=<value>]`, so an inserted item shows as one addition. `segments`
is the path as a list: strings for keys, numbers for indexes, and
`{"name": ...}` for named items.

## strata-verify-summary/1

- `legend.status`: `unchanged`, `drifted`, and `error`.
- `summary`: `records`, `unchanged`, `drifted`, `errors`, and `status`, which is
  the exit status in words.
- `records[]`: one entry for each file, in the order given: `path`, `status`,
  `name`, `version`, `uid`, `projectUid`, `recordedAt`, `rootRecorded`, and
  `rootLive`.
  - A drifted record adds `maxImpact`, `packsChanged`, and the full `diff`.
  - A cluster record with changed variable values adds `clusterVariables`,
    whatever its status.
  - A failed record adds `error`. See [Errors](./errors.md).

## strata-chain/1

- `legend`, and `policy`: `exact` or `registry-move`.
- `same`: each pair passed.
- `summary`: `pairs`, `passed`, and `failed`.
- `records[]`: `label`, `name`, `version`, `uid`, `projectUid`, and `root`.
- `pairs[]`: `a`, `b`, `status` (`identical`, `same-content`, `registry-move`, or
  `different`), `rootA`, `rootB`, `packsRepinned`, `packsRetagged`, and
  `reasons[]`.

## strata-consumers/1

- `profile`: `uid`, `name`, and `version`.
- `count`: the number of consumers. Exit `0` when it is `0`.
- `overwritten`: the number of consumers whose copy names another version.
- `consumers[]`: `uid`, `name`, `projectUid`, `state`, `version`, and `error`
  when strata cannot read the cluster. The names and the projects come from
  `status.inUseClusters` of the profile.

## strata-list/1

- `projectUid` and `projectName`: the scope.
- `pack`: the `--pack` text, when given.
- `cluster`: the `--cluster` text, when given.
- `profiles[]`, `clusters[]`, and `projects[]`: only the kinds that were listed.
  - A profile: `ref`, `name`, `version`, `uid`, and `cloudType`. With `--packs`,
    also `type`, `consumers`, and `packs[]` with `layer`, `name`, `tag`, and
    `version`, or `error`.
  - A cluster: `ref`, `name`, `uid`, `state`, `cloudType`, and `projectUid`.
    With `--applied`, also `profiles[]` with the `uid`, `name`, `version`, and
    `type` of each profile the cluster applies, or `error`.
  - A project: `name` and `uid`.

## strata-packs/1

- `name`: the pack that was asked for, and `cloud` when `--cloud` was given.
- `registries[]`: one entry per registry that has the pack, with `registry`,
  `registryUid`, `layer`, and `versions[]`.
  - A version: `version`, `packUid`, `digest`, `type`, `cloudTypes[]`, `tags[]`
    with the exact tag first, and `latest`, which is true for the newest version
    of that registry.

