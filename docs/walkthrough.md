# How a team uses strata

This page shows one platform team and one profile through a release cycle.
Each command runs offline against the files in `examples/`. Build the tool
first with `cargo build --release`. When you connect to Palette, replace the
file paths with live refs: a profile uid, `<name>@<version>`, `<name>@latest`
for the highest version in the scope, or `cluster:<uid>` for what a cluster
runs. `strata list` prints the profile and cluster refs in the scope, and
`strata list --pack <name>` the profile versions that carry a pack.

The cast. The team in this page publishes its profile with Terraform. Strata
does not know that and does not need to. The same commands work when a team
publishes through the API or the Palette UI, and whatever runs them: a CI
job or an engineer at a terminal. The team runs two Palette projects:

| thing        | name in this page                                                          |
|--------------|----------------------------------------------------------------------------|
| profile      | `aws-pxk`, the example profile built from real packs in `examples/`        |
| projects     | `dev` and `prod`, one Palette project each                                 |
| records      | `releases/<version>/<project>.json`, committed next to the profile definition |
| the tool     | `strata`, run from `target/release/strata`                                 |

The names are the team's choice. Strata has no opinion on repository layout,
project names, or file names. It reads refs and JSON files and returns an
exit code.

```
alias strata=target/release/strata
```

## Conventions the team agrees on once

1. **Where snapshots live.** Next to the profile definition, at
   `releases/<version>/<project>.json`, committed with the release. The
   `examples/profiles/` files stand in for these below.
2. **Which project a command talks to.** Set `PALETTE_PROJECT_UID` to the
   project before every live command. One project, one uid. The snapshot
   records it, so `verify` later finds the project on its own.
   `strata list projects` prints the uid of each project the key can see.
3. **Exit codes drive automation.** `0` means identical, or unchanged for
   `verify`. `1` means different, or drifted. `2` means the tool could not
   answer. Treat `2` as a failed job, never as "no change".
4. **A published version is never edited.** If a change is necessary, bump
   the version. `verify` tells the team when this rule is broken.

## Day 1: record the first release

Trigger: `aws-pxk` 1.0.0 is published to project `dev`.

Who: whoever published it, right after. Strata cannot tell a CI job from a
person at a terminal.

```
PALETTE_PROJECT_UID=$DEV_PROJECT strata snapshot aws-pxk@1.0.0 -o releases/1.0.0/dev.json
```

What you see on stderr:

```
strata: wrote releases/1.0.0/dev.json (root aee66c68d829)
```

What it is: the record of the release. The file holds the Merkle root and
one node hash per pack. It holds the canonical values, the raw values, and
the manifests. It lists the clusters that used the version at that moment.
Commit the file.

Do the same each time a version is published to another project:
`releases/1.0.0/prod.json`, and later `releases/1.1.0/prod.json`.

## Day 2: a pack bump arrives

Trigger: four infra packs have new versions. A pull request bumps the pins
in the team's candidate file for `aws-pxk` and sets the version to 1.1.0.
Nothing is published yet.

The candidate file names each layer's pack and version. A layer without
values gets the pack's defaults. `docs/model.md` describes the format.

```yaml
name: aws-pxk
version: 1.1.0
cloudType: aws
registry: Public Repo
layers:
  - name: ubuntu-aws
    version: "24.04"
  - name: kubernetes
    version: 1.36.2
  - name: cni-cilium-oss
    version: 1.20.1
  - name: csi-aws-ebs
    version: 1.65.0
  - name: cert-manager
    version: 1.21.1
```

Who: the pull request job, before anyone approves. It builds the candidate's
snapshot, compares it with the last record, and posts one comment with a
`repave` label when the impact calls for it.

```
strata snapshot --from-candidate aws-pxk.yaml -o candidate.json
scripts/pr-comment.sh releases/1.0.0/dev.json candidate.json
```

Offline: `strata compare examples/profiles/aws-pxk@1.0.0.json examples/profiles/aws-pxk@1.1.0.json --format markdown`

A candidate snapshot needs the registry lookups, so the offline command
compares the published example instead. It has the root the candidate
produces.

What the reviewer sees at the top of the comment:

```
**Result:** CHANGED · max impact: `repave` · packs: 4 changed, 0 added, 0 removed, 1 unchanged · variables: unchanged

| | layer | pack | version | change | impact |
|---|---|---|---|---|---|
| `~` | os | `ubuntu-aws` | 20.04 → 24.04 | IDENTITY UPSTREAM | node repave (OS image change) |
| `~` | k8s | `kubernetes` | 1.35.3 → 1.36.2 | IDENTITY UPSTREAM AUTHORED | node repave (Kubernetes version change) |
| `~` | cni | `cni-cilium-oss` | 1.19.6 → 1.20.1 | IDENTITY UPSTREAM AUTHORED | cluster-wide infra addon update |
| `~` | csi | `csi-aws-ebs` | 1.63.1 → 1.65.0 | IDENTITY UPSTREAM AUTHORED | cluster-wide infra addon update |
| ` ` | addon | `cert-manager` | 1.21.1 | unchanged |  |
```

How to read one row. `IDENTITY` means the layer points at a different pack
version. `UPSTREAM` means the digest of the pack changed. `AUTHORED` means
the values or manifests on the profile changed. Here the new pack versions
bring new default values. The collapsible block under the table lists each
changed value, for example `kubeadmconfig.dns.imageTag: "v1.13.1" ->
"v1.14.2"`.

The impact column is the sentence the approver needs. Two layers say
`node repave`. This release replaces each node.

After approval the pull request merges and 1.1.0 is published to `dev`.
Record it, as on day 1, and prove that what was published is what was
approved, with the candidate rebuilt from the merged file:

```
PALETTE_PROJECT_UID=$DEV_PROJECT strata snapshot aws-pxk@1.1.0 -o releases/1.1.0/dev.json
strata snapshot --from-candidate aws-pxk.yaml -o candidate.json
strata same-release candidate.json releases/1.1.0/dev.json
```

`SAME RELEASE` means the published version is the one the reviewer saw.

### Let a script enforce a rule from the JSON

The same comparison as JSON is for scripts and for AI review. The `summary`
block answers the policy question in one field.

```
strata compare releases/1.0.0/dev.json releases/1.1.0/dev.json --format json > report.json
jq -r .summary.maxImpact report.json          # repave
jq -e '.summary.maxImpact != "repave"' report.json || gh pr edit "$PR" --add-label repave
```

Rule: a `repave` label requires a second approver for `prod`. A script sets
the label. A reviewer does not need to read the values to know.

To list only the layers that cause the repave:

```
jq -r '.packs[] | select(.impact.level == "repave") | "\(.name): \(.impact.text)"' report.json
```

```
ubuntu-aws: node repave (OS image change)
kubernetes: node repave (Kubernetes version change)
```

## Day 3: a configuration edit

Trigger: an engineer attaches a manifest to `cert-manager` and adds a profile
variable. No pack version changes. The version becomes 1.1.1.

Offline: `strata compare examples/profiles/aws-pxk@1.1.0.json examples/profiles/aws-pxk@1.1.1.json`

```
aws-pxk@1.1.0 (uid 64e0000000000000000000b2)  ->  aws-pxk@1.1.1 (uid 64e0000000000000000000c3)
root       8a8a64d3f95d -> 088136c929bd   CHANGED
consumers  0 -> 0 clusters

  variables CHANGED
        + variable platform_owner (required)

  [os   ] ubuntu-aws                 24.04              unchanged
  [k8s  ] kubernetes                 1.36.2             unchanged
  [cni  ] cni-cilium-oss             1.20.1             unchanged
  [csi  ] csi-aws-ebs                1.65.0             unchanged
~ [addon] cert-manager               1.21.1             AUTHORED
      + manifest namespace-labels
      impact: in-place addon upgrade
```

How to read it. The new variable is one line with its traits; a changed
variable would show only the fields that changed. Each infra layer is
`unchanged`. One add-on has `AUTHORED` only: the same pack with a new
manifest. The impact is `in-place`, so no
node is replaced. `maxImpact` in the JSON is `in-place`, so the script does
not add the `repave` label. This is a small, safe release.

## Day 4: promote to prod

Trigger: the team promotes 1.1.1 to `prod`, behind its approval gate. After
the publication it records the project:

```
PALETTE_PROJECT_UID=$PROD_PROJECT strata snapshot aws-pxk@1.1.1 -o releases/1.1.1/prod.json
```

Promotion check, right after:

```
strata same-release releases/1.1.1/dev.json releases/1.1.1/prod.json
```

```
IDENTICAL      releases/1.1.1/dev.json -> releases/1.1.1/prod.json   root 088136c929bd
SAME RELEASE: 1 pairs passed
```

Expected: exit `0`. The same release went to each project. Any other result
stops the promotion. Prod must run the release that dev approved. With more
projects, list every record in promotion order; each is compared to the
next.

One difference can be intended. Some teams promote packs between registries,
for example from a staging registry to a production one, and re-pin the
profile. Then each pack shows `IDENTITY UPSTREAM` with no `AUTHORED`: the
same configuration and a different registry. Tell the check to accept
exactly that and nothing else:

```
strata same-release --allow registry-move releases/1.1.1/dev.json releases/1.1.1/prod.json
```

```
REGISTRY MOVE  releases/1.1.1/dev.json -> releases/1.1.1/prod.json   5 packs re-pinned, same configuration
SAME RELEASE: 1 pairs passed
```

A value that changed during the promotion still fails, and the report names
the pack and the leaf:

```
DIFFERENT      releases/1.1.1/dev.json -> releases/1.1.1/prod.json   root 088136c929bd -> 3f1c0a9e7b21
               pack addon/cert-manager AUTHORED: charts.cert-manager.replicaCount
NOT THE SAME RELEASE: 1 of 1 pairs failed
```

Find the cause before you continue.

## Day 5: a cluster moves to the new version

Trigger: the cluster `workload-a` in `prod` should run `aws-pxk` 1.1.0. The
team proposes 1.1.1.

Who: whoever prepares the change.

First, check that the cluster runs the release its record describes. A
cluster can stay on an older version, or someone can edit its pack values.

```
PALETTE_PROJECT_UID=$PROD_PROJECT strata compare releases/1.1.0/prod.json cluster:<uid>/aws-pxk
```

Offline: `strata compare examples/profiles/aws-pxk@1.1.0.json examples/clusters/workload-a.json`

```
aws-pxk@1.1.0 (uid 64e0000000000000000000b2)  ->  aws-pxk@1.1.0 on cluster workload-a (uid 64e0000000000000000000d1)
root       8a8a64d3f95d -> 8a8a64d3f95d   IDENTICAL
```

`IDENTICAL`, exit `0`: the cluster runs exactly the release. Any other result
means the run moved away from the release. Find out why before you move it
again. `/aws-pxk` selects that profile when the cluster runs several.

Then add the comparison of the two records to the review:

```
strata compare releases/1.1.0/prod.json releases/1.1.1/prod.json --format markdown
```

The cluster reviewer reads the impact column first. `in-place` means
approve. `repave` means schedule a maintenance window first.

To run the cluster commands against a real cluster, `examples/live/` builds
one that runs a recorded version, next to a newer version. Its
`compare-release-cluster` and `compare-next-cluster` checks are the two
comparisons in this section.

The same check against the proposed record shows what the move changes on
this cluster. The `-` lines are what the cluster does not have yet:

Offline: `strata compare examples/profiles/aws-pxk@1.1.1.json examples/clusters/workload-a.json`

```
aws-pxk@1.1.1 (uid 64e0000000000000000000c3)  ->  aws-pxk@1.1.0 on cluster workload-a (uid 64e0000000000000000000d1)
root       088136c929bd -> 8a8a64d3f95d   CHANGED
consumers  0 -> 0 clusters

  variables CHANGED
        - variable platform_owner (required)

  [os   ] ubuntu-aws                 24.04              unchanged
  [k8s  ] kubernetes                 1.36.2             unchanged
  [cni  ] cni-cilium-oss             1.20.1             unchanged
  [csi  ] csi-aws-ebs                1.65.0             unchanged
~ [addon] cert-manager               1.21.1             AUTHORED
      - manifest namespace-labels
      impact: in-place addon upgrade
```

After the move, that command prints `IDENTICAL`. To keep a record of what the
cluster runs, snapshot it like a profile:

```
PALETTE_PROJECT_UID=$PROD_PROJECT strata snapshot cluster:<uid>/aws-pxk -o runs/workload-a.json
```

`strata verify runs/workload-a.json` then fetches the cluster again and
reports `DRIFT` when its run no longer matches the record.

## Every night: prove that published versions did not change

Trigger: a scheduled job.

Who: the job, over every record in the repository.

```
strata verify 'releases/**/*.json' --format json > verify.json || echo "not clean"
```

What `verify` does: for each record it reads the uid and the project from
the file, fetches that profile again in that project, and compares roots.
The job needs one API key and nothing else. Exit `0` means every record is
unchanged, `1` means at least one drifted, `2` means at least one could not
be checked.

What you see on the terminal, without `--format json`:

```
OK     releases/1.1.1/dev.json   aws-pxk@1.1.1  unchanged since <recorded time>
DRIFT  releases/1.1.1/prod.json  aws-pxk@1.1.1  differs from the copy recorded <recorded time>; max impact in-place, 1 packs changed
2 records: 1 unchanged, 1 drifted, 0 errors
```

The JSON document carries the full comparison for each drifted record. To
read one drifted record as a report, run `verify` on that file alone:

```
strata verify releases/1.1.1/prod.json
```

```
~ [addon] cert-manager               1.21.1             AUTHORED
      values: 1 changed, 0 added, 0 removed
        ~ charts.cert-manager.replicaCount: 1 -> 2
      impact: in-place addon upgrade
DRIFT: published version aws-pxk@1.1.1 differs from the copy recorded <recorded time>
```

What to do on `DRIFT`: open an issue with the report. Then restore the rule.
The fix is a new version that carries the change through review and
publication. Do not repair the edited object in place. The record is the
truth. The live object is the deviation.

## Before end of life: the consumer check

Trigger: the team wants to delete `aws-pxk` 1.0.0 from a project.

```
PALETTE_PROJECT_UID=$PROD_PROJECT strata consumers aws-pxk@1.0.0 && delete_it
```

`consumers` asks Palette which clusters use the version now, then reads each
cluster in its own project for its state. It needs a live tenant. What you
see when two clusters still use it, and one cannot be read with this key:

```
aws-pxk@1.0.0 (uid 64e0000000000000000000a1): 2 consumers
  workload-a  64f1000000000000000000e1  project 64f20000000000000000a0f1  Running
  workload-b  64f1000000000000000000e2  project 64f20000000000000000a0f1  unknown  (GET /v1/spectroclusters/64f1000000000000000000e2 failed with HTTP 403: ...)
```

Exit `0` means no cluster uses the version, so the `&&` lets the delete run.
Exit `1` means at least one does; each must move to a newer version first. A
cluster that cannot be read still counts. `--format json` writes
`strata-consumers/1` for a script.

Offline, a record keeps the consumer uids from the moment it was taken:
`jq .source.consumers examples/profiles/aws-pxk@1.1.1.json`

To see `consumers` against a real cluster, `examples/live/` runs it on the
version a cluster runs and on one nothing runs: its `consumers-release` and
`consumers-next` checks.

## Why the nightly check is needed

Palette lets a published profile version be edited in place. The live example
(`examples/live/`) edited one value of a published version through the API:
the version kept its uid and its number, and no new version appeared. Only
`strata verify` noticed, with `DRIFT` and the changed leaf. The nightly job is
the guard.

## Cheat sheet

| question                                              | command                                                        | exit 0 means            | exit 1 means                     |
|-------------------------------------------------------|----------------------------------------------------------------|-------------------------|----------------------------------|
| Which refs can I use here?                            | `strata list`                                                     | listed                  |                                  |
| Which profile versions carry this pack?               | `strata list --pack <name>`                                       | listed                  |                                  |
| What changed between two versions?                    | `strata compare A B`                                              | identical               | different, read the report       |
| What does the approver need to see?                   | `strata compare A B --format markdown`                            | same                    | same                             |
| Does this release repave nodes?                       | `strata compare A B --json \| jq -r .summary.maxImpact`           | prints the level        |                                  |
| Record this release                                   | `strata snapshot <name>@<version> -o releases/<version>/<project>.json` | written           |                                  |
| Record the newest published version                   | `strata snapshot <name>@latest -o rel.json`                       | written; `source.version` says which |                       |
| Does this cluster run its release?                    | `strata compare releases/V/<project>.json cluster:<uid>/<profile>` | yes                   | no, read the report              |
| Did dev and prod get the same release?                | `strata same-release releases/V/dev.json releases/V/prod.json`    | yes                     | no, the reasons say why; stop the promotion |
| Same release, packs re-pinned from another registry?  | `strata same-release --allow registry-move A B`                   | yes                     | a value or a version moved too   |
| Was a published version edited?                       | `strata verify releases/V/<project>.json`                         | unchanged               | drifted, open an issue           |
| Were any published versions edited?                   | `strata verify 'releases/**/*.json'`                              | all unchanged           | some drifted; `2` means a record could not be checked |
| Can we delete this version?                           | `strata consumers <ref>`                                          | nothing uses it         | clusters still use it            |
| What does the JSON mean?                              | `strata schema diff`                                              | prints the JSON Schema  |                                  |

## What the tool does not do yet

- Accept project and cluster names; today both are uids (#18).
- Show the values a cluster gave its profile variables (#20).
- Write a candidate file from a published profile; the live example uses a
  jq filter for now (#21).
- Report drift from the nightly `verify` on its own; today the nightly job
  prints it (#25).
