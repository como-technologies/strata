# Preflight in a pipeline

Three jobs, one for each tier. Each one runs where its inputs are.

| Job | When | Command | Needs |
| --- | --- | --- | --- |
| static | each pull request that changes a cluster | `strata preflight cluster.json --cloud eks --tier static --records 'releases/**/*.json' --offline` | the records of the profiles in the repository |
| live | before each build | `strata preflight cluster.json --cloud eks --strict --out preflight` | `PALETTE_API_KEY`; the tools of the cloud for the cloud tier |
| watch | during the build | `strata preflight --watch cluster:$UID --limit 3600` | `PALETTE_API_KEY`; the uid, from the create |

Exit `0` lets the next step run. Exit `1` stops it: a counted check failed, or
the readiness is under `--ready`. Exit `2` is a failed job, never "no fault":
strata could not answer.

## The static tier on each pull request

The static tier needs no key and no network, so it runs on every pull request
in milliseconds. Give it the records of the profiles that the repository keeps,
so the rules on the profiles run too:

```bash
strata preflight clusters/payments.json --cloud eks --tier static \
  --records 'releases/**/*.json' --offline --format markdown > preflight.md
```

The markdown is one table for a pull request comment, with the reproduce lines
in a fold. `--strict` on a pull request stops the merge on a failed probe as
well; without it, a failed probe is a line to read. [On GitHub and
GitLab](#on-github-and-gitlab) shows how a job posts the table.

## The live tier before a build

```bash
strata preflight clusters/payments.json --cloud eks --strict --out preflight || exit 1
terraform apply
```

With a key, strata records each profile from Palette, reads the cloud account
and the packs, calls the validate routes, and runs the checks of the cloud tier
with the tools on the machine: `aws`, `gcloud`. A tool that is not there makes
its rules `NOT CHECKED`, with the command to run by hand. `--no-exec` runs no
tool at all.

`--out preflight` writes `report.md`, `report.json`, `report.html`,
`world.json`, and `entity.json`. Keep the directory as an artifact of the job:
the reproduce lines of the report read the two JSON files.

## The watch during the build

```bash
uid=$(curl -s -X POST ... /v1/spectroclusters/eks | jq -r .uid)
strata preflight --watch "cluster:$uid" --every 30 --limit 3600
```

The watch reads the cluster, its summary, and its events every 30 seconds. Each
4xx but 401 and 429 gives a reading with no summary or no events. A 401 or a 429
stops the watch with its error, as a read of the world does. It ends at the first
known stuck state with the event that shows it, or when the
cluster is `Running` with each counted rule passed, or at the limit; a warning
is in the report, and the watch reads on. The provider gives
exit `0` at its create timeout while the cluster still builds
([Client tools](../internals/palette/limits.md#what-strata-preflight-does-not-check));
the watch is the step that says `Running`.

## On GitHub and GitLab

`.github/workflows/preflight.yml` is a reusable workflow: the caller gives the
source, the cloud, the tiers, the records, and the release of strata, and the
job installs the two binaries, runs `scripts/ci/preflight.sh`, writes the
markdown to the job summary, and keeps the directory of `--out` as an artifact.
The script builds the options of `strata preflight`, and its exit status is the
status of strata.

| Input | Meaning |
| --- | --- |
| `source` | The body of the create as a file in the checkout, or `cluster:UID`. |
| `cloud`, `change`, `tier`, `records`, `probes`, `release`, `strict`, `ready` | The options of the command, by the same names. |
| `no-exec`, `verbose` | `--no-exec` and `--verbose`. |
| `rule` | `--rule`: the ids of the rules, separated by spaces. Strata runs only these rules. |
| `watch`, `every`, `limit` | `--watch`, `--every`, and `--limit`. Give `cluster:UID` as `source`. The job needs the key. |
| `out` | The directory for `report.md` and the other files. Default `preflight`. The artifact `preflight` holds it. |
| `palette-api`, `strata-version` | A self-hosted Palette, and the release of strata. |
| secrets `palette-api-key`, `palette-project-uid` | For the palette tier and for `cluster:UID`. Without a key the job runs the static tier alone. |
| secret `strata-token` | Reads this repository while it is private. |

The workflow sets `--format markdown`. It sets `--quiet` too, except when
`verbose` is `true`: `--quiet` wins over `--verbose`. The options `--rules`,
`--promotion`, `--ledger`, and `--schema` run no check of a cluster, so the
workflow has no input for them.

`examples/ci/github/strata.yml` holds a `preflight` job beside the other six,
and `examples/ci/gitlab/strata.yml` a `strata-preflight` job: both run the
static tier on each pull request, with the records of the repository. The job
that builds runs the live tier with the key, and the watch after the create.
The GitHub file also holds the job of the comment, in a comment of the file.

### The report as a comment

`preflight.yml` posts no comment, and it asks only for `contents: read`. A
second reusable workflow, `.github/workflows/preflight-comment.yml`, posts the
report. Call it in a second job, with `needs:` on the job of `preflight.yml`:

```yaml
permissions:
  contents: read
  pull-requests: write
jobs:
  preflight:
    uses: como-technologies/strata/.github/workflows/preflight.yml@v0.24.0
    with:
      source: clusters/payments.json
      strata-version: v0.24.0
  preflight-comment:
    needs: preflight
    if: ${{ !cancelled() && github.event_name == 'pull_request' }}
    uses: como-technologies/strata/.github/workflows/preflight-comment.yml@v0.24.0
    with:
      strata-version: v0.24.0
```

The second job takes `report.md` from the artifact `preflight` of the first
job. It posts one comment that starts with the marker
`<!-- strata-preflight -->`, and each push updates that comment. It is a comment
of its own, beside the comment of [`pr-comment.sh`](./pr-comments.md), which
starts with another marker. The condition `!cancelled()` runs the second job
also when the check failed, because a person must read a failed report.

Only `preflight-comment.yml` asks for `pull-requests: write`. The caller grants
that permission to the second job. A caller that wants no comment grants less,
and `preflight.yml` still runs. A run with no pull request, such as a build job
on the default branch, posts nothing.

| Input or secret | Meaning |
| --- | --- |
| `strata-version` | The release of strata. Give the tag that the first job uses. |
| secret `strata-token` | Reads this repository while it is private. |

The workflow runs `scripts/ci/pr-comment.sh --preflight preflight/report.md`. A
pipeline on another host runs the same command: see
[Post a preflight report](./pr-comments.md#post-a-preflight-report).

### The watch in the workflow

To watch a build, call the workflow after the create. Give the source
`cluster:<uid>`, the input `watch: true`, and the secret `palette-api-key`. The
inputs `every` and `limit` set the seconds between two readings and the longest
watch.
The job ends at the first stuck state, at `Running` with each counted rule
passed, or at the limit. Set the limit under the timeout of the job.

### GitLab

The job `strata-preflight` reads one variable for each input, with the prefix
`STRATA_PREFLIGHT_`. [GitLab CI](./gitlab.md#the-variables) lists them and gives
the default of each one.

The job runs the static tier with `--offline`, also when `PALETTE_API_KEY` is
set for the other jobs. For the live tier, or for the watch, set
`STRATA_PREFLIGHT_TIER` to an empty value. The job then reads Palette when the
key is set.

### On another host

The reusable workflow and the GitLab job call one script,
`scripts/ci/preflight.sh`. A pipeline on another host calls it too. The
installer puts it beside `pr-comment.sh`, which the comment needs.

Set `STRATA_BIN` to the binary, and set the variables with the prefix
`STRATA_PREFLIGHT_`. [GitLab CI](./gitlab.md#the-variables) lists them. The
script prints the markdown report and exits with the status of `strata
preflight`.

The script has one option, `--offline`: it reads nothing from Palette. The script
gives it too when `PALETTE_API_KEY` is empty.

## Custom probes of a company

A company adds the rules of its own landing zone in a file of the same format:

```yaml
section: landing-zone
title: The landing zone of Example Corp
rules:
  - id: example-region
    title: The cluster is in an allowed region
    engines: [aws, eks]
    tier: static
    status: proposed
    what: The landing zone holds us-east-2 and eu-west-1. A cluster in another region has no VPC.
    check:
      kind: static
      expr: |
        .entity.spec.cloudConfig.region as $r
        | if (["us-east-2", "eu-west-1"] | index($r)) != null then []
          else ["region \($r) is not in the landing zone"] end
    tests:
      - fault: fixtures/other-region.json
        clean: fixtures/clean.json
```

```bash
strata preflight cluster.json --cloud eks --probes landing-zone.yaml
```

A custom probe follows the same promotion: the company keeps its own ledger of
runs (`--ledger runs.yaml`), and `--promotion` says when a probe meets the bar.
[The rule format](../reference/preflight-rules.md) states the format and the
ledger.

## A rule set for each Palette release

A rule can name the Palette releases it applies to (`releases: "<4.11"`). A run
with `--release 4.10` skips the rules of other releases; a run without it runs
every rule, because a tenant key cannot read the release from Palette: see
[The release of Palette](../internals/palette/limits.md#the-release-of-palette).
To see what a release changes:

```bash
strata preflight --rules --cloud gke --release 4.10 --json | jq -r '.rules[].id' | sort > r410.txt
strata preflight --rules --cloud gke --release 4.11 --json | jq -r '.rules[].id' | sort > r411.txt
diff r410.txt r411.txt
```

A new release that changes the behavior of a rule sets its counts in the ledger
to zero for that release: a preflight check is proven per engine and per
release.

Run the cloud tier again after each upgrade of Palette, before the first
create. The first AWS create after an upgrade updates the CAPA stack of the
account, and an update that the role cannot apply stops each AWS create of the
account. Give the release with its patch (`--release 4.10.12`): a patch release
can change the stack too. See
[After an upgrade of Palette](../use-cases/stop-a-build.md#after-an-upgrade-of-palette).

## In a Rust program

`strata-preflight` is a library too. A Rust program can run the catalog over an
entity of its own, with no command line. [Use strata-preflight as a
library](./library.md#use-strata-preflight-as-a-library) shows the dependency,
the items to start from, and an example crate.

## The lab

The live example of this repository runs the same three tiers around each
build of each cloud: `examples/live/preflight.sh` first, with the checks of the
lab itself (the tools, the auth of the cloud CLIs, the ECR login, the leftovers
of a run before), then `examples/live/preflight-plan.sh` with the test plan of
#91, which makes each fault on purpose and checks that strata finds it, and
appends the run to the ledger. See
[The live example](../development/live-example.md#the-preflight-checks).
