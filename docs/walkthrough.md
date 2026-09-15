# How a team uses strata

This page shows one platform team and one profile through a release cycle.
Every command runs offline against the files in `examples/`. Build the tool
first with `cargo build --release`. When you connect to Palette, replace the
file paths with live refs: a profile uid or `<name>@<version>`.

The cast. The team keeps one repository per pack, one per profile, and one
per cluster, and runs four tiers that are each a Palette project:

| thing              | name in this page                          |
|--------------------|--------------------------------------------|
| profile repository | `profile-aws-pxk`                          |
| profile            | `aws-pxk`, an AWS infra profile            |
| tiers              | `dev`, `test`, `stage`, `prod`; each tier is one Palette project |
| cluster repository | `cluster-workload-a`, which pins one `aws-pxk` version per tier |
| the tool           | `strata`, run from `target/release/strata` |

```
alias strata=target/release/strata
```

## Conventions the team agrees once

1. **Where snapshots live.** In the profile repository, at
   `releases/<version>/<tier>.json`. The release workflow adds the file in the
   same pull request that appends `CHANGELOG.md`. The `examples/profiles/`
   files stand in for these below.
2. **Which project a command talks to.** Set `PALETTE_PROJECT_UID` to the
   tier's project before every live command. One tier, one project, one uid.
3. **Exit codes drive the pipeline.** `0` means identical, or unchanged for
   `verify`. `1` means different, or drifted. `2` means the tool could not
   answer; treat it as a failed job, never as "no change".
4. **A published version is never edited.** If a change is needed, bump the
   version. `verify` is how the team finds out when this rule is broken.

## Day 1: record the first release

Trigger: the `main` workflow applied `aws-pxk` 1.0.0 to project `dev`.

Who: the workflow, right after `terraform apply`.

```
PALETTE_PROJECT_UID=$DEV_PROJECT strata snapshot aws-pxk@1.0.0 -o releases/1.0.0/dev.json
```

What you see on stderr:

```
strata: wrote releases/1.0.0/dev.json (root aee66c68d829)
```

What it is: the record of the release. The file holds the Merkle root, one
node hash per pack, the canonical values, the raw values and manifests, and
the clusters that used the version at that moment. Commit it.

Do the same after each tier apply in the `release` workflow:
`releases/1.0.0/test.json`, `stage.json`, `prod.json`.

## Day 2: a pack bump arrives

Trigger: `pack-published` events bumped four infra layers. The bump pull
request sets `aws-pxk` to 1.1.0. Merge it. The `main` workflow applies 1.1.0
to `dev`.

Who: the workflow, right after the apply. It compares the previous record to
the new version and posts the result on the merged pull request.

```
PALETTE_PROJECT_UID=$DEV_PROJECT strata compare releases/1.0.0/dev.json aws-pxk@1.1.0 --format markdown > report.md
gh pr comment "$PR" --body-file report.md
```

Offline: `strata compare examples/profiles/aws-pxk@1.0.0.json examples/profiles/aws-pxk@1.1.0.json --format markdown`

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
version. `UPSTREAM` means the pack's digest changed. `AUTHORED` means the
values or manifests on the profile changed; here it is the new default values
that came with the new pack versions. The collapsible block under the table
lists every changed value, for example `kubeadmconfig.dns.imageTag: "v1.13.1"
-> "v1.14.2"`.

The impact column is the sentence the approver needs. Two layers say
`node repave`. This release replaces every node.

Then record the new version, as on day 1:

```
PALETTE_PROJECT_UID=$DEV_PROJECT strata snapshot aws-pxk@1.1.0 -o releases/1.1.0/dev.json
```

### Let the pipeline enforce a rule from the JSON

The same comparison as JSON is for scripts and for AI review. The `summary`
block answers the policy question in one field.

```
strata compare releases/1.0.0/dev.json releases/1.1.0/dev.json --format json > report.json
jq -r .summary.maxImpact report.json          # repave
jq -e '.summary.maxImpact != "repave"' report.json || gh pr edit "$PR" --add-label repave
```

Rule: a `repave` label requires a second approver on the `prod` environment.
The pipeline sets the label; humans do not have to read the values to know.

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
aws-pxk@1.1.0 (64e0000000000000000000b2)  ->  aws-pxk@1.1.1 (64e0000000000000000000c3)
root       8a8a64d3f95d -> 088136c929bd   CHANGED
consumers  0 -> 0 clusters

  variables CHANGED
        + platform_owner.displayName: "Platform owner"
        + platform_owner.name: "platform_owner"
        + platform_owner.required: true
        ...

  [os   ] ubuntu-aws                 24.04              unchanged
  [k8s  ] kubernetes                 1.36.2             unchanged
  [cni  ] cni-cilium-oss             1.20.1             unchanged
  [csi  ] csi-aws-ebs                1.65.0             unchanged
~ [addon] cert-manager               1.21.1             AUTHORED
      + manifest namespace-labels
      impact: in-place addon upgrade
```

How to read it. Every infra layer is `unchanged`. One add-on has `AUTHORED`
only: same pack, new manifest. The impact is `in-place`, so no node is
replaced. `maxImpact` in the JSON is `in-place`, so the pipeline does not add
the `repave` label. This is what a small, safe release looks like.

## Day 4: promote through the tiers

Trigger: the team tags 1.1.1. The `release` workflow applies it to `test`,
then `stage`, then `prod`, each behind its environment gate. After each apply
it records the tier:

```
PALETTE_PROJECT_UID=$TEST_PROJECT  strata snapshot aws-pxk@1.1.1 -o releases/1.1.1/test.json
PALETTE_PROJECT_UID=$STAGE_PROJECT strata snapshot aws-pxk@1.1.1 -o releases/1.1.1/stage.json
PALETTE_PROJECT_UID=$PROD_PROJECT  strata snapshot aws-pxk@1.1.1 -o releases/1.1.1/prod.json
```

Promotion check, in the workflow after the `prod` apply:

```
strata compare releases/1.1.1/test.json releases/1.1.1/prod.json || exit 1
```

Expected: `IDENTICAL`, exit `0`. The same release went to every tier. Any
other result stops the release, because prod would not be running what test
approved.

One difference is by design. The release build rewrites pack pins from the
`packs-dev` registry to `packs`, so `dev` versus `test` shows `IDENTITY
UPSTREAM` on every pack, with no `AUTHORED` and no value lines. That pattern
is the signature of a clean promotion: same configuration, different
registry. An `AUTHORED` flag in that comparison means a value changed on the
way to the release registry, and that is a bug to chase.

## Day 5: the cluster repository moves a tier

Trigger: `profile-published` for 1.1.1 opens a bump pull request in
`cluster-workload-a`. It changes `tiers/test.yaml` from `aws-pxk: 1.1.0` to
`aws-pxk: 1.1.1`.

Who: the bump workflow adds the comparison of the two profile records to the
pull request body.

```
strata compare releases/1.1.0/test.json releases/1.1.1/test.json --format markdown
```

The cluster reviewer reads the impact column and nothing else. `in-place`
means approve. `repave` means schedule a window first.

What is not there yet: comparing the record against what the cluster is
running now. A cluster's applied profile templates use the same pack shape and
are the planned next input.

## Every night: prove that published versions did not change

Trigger: a scheduled workflow.

Who: the workflow, over every record in the repository.

```
for f in releases/*/*.json; do
  tier=$(basename "$f" .json)
  PALETTE_PROJECT_UID=$(project_uid_for "$tier") strata verify "$f" --quiet || echo "DRIFT in $f"
done
```

What `verify` does: it reads the uid from the record, fetches that profile
again, and compares roots.

What you see when the rule held:

```
OK: published version aws-pxk@1.1.1 is unchanged since <recorded time>
```

What you see when someone edited the published version in the Palette UI:

```
~ [addon] cert-manager               1.21.1             AUTHORED
      values: 1 changed, 0 added, 0 removed
        ~ charts.cert-manager.replicaCount: 1 -> 2
      impact: in-place addon upgrade
DRIFT: published version aws-pxk@1.1.1 differs from the copy recorded <recorded time>
```

What to do on `DRIFT`: open an issue with the report, and restore the rule.
The fix is a new version that carries the change through the pipeline. The
edited object is not repaired in place, because the record is the truth and
the live object is the deviation.

## Before end of life: the consumer check

Trigger: the team wants to delete `aws-pxk` 1.0.0 from a project.

```
PALETTE_PROJECT_UID=$PROD_PROJECT strata snapshot aws-pxk@1.0.0 | jq .source.consumers
```

Offline: `jq .source.consumers examples/profiles/aws-pxk@1.1.1.json`

```
{
  "clusters": [],
  "count": 0
}
```

Delete only when `count` is `0`. Any listed cluster uid must move to a newer
version first. The nightly `verify` run refreshes this list, so the latest
record is never older than a day.

## Confirm how Palette treats an edit to a published version

One Palette behavior is still unconfirmed, and the nightly check depends on
it: what happens when someone edits a published profile version in place.
Confirm it in a sandbox tenant:

1. `strata snapshot <profile>@<version> -o before.json`
2. Edit one value of that version in the Palette UI and save.
3. `strata verify before.json`

Write the result down with the team's model notes. If `verify` exits `1`,
Palette allowed the edit and the nightly job is the guard. If Palette created
a new version instead, `verify` exits `0` and the new version shows up in the
listing.

## Cheat sheet

| question                                              | command                                                        | exit 0 means            | exit 1 means                     |
|-------------------------------------------------------|----------------------------------------------------------------|-------------------------|----------------------------------|
| What changed between two versions?                    | `strata compare A B`                                              | identical               | different, read the report       |
| What does the approver need to see?                   | `strata compare A B --format markdown`                            | same                    | same                             |
| Does this release repave nodes?                       | `strata compare A B --json \| jq -r .summary.maxImpact`           | prints the level        |                                  |
| Record this release                                   | `strata snapshot <name>@<version> -o releases/<version>/<tier>.json` | written              |                                  |
| Did test and prod get the same release?               | `strata compare releases/V/test.json releases/V/prod.json`        | yes                     | no, stop the release             |
| Was a published version edited?                       | `strata verify releases/V/<tier>.json`                            | unchanged               | drifted, open an issue           |
| Can we delete this version?                           | `strata snapshot <ref> \| jq .source.consumers.count`             | `0` means yes           |                                  |
| What does the JSON mean?                              | `strata schema diff`                                              | prints the JSON Schema  |                                  |

## What the tool does not do yet

- Compare a profile record against what a cluster is running now.
- Snapshot a candidate from the repository files before it is published.
  Today the comparison runs after the `dev` apply, and that report is the
  evidence for the `test`, `stage`, and `prod` gates.
- Read the `UPSTREAM` digest from the ECR registry. Today it comes from the
  digest Palette stamps on the pack ref, which the team has not yet confirmed
  against a live profile.
