# Preflight in a pipeline

Three jobs, one for each tier. Each one runs where its inputs are.

| Job | When | Command | Needs |
| --- | --- | --- | --- |
| static | each pull request that changes a cluster | `strata preflight cluster.json --cloud eks --tier static --records 'releases/**/*.json' --offline` | the records of the profiles in the repository |
| live | before each build | `strata preflight cluster.json --cloud eks --strict --out preflight` | `PALETTE_API_KEY`; the tools of the cloud for the cloud tier |
| watch | during the build | `strata preflight --watch cluster:$UID --limit 3600` | `PALETTE_API_KEY`; the uid, from the create |

Exit `0` lets the next step run: no counted check failed. The last line says
`NOT COUNTED` when no counted check ran, and `--strict` counts each probe. Exit
`1` stops it: a counted check failed, or the readiness is under `--ready`. Exit
`2` is a failed job, never "no fault": strata could not answer.

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

With a key, strata does these steps:

- It records each profile from Palette.
- It reads the cloud account and the packs.
- It calls the validate routes.
- It runs the checks of the cloud tier with the tools on the machine (`aws`,
  `gcloud`).

A tool that is not there makes its rules `NOT CHECKED`, with the command to run
by hand. `--no-exec` runs no tool at all.

`--out preflight` writes `report.md`, `report.json`, and `report.html`. Keep the
directory as an artifact of the job. `--world` adds `world.json` and
`entity.json`, which the reproduce lines of the report read. They hold the
values of the variables: see [The files of the job](#the-files-of-the-job).

## The watch during the build

```bash
uid=$(curl -s -X POST ... /v1/spectroclusters/eks | jq -r .uid)
strata preflight --watch "cluster:$uid" --every 30 --limit 3600
```

The watch reads the cluster, its summary, and its events every 30 seconds. Each
4xx but 401 and 429 gives a reading with no summary or no events. A 401 or a 429
stops the watch with its error, as a read of the world does.

The watch ends at the first known stuck state, with the event that shows it. It
also ends when the cluster is `Running` with each counted rule passed, or at the
limit. A warning is in the report, and the watch continues. The provider gives
exit `0` at its create timeout while the cluster still builds
([Client tools](../internals/palette/limits.md#what-strata-preflight-does-not-check)).
The watch is the step that says `Running`.

## On GitHub and GitLab

`.github/workflows/preflight.yml` is a reusable workflow. The caller gives the
source, the cloud, the tiers, the records, and the release of strata. The job
checks the signature of `SHA256SUMS`, installs the two binaries, and runs
`scripts/ci/preflight.sh`. It writes the markdown to the job summary, and keeps
the directory of `--out` as an artifact.
The script builds the options of `strata preflight`, and its exit status is the
status of strata.

| Input | Meaning |
| --- | --- |
| `source` | The body of the create as a file in the checkout, or `cluster:UID`. |
| `cloud`, `change`, `tier`, `records`, `probes`, `release`, `strict`, `ready` | The options of the command, by the same names. |
| `no-exec`, `verbose` | `--no-exec` and `--verbose`. |
| `rule` | `--rule`: the ids of the rules, separated by spaces. Strata runs only these rules. |
| `watch`, `every`, `limit` | `--watch`, `--every`, and `--limit`. Give `cluster:UID` as `source`. The job needs the key. |
| `out` | The directory for the three reports. Default `preflight`. The artifact `preflight` holds it. |
| `world` | `true` gives `--world`: the artifact also keeps `world.json` and `entity.json`. Default `false`. See [The files of the job](#the-files-of-the-job). |
| `palette-api`, `strata-version` | A self-hosted Palette, and the release of strata. |
| secrets `palette-api-key`, `palette-project-uid` | For the palette tier and for `cluster:UID`. Without a key the job runs the static tier alone, and says so: see [The key of the job](#the-key-of-the-job). |

The workflow sets `--format markdown`. It sets `--quiet` too, except when
`verbose` is `true`: `--quiet` has priority over `--verbose`. The options `--rules`,
`--promotion`, `--ledger`, and `--schema` run no check of a cluster, so the
workflow has no input for them.

`examples/ci/github/strata.yml` holds a `preflight` job beside the other six.
`examples/ci/gitlab/strata.yml` holds a `strata-preflight` job. Both run the
static tier on each pull request, with the records of the repository. The job
that builds runs the live tier with the key, and the watch after the create.
The GitHub file also holds the job of the comment, in a comment of the file.

### The files of the job

The job keeps the directory of `out` as the artifact `preflight`, and each
reader of the run can download it. By default, the directory holds the three
reports, and no value of a variable. Palette can quote the value of a variable
in a refusal. So each message of a report holds a mark in place of each value.
See [The values in a message](../commands/preflight/report.md#the-values-in-a-message).
Here the job checks the cluster `payments`, whose variable `platform_owner` has
the value `team-payments`:

```text
{{#include ../output/preflight-job-files.txt}}
```

The input `world: true` adds `world.json` and `entity.json`, which the
reproduce lines of the report read. The two files hold the value of each
variable that is not sensitive. [The report on
disk](../commands/preflight/report.md#the-report-on-disk) states what each file
holds. Give the input only when each reader of the run can see these values.
The GitLab job has a variable for the same choice: see
[GitLab CI](./gitlab.md#the-variables).

Without the two files, the reproduce lines name the directory `$OUT`, and the
report says how to write them. Run the command again on your machine with
`--out DIR --world`. Then run the lines with `OUT=DIR`.

The input `verbose: true` writes more lines to the log of the job, and these
lines can hold a value of a variable. Give it only when each reader of the run
can see the values of the variables.

### The key of the job

`preflight.sh` looks for the key in the places where strata looks:
`PALETTE_API_KEY`, `SPECTROCLOUD_APIKEY`, and `~/.palette.env`. It reads the
file as strata does, and only from an absolute `HOME`.
[Settings](../reference/settings.md) states the order and the rule of the home. When
the script finds a key, the job reads Palette. When it finds no key, the script
gives `--offline`, and the job runs the static tier alone.

The first line of the log then says that the run is offline, and why. The report
says `offline` after the tiers, and the JSON report holds `options.offline`. Here
the variable of the key is empty:

```text
{{#include ../output/preflight-job-offline.txt}}
```

An offline job exits `0` when no counted static check fails. The checks that did not
run count against the readiness. To stop such a job, give the input `ready`,
such as `80`. See [Exit codes, and `--strict`](../commands/preflight.md#exit-codes-and---strict).

The role of the key decides which rules of the palette tier can run. The
validate route of the create needs `cluster.create`, and no viewer role gives
it. With a read-only key, the rules of that route give `ERROR`, and the job
exits `2`. See [The key of the live tier](../commands/preflight/tiers.md#the-key-of-the-live-tier).

The script `palette-key.sh` looks for the key. `preflight.sh` and
`merge-gate.sh` call it, and each release carries it beside them. A job can call
it too: exit `0` when it finds a key, and exit `1` when it finds none. With exit
`1` it prints the places where it looked. It never prints the key.

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
    uses: como-technologies/strata/.github/workflows/preflight.yml@v0.33.0
    with:
      source: clusters/payments.json
      strata-version: v0.33.0
  preflight-comment:
    needs: preflight
    if: ${{ !cancelled() && github.event_name == 'pull_request' }}
    uses: como-technologies/strata/.github/workflows/preflight-comment.yml@v0.33.0
    with:
      strata-version: v0.33.0
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

| Input | Meaning |
| --- | --- |
| `strata-version` | The release of strata. Give the tag that the first job uses. |

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

The job runs the static tier with `--offline`, also when the other jobs have
`PALETTE_API_KEY`. For the live tier, or for the watch, set
`STRATA_PREFLIGHT_TIER` to an empty value. The job then reads Palette when it
finds a key: see [The key of the job](#the-key-of-the-job).

### On another host

The reusable workflow and the GitLab job call one script,
`scripts/ci/preflight.sh`. A pipeline on another host calls it too. The
installer puts it beside `palette-key.sh`, which looks for the key, and
`pr-comment.sh`, which the comment needs.

Set `STRATA_BIN` to the binary, and set the variables with the prefix
`STRATA_PREFLIGHT_`. [GitLab CI](./gitlab.md#the-variables) lists them. The
script prints the markdown report and exits with the status of `strata
preflight`.

The script has one option, `--offline`: it reads nothing from Palette. The script
gives it too when it finds no key. The log and the report then say that the run
is offline: see [The key of the job](#the-key-of-the-job).

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

A custom probe follows the same promotion. The company keeps its own ledger of
runs (`--ledger runs.yaml`). `--promotion` says when a probe meets the bar.
[The rule format](../reference/preflight-rules.md) states the format and the
ledger.

## A rule set for each Palette release

A rule can name the Palette releases it applies to (`releases: "<4.11"`). A run
with `--release 4.10` skips the rules of other releases. A run without it runs
every rule, because a tenant key cannot read the release from Palette. A text that
is not a version, such as `4.11.x`, is a `usage` error with exit `2`. See
[The release of Palette](../internals/palette/limits.md#the-release-of-palette).
To see what a release changes:

```bash
strata preflight --rules --cloud gke --release 4.10 --json | jq -r '.rules[].id' | sort > r410.txt
strata preflight --rules --cloud gke --release 4.11 --json | jq -r '.rules[].id' | sort > r411.txt
diff r410.txt r411.txt
```

A new release that changes the behavior of a rule sets its counts in the ledger
to zero for that release. The ledger proves a preflight check per engine and per
release. `strata preflight --promotion --release 4.11` counts only the runs of
`4.11`, and it says `demote` for a rule that no run proved on `4.11`. See
[The runs of one release](../commands/preflight/promotion.md#the-runs-of-one-release).

Run the cloud tier again after each upgrade of Palette, before the first
create. The first AWS create after an upgrade updates the CAPA stack of the
account. An update that the role cannot apply stops each AWS create of the
account. Give the release with its patch (`--release 4.10.12`). A patch release
can change the stack too. See
[After an upgrade of Palette](../use-cases/stop-a-build.md#after-an-upgrade-of-palette).

## In a Rust program

`strata-preflight` is a library too. A Rust program can run the catalog over an
entity of its own, with no command line. [Use strata-preflight as a
library](./library.md#use-strata-preflight-as-a-library) shows the dependency,
the items to start from, and an example crate.

## The lab

The live example of this repository runs the same three tiers around each
build of each cloud. It runs two scripts in this order:

- `examples/live/preflight.sh` runs first. It runs the checks of the lab itself
  (the tools, the auth of the cloud CLIs, the ECR login, the leftovers of a run
  before).
- `examples/live/preflight-plan.sh` runs second. It runs the test plan of #91.
  The plan makes each fault on purpose and checks that strata finds it. The
  script also appends the run to the ledger.

See
[The live example](../development/live-example.md#the-preflight-checks).
