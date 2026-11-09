# GitLab CI

The scripts run on GitLab as they run on GitHub. Each host call goes through one
set of functions: `gh` for GitHub, and the REST API v4 with `curl` for GitLab.

| Script | On GitHub | On GitLab |
| --- | --- | --- |
| [`pr-comment.sh`](./pr-comments.md) | one comment on the pull request, and a label; with `--preflight`, one comment for the preflight report | one note on the merge request, and a label; with `--preflight`, one note for the preflight report |
| [`verify-report.sh`](./drift-issues.md) | one issue per record that drifted, is unverified, or could not be checked | the same, as GitLab issues |
| [`upstream-report.sh`](./upstream-issues.md) | one issue per pack that is behind | the same, as GitLab issues |
| [`preflight.sh`](./preflight.md#on-another-host) | `strata preflight` with its options from the inputs of the workflow | the same, from the variables `STRATA_PREFLIGHT_*`; with `STRATA_PREFLIGHT_COMMENT`, one note for the report |
| [`merge-gate.sh`](./merge-gate.md) | a required status check | a job that "Pipelines must succeed" waits for |

`pr-comment.sh`, `verify-report.sh`, and `upstream-report.sh` take `--host
github` or `--host gitlab`.
Without it, the CI variables pick: `GITLAB_CI` or `CI_PROJECT_ID` means GitLab,
and everything else means GitHub. A terminal needs `--host gitlab` when no CI
variable is set. `merge-gate.sh` calls no host, and it has no such option. `preflight.sh` has none
either: its comment picks the host from the CI variables.

## The pipeline

`examples/ci/gitlab/strata.yml` holds the jobs below, ready to include. Copy
it into `.gitlab/strata.yml` and include it:

```yaml
include:
  - local: .gitlab/strata.yml

variables:
  STRATA_VERSION: v0.32.2
  STRATA_A: releases/dev.json        # the record that main has
  STRATA_B: candidate.yaml           # what this merge request proposes
  STRATA_RECORDS: "releases/**/*.json"
```

| Job | Runs on | Does |
| --- | --- | --- |
| `strata-comment` | each merge request pipeline | Compares `STRATA_A` with `STRATA_B`, and keeps one note and the label. |
| `strata-preflight` | each merge request pipeline | Runs `preflight.sh` on the body of a create, and keeps the report as an artifact. With `STRATA_PREFLIGHT_COMMENT`, it also keeps one note. See [Preflight in a pipeline](./preflight.md#on-github-and-gitlab). |
| `strata-merge-gate` | each merge request pipeline | Checks the merge result. Make the merge button wait for it. |
| `strata-verify` | a pipeline schedule | Checks `STRATA_RECORDS` against Palette and keeps one issue per record that drifted, is unverified, or could not be checked. |
| `strata-upstream` | a pipeline schedule, when `STRATA_SAVE` is empty | Checks the pins of `STRATA_RECORDS` against the registries and keeps one issue per pack that is behind, with the label `STRATA_BEHIND_LABEL`. |
| `strata-upstream-save` | a pipeline schedule, when `STRATA_SAVE` names a file | Does the same as `strata-upstream`, and keeps the file as an artifact. |
| `strata-promote-gate` | a pipeline that carries `STRATA_PROMOTE` | Checks that the records of a promotion are one release. Manual. |
| `strata-retire-gate` | a pipeline that carries `STRATA_RETIRE` | Checks that no cluster the key can read runs the version. Manual. |
| `strata-observer-propose` | a pipeline schedule, when `STRATA_OBSERVER_KUBECONFIG` is set | Keeps one merge request open with the first-seen records of the observer. `GITLAB_TOKEN` pushes its branch. |
| `strata-observer-publish` | each push to the default branch, when `STRATA_OBSERVER_KUBECONFIG` is set | Writes `STRATA_OBSERVER_RECORDS` into the Secrets of the observer, with the revision of the push. |

Add the schedule in CI/CD, Schedules. Make the merge button wait for the gate:
in Settings, Merge requests, turn on "Pipelines must succeed".

## The variables

| Variable | Needed for | Note |
| --- | --- | --- |
| `GITLAB_TOKEN` | the note, the label, the issues | A project or group access token with the `api` scope. **The job token cannot write notes or issues.** Mask it. |
| `PALETTE_API_KEY` | verify, the upstream report, `STRATA_REPAVE`, and a ref that is not a file | Mask it. |
| `PALETTE_PROJECT_UID` | a ref in a project | |
| `STRATA_REPAVE` | the comment | `true` asks Palette if the change repaves the cluster, and its answer decides the label. `false` by default. See [Pull request comments](./pr-comments.md#ask-palette-if-the-change-repaves). |
| `STRATA_TOKEN` | the download | A token that can read the strata releases, while that repository is private. |
| `STRATA_PROMOTE` | the promotion gate | The records, in promotion order. Give it on the pipeline run, not as a project variable: the job appears only in a pipeline that carries it. |
| `STRATA_RETIRE` | the retire gate | The version to retire, as `name@version`. Give it on the pipeline run, as above. |
| `STRATA_BEHIND_LABEL` | the upstream report | The label of its issues, `strata-behind` by default. |
| `STRATA_ALLOW_NO_DIGEST` | the merge gate, the drift report | `true` gives the script `--allow-no-digest`: a pack with no upstream digest does not make the record red. `false` by default. |
| `STRATA_WHO` | the drift report | `true` gives the script `--who`: the issue of a record that drifted names who changed it. `false` by default. |
| `STRATA_NO_IMPACT` | the upstream report | `true` gives the script `--no-impact`. `false` by default. |
| `STRATA_SAVE` | the upstream report | A file for `--save`, which keeps the report of the run. Then `strata-upstream-save` runs in place of `strata-upstream`, and keeps the file as an artifact. Empty by default: no file, and no artifact. |
| `STRATA_PROMOTE_ALLOW` | the promotion gate | The differences to accept, separated by spaces: `registry-move`, `no-digest`. Empty by default. |
| `PALETTE_PROJECT` | a ref in a project | The project by name, for a ref such as `name@version` in the promotion gate or the retire gate. `PALETTE_PROJECT_UID` has priority over it. |
| `STRATA_PREFLIGHT_SOURCE` | the preflight job | The body of the create, or `cluster:UID`. `clusters/payments.json` by default. |
| `STRATA_PREFLIGHT_CLOUD`, `_TIER`, `_RECORDS` | the preflight job | `--cloud`, `--tier`, and `--records`. `eks`, `static`, and `releases/**/*.json` by default. |
| `STRATA_PREFLIGHT_CHANGE`, `_PROBES`, `_RELEASE`, `_READY`, `_RULE`, `_EVERY`, `_LIMIT` | the preflight job | The change file, `--probes`, `--release`, `--ready`, `--rule`, `--every`, and `--limit`. Empty by default: the job does not give the option. `_RULE` is a list separated by spaces. |
| `STRATA_PREFLIGHT_STRICT`, `_NO_EXEC`, `_VERBOSE`, `_WATCH` | the preflight job | `true` gives the option of the same name. `false` by default. |
| `STRATA_PREFLIGHT_OUT` | the preflight job | The directory for `--out`. `preflight` by default. The job keeps it as an artifact. |
| `STRATA_PREFLIGHT_COMMENT` | the preflight job | `true` posts `report.md` as one note on the merge request, with `pr-comment.sh --preflight`. `false` by default. It needs `GITLAB_TOKEN`. |
| `STRATA_OBSERVER_KUBECONFIG` | the two jobs of the observer | A CI/CD variable of the type File: a kubeconfig of the publisher of the chart. Without it, the two jobs do not run. See [Approve the records with a pipeline](./observer.md#approve-the-records-with-a-pipeline). |
| `STRATA_OBSERVER_DIR`, `_NAMESPACE` | `strata-observer-propose` | `--dir` and `--namespace` of `observer-propose.sh`. `releases/observer` and `strata-observer` by default. |
| `STRATA_OBSERVER_RECORDS` | `strata-observer-publish` | The approved records that the job writes into the Secrets, with the revision of the push. `releases/observer/*.json` by default. |
| `STRATA_KUBECTL_VERSION` | the two jobs of the observer | The version of kubectl that the jobs install, with its checksum checked. |

The jobs read the merge request from `CI_MERGE_REQUEST_IID`, the project from
`CI_PROJECT_ID`, and the API from `CI_API_V4_URL`, so a self-hosted GitLab needs
no setting. `--repo group/subgroup/project` names a project from a terminal.

## Install strata in a job

`install-strata.sh` downloads one release. When `cosign` 3 or later is on the
`PATH`, it checks the signature of `SHA256SUMS`. It checks every file against
`SHA256SUMS`, and prints the path of the binary:

```bash
bash install-strata.sh --version v0.32.2 --dir .strata
export STRATA_BIN=$PWD/.strata/strata
.strata/merge-gate.sh "releases/*.json"
```

Each release carries the binary for four targets, `SHA256SUMS`, and the five
scripts, so one download gives a job everything. Use **v0.5.2** or later: it is
the first release with all four. `v0.5.1` carries three of them and no
`merge-gate.sh`, and `v0.5.0` carries none. An older pipeline clones this
repository at the tag and runs what `scripts/` holds there: at `v0.5.0`, the
comment and the report alone. The installer
itself is a release file: the pipeline downloads it first. A token in `STRATA_TOKEN`,
`GITHUB_TOKEN` or `GH_TOKEN` reads a private repository; a public one needs
none.

[Install](../install.md#one-command) lists the options of the installer.
[Check the signature](../install.md#check-the-signature) gives the identity that
the signature must have. The image of the drop-in has no `cosign`, so its job
prints that it did not check the signature. To require the check, install
`cosign` 3 or later in the job. Then add `--require-signature` to the command.

## What is the same, and what is not

The reports, the exit codes, and the markers are the same on both hosts.
[The comment](./pr-comments.md) and [the issues](./drift-issues.md) name the two
markers.

The scripts create the label before they add it. The label has a description
that says what it means. So a project that does not have the label yet gets it.

One thing differs. The scripts read these items:

- On GitLab: the first 100 open issues with the label, the first 100 labels, and
  the first 100 notes of a merge request.
- On GitHub: the first 1000 issues, the first 1000 labels, and every page of the
  comments.

A dry run calls no host. So `pr-comment.sh --dry-run`, `verify-report.sh --dry-run`,
and `upstream-report.sh --dry-run` need no `GITLAB_TOKEN`.
