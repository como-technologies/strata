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
well; without it, a failed probe is a line to read.

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

The watch reads the cluster, its summary, and its events every 30 seconds. When
Palette does not give the summary or the events to the key, with a 404 or the
403 of a project key, the reading holds none. A 401 or a 429 stops the watch
with its error, as a read of the world does. It
ends at the first known stuck state with the event that shows it, or when the
cluster is `Running` with each counted rule passed, or at the limit; a warning
is in the report, and the watch reads on. The provider gives
exit `0` at its create timeout while the cluster still builds
([Client tools](../internals/palette/limits.md#what-strata-preflight-does-not-check));
the watch is the step that says `Running`.

## On GitHub and GitLab

`.github/workflows/preflight.yml` is a reusable workflow: the caller gives the
source, the cloud, the tiers, the records, and the release of strata, and the
job installs the two binaries, runs `strata preflight`, writes the markdown to
the job summary, and keeps `preflight/` as an artifact. Its exit status is the
status of strata.

| Input | Meaning |
| --- | --- |
| `source` | The body of the create as a file in the checkout, or `cluster:UID`. |
| `cloud`, `change`, `tier`, `records`, `probes`, `release`, `strict`, `ready` | The options of the command, by the same names. |
| `palette-api`, `strata-version` | A self-hosted Palette, and the release of strata. |
| secrets `palette-api-key`, `palette-project-uid` | For the palette tier and for `cluster:UID`. Without a key the job runs the static tier alone. |
| secret `strata-token` | Reads this repository while it is private. |

`examples/ci/github/strata.yml` holds a `preflight` job beside the other six,
and `examples/ci/gitlab/strata.yml` a `strata-preflight` job: both run the
static tier on each pull request, with the records of the repository. The job
that builds runs the live tier with the key, and the watch after the create.

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
every rule, because a tenant key cannot read the release from Palette. To see
what a release changes:

```bash
strata preflight --rules --cloud gke --release 4.10 --json | jq -r '.rules[].id' | sort > r410.txt
strata preflight --rules --cloud gke --release 4.11 --json | jq -r '.rules[].id' | sort > r411.txt
diff r410.txt r411.txt
```

A new release that changes the behavior of a rule sets its counts in the ledger
to zero for that release: a preflight check is proven per engine and per
release.

## The lab

The live example of this repository runs the same three tiers around each
build of each cloud: `examples/live/preflight.sh` first, with the checks of the
lab itself (the tools, the auth of the cloud CLIs, the ECR login, the leftovers
of a run before), then `examples/live/preflight-plan.sh` with the test plan of
#91, which makes each fault on purpose and checks that strata finds it, and
appends the run to the ledger. See
[The live example](../development/live-example.md#the-preflight-checks).
