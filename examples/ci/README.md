# The CI drop-ins

One file per code host. Each one runs the same three jobs, from the same three
scripts, with the same reports and exit codes.

| File | Copy it to | Runs |
| --- | --- | --- |
| `github/strata.yml` | `.github/workflows/strata.yml` of your repository | the comment and the merge gate on a pull request, the drift report on a schedule |
| `gitlab/strata.yml` | `.gitlab/strata.yml` of your project, and include it | the comment and the merge gate on a merge request pipeline, the drift report on a pipeline schedule |

Set the refs and the records that your repository holds, and the version of
strata that the jobs install. [The book](../../docs/src/automation/scripts.md)
describes each script, and
[GitLab CI](../../docs/src/automation/gitlab.md) lists the variables that a
GitLab project needs.

## What is not here, and why

The GitHub file calls three **reusable workflows** that live in
`.github/workflows/` of this repository: `comment.yml`, `verify.yml` and
`merge-gate.yml`. GitHub resolves `uses: owner/repo/.github/workflows/x.yml@tag`
only under that path, so those three cannot move here. They are published, not
examples: your repository calls them and never copies them.

The GitLab file needs no such thing, because an include takes any path. It
installs the release itself and runs the scripts from it.

`.github/workflows/ci.yml` and `release.yml` are the CI of this repository, not
something to copy.
