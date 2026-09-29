# GitLab CI

The scripts run on GitLab as they run on GitHub. Each host call goes through one
set of functions: `gh` for GitHub, and the REST API v4 with `curl` for GitLab.

| Script | On GitHub | On GitLab |
| --- | --- | --- |
| [`pr-comment.sh`](./pr-comments.md) | one comment on the pull request, and a label | one note on the merge request, and a label |
| [`verify-report.sh`](./drift-issues.md) | one issue per record that drifted, is unverified, or could not be checked | the same, as GitLab issues |
| [`upstream-report.sh`](./upstream-issues.md) | one issue per pack that is behind | the same, as GitLab issues |
| [`merge-gate.sh`](./merge-gate.md) | a required status check | a job that "Pipelines must succeed" waits for |

`pr-comment.sh`, `verify-report.sh`, and `upstream-report.sh` take `--host
github` or `--host gitlab`.
Without it, the CI variables pick: `GITLAB_CI` or `CI_PROJECT_ID` means GitLab,
and everything else means GitHub. A terminal needs `--host gitlab` when no CI
variable is set. `merge-gate.sh` calls no host, and it has no such option.

## The pipeline

`examples/ci/gitlab/strata.yml` holds the six jobs, ready to include. Copy
it into `.gitlab/strata.yml` and include it:

```yaml
include:
  - local: .gitlab/strata.yml

variables:
  STRATA_VERSION: v0.15.0
  STRATA_A: releases/dev.json        # the record that main has
  STRATA_B: candidate.yaml           # what this merge request proposes
  STRATA_RECORDS: "releases/**/*.json"
```

| Job | Runs on | Does |
| --- | --- | --- |
| `strata-comment` | each merge request pipeline | Compares `STRATA_A` with `STRATA_B`, and keeps one note and the label. |
| `strata-merge-gate` | each merge request pipeline | Checks the merge result. Make the merge button wait for it. |
| `strata-verify` | a pipeline schedule | Checks `STRATA_RECORDS` against Palette and keeps one issue per record that drifted, is unverified, or could not be checked. |
| `strata-upstream` | a pipeline schedule | Checks the pins of `STRATA_RECORDS` against the registries and keeps one issue per pack that is behind, with the label `STRATA_BEHIND_LABEL`. |
| `strata-promote-gate` | a pipeline that carries `STRATA_PROMOTE` | Checks that the records of a promotion are one release. Manual. |
| `strata-retire-gate` | a pipeline that carries `STRATA_RETIRE` | Checks that no cluster the key can read runs the version. Manual. |

Add the schedule in CI/CD, Schedules. Make the merge button wait for the gate:
in Settings, Merge requests, turn on "Pipelines must succeed".

## The variables

| Variable | Needed for | Note |
| --- | --- | --- |
| `GITLAB_TOKEN` | the note, the label, the issues | A project or group access token with the `api` scope. **The job token cannot write notes or issues.** Mask it. |
| `PALETTE_API_KEY` | verify, the upstream report, and a ref that is not a file | Mask it. |
| `PALETTE_PROJECT_UID` | a ref in a project | |
| `STRATA_TOKEN` | the download | A token that can read the strata releases, while that repository is private. |
| `STRATA_PROMOTE` | the promotion gate | The records, in promotion order. Give it on the pipeline run, not as a project variable: the job appears only in a pipeline that carries it. |
| `STRATA_RETIRE` | the retire gate | The version to retire, as `name@version`. Give it on the pipeline run, as above. |
| `STRATA_BEHIND_LABEL` | the upstream report | The label of its issues, `strata-behind` by default. |

The jobs read the merge request from `CI_MERGE_REQUEST_IID`, the project from
`CI_PROJECT_ID`, and the API from `CI_API_V4_URL`, so a self-hosted GitLab needs
no setting. `--repo group/subgroup/project` names a project from a terminal.

## Install strata in a job

`install-strata.sh` downloads one release, checks every file against
`SHA256SUMS`, and prints the path of the binary:

```bash
bash install-strata.sh --version v0.15.0 --dir .strata
export STRATA_BIN=$PWD/.strata/strata
.strata/merge-gate.sh "releases/*.json"
```

Each release carries the binary for four targets, `SHA256SUMS`, and the four
scripts, so one download gives a job everything. Use **v0.5.2** or later: it is
the first release with all four. `v0.5.1` carries three of them and no
`merge-gate.sh`, and `v0.5.0` carries none. An older pipeline clones this
repository at the tag and runs what `scripts/` holds there: at `v0.5.0`, the
comment and the report alone. The installer
itself is a release file: the pipeline downloads it first. A token in `STRATA_TOKEN`,
`GITHUB_TOKEN` or `GH_TOKEN` reads a private repository; a public one needs
none.

[Install](../install.md#one-command) lists the options of the installer.

## What is the same, and what is not

The reports, the exit codes, and the markers are the same on both hosts.
[The comment](./pr-comments.md) and [the issues](./drift-issues.md) name the two
markers.

The scripts create the label before they add it, with a description that says
what it means, so a project that does not have it yet gets it.

Two things differ. On GitLab the scripts read the first 100 open issues with the
label, the first 100 labels, and the first 100 notes of a merge request; on
GitHub they read the first 1000 issues, the first 1000 labels, and every page of
the comments. On GitLab `pr-comment.sh --dry-run` needs `GITLAB_TOKEN` in the
environment, and `verify-report.sh --dry-run` and `upstream-report.sh --dry-run`
do not.
