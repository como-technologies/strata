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
        + db_password.name: "db_password"

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

The first line names the two profiles and their uids. The second line shows
the two roots and `IDENTICAL` or `CHANGED`. The third line shows the consumer
counts. Then come the variables. Then comes one line per pack, in layer order.
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

### Policy checks with jq

```
strata compare A B --format json > report.json
jq -r .summary.maxImpact report.json
jq -e '.summary.maxImpact != "repave"' report.json || echo "needs a second approver"
jq -r '.packs[] | select(.impact.level == "repave") | "\(.name): \(.impact.text)"' report.json
```
