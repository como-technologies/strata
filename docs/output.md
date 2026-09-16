# Output formats

Strata has three output formats. `--format terminal` is the default.
`--format markdown` is for pull request comments. `--format json` is for
scripts and AI clients. `--json` is short for `--format json`.

## The terminal report

```
lab-profile@1.2.0 (64f0000000000000000000a1)  ->  lab-profile@1.3.0 (64f0000000000000000000b2)
root       73c75fb7dba6 -> d5c5d079f15f   CHANGED
consumers  0 -> 2 clusters

  variables CHANGED
        ~ cluster_env.defaultValue: "dev" -> "staging"
        + variable db_password (required, sensitive)

  [os   ] ubuntu-aws                 22.04              unchanged
~ [k8s  ] kubernetes                 1.29.4 -> 1.30.2   IDENTITY UPSTREAM AUTHORED
      upstream digest  sha256:k8s12 -> sha256:k8s13   (packRef.digest)
      values: 3 changed, 1 added, 0 removed
        ~ kubeadmconfig.apiServer.extraArgs.audit-log-maxage: "30" -> "90"
        + kubeadmconfig.apiServer.extraArgs.profiling: "false"
      impact: node repave (Kubernetes version change)
~ [cni  ] cni-cilium-oss             1.15.3             UPSTREAM
      upstream digest  sha256:cni-a -> sha256:cni-b   (packRef.digest)
      impact: pack content changed under the same tag, applied on next profile sync
~ [addon] prometheus-operator        55.8.3             AUTHORED
      ~ manifest extra-rules
      + manifest dashboards
      impact: in-place addon upgrade
+ [addon] cert-manager               1.14.0             ADDED
- [addon] external-dns               0.14.0             REMOVED
```

The first line names the two sides and their uids. A side taken from a
cluster reads `name@version on cluster <name>` and shows the cluster uid. The second line shows
the two roots and `IDENTICAL` or `CHANGED`. The third line shows the consumer
counts. Then come the variables, in name order: one line for an added or
removed variable with its traits (`required`, `sensitive`, `hidden`,
`immutable`), and one line per changed field of a changed variable. Then comes
one line per pack, in layer order.
A changed pack shows its flags. Below it, indented, come the upstream digest,
the changed values, the manifest changes, and the impact.

`--full` adds a unified diff of the raw values and of each changed manifest.

### Colors

When stdout is a terminal, the report is colored. Repave impacts are red.
Cluster-wide is yellow. In-place is cyan. Added is green. Removed is red.
Changed is yellow. Old values are red and new values are green. Unchanged
packs are dim. Output to a pipe or a file is plain.

`--color=always` forces color. `--color=never` or `--no-color` removes it.
Strata obeys the `NO_COLOR` and `CLICOLOR_FORCE` environment variables. With
`--full`, the unified diffs use the same colors.

## The markdown comment

`--format markdown` writes a pull request comment. It has:

- a header table with the two uids, the two roots, the consumer counts, and
  the fetch times
- a result line with `IDENTICAL` or `CHANGED`, the maximum impact, the pack
  counts, and the variables state
- one table row per pack with layer, name, version, flags, and impact
- one collapsible `diff` block for the variables, with the same lines as
  the terminal report
- one collapsible `diff` block per changed pack, with the changed values,
  the upstream digest, and the manifest changes

`--full` appends the unified diffs in `diff` code blocks.

## The JSON document

`--format json` writes the document `strata-diff/1`. A program or a language
model can read it without these pages:

- `schema`, `tool`, and `generatedAt` identify the document.
- `legend` explains the three flags, each `status`, each `op`, and each
  impact level in plain sentences.
- `summary` gives `packsAdded`, `packsRemoved`, `packsChanged`,
  `packsUnchanged`, `identityChanges`, `upstreamChanges`, `authoredChanges`,
  `variablesChanged`, and `maxImpact`.
- `a` and `b` are the two snapshot sources, with uid, version, consumer
  count, and fetch time.
- `variables.diff[]` lists leaf changes with the path `<variable>.<field>`.
  `variables.byName` groups them: `added[]` and `removed[]` carry `name`,
  `required`, `sensitive`, `hidden`, and `immutable`; `changed[]` carries the
  `name` and the changed `fields[]` as leaves with paths relative to the
  variable, such as `defaultValue`.
- `packs[]` has one entry per pack key on either side, sorted by layer and
  then by name. Each entry has `status`, the booleans `identity`,
  `upstream`, and `authored`, `paramsChanged`, and `impact` with `level` and
  `text`. The sides `a` and `b` hold tag, version, packUid, registryUid, the
  upstream digest and its source, the values hash, and the node hash.
  `values` holds `changed`, `parseable`, the three counts, and `diff[]`.
  `manifests[]` lists changes with the content hashes.
- Each leaf has `op`, `path`, `segments`, `old`, and `new`. `segments` is
  the path as a list: strings for keys, integers for indexes, and
  `{"name": ...}` for named list items.
- `verify` is present after `strata verify`. It holds `status`, `uid`,
  `recordedAt`, and `checkedAt`.

The JSON is never colored and never truncated. `--compact` writes it on one
line. `strata schema diff` prints the JSON Schema of the document.

## The verify summary

`strata verify` with several files, or with a quoted pattern such as
`'releases/**/*.json'`, checks each record in the project it was taken in and
prints one line per record and one line of counts:

```
OK     releases/1.1.1/dev.json   aws-pxk@1.1.1  unchanged since 2026-09-16T01:56:23Z
DRIFT  releases/1.1.1/prod.json  aws-pxk@1.1.1  differs from the copy recorded 2026-09-16T02:10:41Z; max impact in-place, 1 packs changed
ERROR  releases/1.0.0/dev.json   aws-pxk@1.0.0  GET /v1/clusterprofiles/64e0000000000000000000a1 failed with HTTP 404: {"code":"ResourceNotFound"}
3 records: 1 unchanged, 1 drifted, 1 errors
```

A record that cannot be read or checked is an `ERROR` line; the other records
are still checked. The exit status is `0` when every record is unchanged, `1`
when any drifted, and `2` when any could not be checked. `--project` checks
every record in one project instead. `--format markdown` writes the same
lines as a table.

`--format json` writes the document `strata-verify-summary/1`:

- `legend.status` explains `unchanged`, `drifted`, and `error`.
- `summary` gives `records`, `unchanged`, `drifted`, `errors`, and `status`,
  which is the exit status in words.
- `records[]` has one entry per file in the order given: `path`, `status`,
  `name`, `version`, `uid`, `projectUid`, `recordedAt`, `rootRecorded`, and
  `rootLive`. A drifted record adds `maxImpact`, `packsChanged`, and the full
  `diff` document. A failed record adds the `error` object described below.

`strata schema verify-summary` prints its JSON Schema. One file on its own
prints the full report and verdict line described above.

## The same-release report

`strata same-release A B C` compares each record to the next, in the order
given, and says whether they are one release. Exit `0` means every pair
passed, `1` means at least one did not.

```
IDENTICAL      releases/1.1.1/dev.json -> releases/1.1.1/test.json   root 088136c929bd
REGISTRY MOVE  releases/1.1.1/test.json -> releases/1.1.1/prod.json   5 packs re-pinned, same configuration
SAME RELEASE: 2 pairs passed
```

```
DIFFERENT      releases/1.1.1/test.json -> releases/1.1.1/prod.json   root 088136c929bd -> 3f1c0a9e7b21
               pack addon/cert-manager AUTHORED: charts.cert-manager.replicaCount
NOT THE SAME RELEASE: 1 of 1 pairs failed
```

By default only equal roots pass. `--allow registry-move` also passes a pair
whose only difference is packs re-pinned from another registry: the same
tag, version, type, values, manifests, and params, with variables unchanged.
Without the flag such a pair is `DIFFERENT` and its reasons name the move,
so the reader can decide to allow it. A reason names the pack and, for an
`AUTHORED` change, the first changed leaf.

`--format json` writes the document `strata-chain/1`: `legend`, `policy`,
`same`, `summary` with `pairs`, `passed`, and `failed`, `records[]` with
each label, name, version, uid, project, and root, and `pairs[]` with `a`,
`b`, `status` (`identical`, `registry-move`, `different`), the two roots,
`packsRepinned`, and `reasons[]`. `strata schema chain` prints its JSON
Schema.

## The consumers report

`strata consumers <ref>` lists the clusters that use one profile version. The
ref is a profile uid, `<name>@<version>`, or a profile record; a record's uid
and project are used, and the list is always fetched live. A cluster ref is
an error.

```
aws-pxk@1.0.0 (64e0000000000000000000a1): 2 consumers
  workload-a  64f1000000000000000000e1  project 64f20000000000000000a0f1  Running
  workload-b  64f1000000000000000000e2  project -  unknown  (GET /v1/spectroclusters/64f1000000000000000000e2 failed with HTTP 404: ...)
```

Names and projects come from the profile's `status.inUseClusters`. Each
cluster is then read in its own project for `status.state`. A cluster that
cannot be read shows `unknown` and the error, and it still counts. The exit
status is `0` when no cluster uses the version and `1` when any does.
`--format markdown` writes a table. `--format json` writes the document
`strata-consumers/1`: `profile` with uid, name, and version, `count`, and
`consumers[]` with `uid`, `name`, `projectUid`, `state`, and `error` when
there is one. `strata schema consumers` prints its JSON Schema.

### Errors in JSON mode

When `--format json` or `--json` is selected and the command fails, stderr
carries one line and the exit status is `2`, as always:

```
{"error":{"kind":"not-snapshot","message":"'profile.json' is not a strata snapshot","path":"profile.json"}}
```

`kind` is a stable kebab-case name: `missing-credential`, `api`,
`invalid-setting`, `pattern`, `transport`, `not-json`, `profile-not-found`,
`cluster-profile-not-found`, `profile-ref-required`, `candidate-invalid`,
`pack-not-found`, `registry-not-found`, `not-snapshot`,
`snapshot-schema`, `no-uid`,
`io`, `empty-file`, `file-not-json`, or `serialize`. `message` is the
sentence the terminal shows. The other fields are the variant's own:

- `variable` and `alternate` for `missing-credential`
- `path` and `status` for `api`
- `variable` and `value` for `invalid-setting`
- `pattern` for `pattern`
- `path` for `transport`, `not-json`, `not-snapshot`, `io`, `empty-file`,
  and `file-not-json`
- `name` and `version` for `profile-not-found`
- `cluster`, `profile`, and `applied` for `cluster-profile-not-found`
- `reference` for `profile-ref-required`
- `path` for `candidate-invalid`
- `name`, `version`, `registry`, and `tag` when one was given, for
  `pack-not-found`
- `registry` and `available` for `registry-not-found`
- `path` and `schema` for `snapshot-schema`

Without JSON selected, stderr is the sentence prefixed with `strata:`.

### Policy checks with jq

```
strata compare A B --format json > report.json
jq -r .summary.maxImpact report.json
jq -e '.summary.maxImpact != "repave"' report.json || echo "needs a second approver"
jq -r '.packs[] | select(.impact.level == "repave") | "\(.name): \(.impact.text)"' report.json
```
