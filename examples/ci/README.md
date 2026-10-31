# The CI drop-ins

One file per code host. Each one runs the same seven jobs, with the same reports
and exit codes. Five of them run from the scripts in `scripts/ci/`. The two
gates run `strata same-release` and `strata consumers` directly, because the
exit status of those commands is the gate. The preflight job calls
`preflight.sh`, which passes the exit status of `strata preflight` on.

| File | Copy it to | Runs |
| --- | --- | --- |
| `github/strata.yml` | `.github/workflows/strata.yml` of your repository | the comment, the preflight check, and the merge gate on a pull request, the drift report and the upstream report on a schedule, the promotion gate and the retire gate on demand |
| `gitlab/strata.yml` | `.gitlab/strata.yml` of your project, and include it | the comment, the preflight check, and the merge gate on a merge request pipeline, the drift report and the upstream report on a pipeline schedule, the promotion gate and the retire gate as manual jobs |

Set the refs and the records that your repository holds, and the version of
strata that the jobs install. Each option that a pipeline sets has an input in
the reusable workflow and a variable in the GitLab file. The comments of the two
files list them. [The book](../../docs/src/automation/scripts.md)
describes each script, and
[GitLab CI](../../docs/src/automation/gitlab.md) lists the variables that a
GitLab project needs.

## What is not here, and why

The GitHub file calls eight **reusable workflows** that live in
`.github/workflows/` of this repository: `comment.yml`, `preflight.yml`,
`preflight-comment.yml`, `verify.yml`, `upstream.yml`, `merge-gate.yml`,
`promote-gate.yml` and `retire-gate.yml`. The job of `preflight-comment.yml` is
in a comment of the file: turn it on to get the preflight report as a comment.
GitHub resolves `uses: owner/repo/.github/workflows/x.yml@tag` only under that
path, so those eight cannot move here. They are published, not
examples: your repository calls them and never copies them.

The GitLab file needs no such thing, because an include takes any path. It
installs the release itself and runs the scripts from it.

`.github/workflows/ci.yml` and `release.yml` are the CI of this repository, not
something to copy.
