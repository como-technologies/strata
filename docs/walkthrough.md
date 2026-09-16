# How a team uses strata

This page shows one platform team and one profile through a release cycle.
Each command runs offline against the files in `examples/`. Build the tool
first with `cargo build --release`. When you connect to Palette, replace the
file paths with live refs: a profile uid or `<name>@<version>`.

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
   project before every live command. One project, one uid.
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
and sets `aws-pxk` to 1.1.0. It merges, and 1.1.0 is published to `dev`.

Who: the job that published it. It compares the previous record to the new
version and posts the result on the merged pull request. `gh` is this team's
code host CLI. Any host that renders markdown works.

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
version. `UPSTREAM` means the digest of the pack changed. `AUTHORED` means
the values or manifests on the profile changed. Here the new pack versions
bring new default values. The collapsible block under the table lists each
changed value, for example `kubeadmconfig.dns.imageTag: "v1.13.1" ->
"v1.14.2"`.

The impact column is the sentence the approver needs. Two layers say
`node repave`. This release replaces each node.

Then record the new version, as on day 1:

```
PALETTE_PROJECT_UID=$DEV_PROJECT strata snapshot aws-pxk@1.1.0 -o releases/1.1.0/dev.json
```

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

How to read it. Each infra layer is `unchanged`. One add-on has `AUTHORED`
only: the same pack with a new manifest. The impact is `in-place`, so no
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
strata compare releases/1.1.1/dev.json releases/1.1.1/prod.json || exit 1
```

Expected: `IDENTICAL`, exit `0`. The same release went to each project. Any
other result stops the promotion. Prod must run the release that dev
approved.

One difference can be intended. Some teams promote packs between registries,
for example from a staging registry to a production one, and re-pin the
profile. Then the comparison shows `IDENTITY UPSTREAM` on each pack, with no
`AUTHORED` and no value lines: the same configuration and a different
registry. An `AUTHORED` flag in that comparison means a value changed during
the promotion. Find the cause before you continue.

## Day 5: a cluster moves to the new version

Trigger: a cluster in `prod` runs `aws-pxk` 1.1.0. The team proposes 1.1.1.

Who: whoever prepares the change adds the comparison of the two records to
the review.

```
strata compare releases/1.1.0/prod.json releases/1.1.1/prod.json --format markdown
```

The cluster reviewer reads the impact column first. `in-place` means
approve. `repave` means schedule a maintenance window first.

Not available yet: a comparison of the record against what the cluster runs
now. The applied profile templates of a cluster use the same pack shape. They
are the planned next input.

## Every night: prove that published versions did not change

Trigger: a scheduled job.

Who: the job, over every record in the repository.

```
for f in releases/*/*.json; do
  project=$(basename "$f" .json)
  PALETTE_PROJECT_UID=$(project_uid_for "$project") strata verify "$f" --quiet || echo "DRIFT in $f"
done
```

`project_uid_for` is the team's own lookup from a project name to its uid.

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

What to do on `DRIFT`: open an issue with the report. Then restore the rule.
The fix is a new version that carries the change through review and
publication. Do not repair the edited object in place. The record is the
truth. The live object is the deviation.

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

Delete only when `count` is `0`. Each listed cluster must move to a newer
version first. The nightly `verify` run refreshes this list. The latest
record is never more than one day old.

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
| Record this release                                   | `strata snapshot <name>@<version> -o releases/<version>/<project>.json` | written           |                                  |
| Did dev and prod get the same release?                | `strata compare releases/V/dev.json releases/V/prod.json`         | yes                     | no, stop the promotion           |
| Was a published version edited?                       | `strata verify releases/V/<project>.json`                         | unchanged               | drifted, open an issue           |
| Can we delete this version?                           | `strata snapshot <ref> \| jq .source.consumers.count`             | `0` means yes           |                                  |
| What does the JSON mean?                              | `strata schema diff`                                              | prints the JSON Schema  |                                  |

## What the tool does not do yet

- Compare a profile record against what a cluster is running now.
- Snapshot a candidate from local files before it is published. Today the
  comparison runs after the version is published.
- Read the `UPSTREAM` digest from the pack's own registry. Today it comes
  from the digest Palette stamps on the pack ref, which the team has not yet
  confirmed against a live profile.
