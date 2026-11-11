# A finding from a real pipeline

The tests and the book run the jobs of strata against the stand-in and the lab.
A team runs the same jobs in its own pipelines, on its own changes, for many
weeks. There it sees what the comment, the gates, the preflight comment, and
the drift and upstream reports tell a reviewer. Report a finding from such a
pipeline with the issue template "A finding from a real pipeline". The review
at the end of each wave reads each report: see
[The review at the end of a wave](./wave-review.md).

[Open the template](https://github.com/como-technologies/strata/issues/new?template=real-pipeline.yml).
The file of the template is `.github/ISSUE_TEMPLATE/real-pipeline.yml`.

## Run the jobs in your pipeline

The repository is public, so the drop-ins install strata with no token of
strata. Include `examples/ci/gitlab/strata.yml` in a GitLab pipeline, or copy
`examples/ci/github/strata.yml` into a GitHub repository: see
[The scripts in this repository](../automation/scripts.md#the-scripts-in-this-repository).
The jobs run on your runners, with the variables that you give. Strata needs no
access to your pipeline or to your tenant to get a report.

## The rule of the names

Each issue of this repository is public, and GitHub keeps each edit of an
issue. So a report holds no data of a client, from its first version:

- Replace the name of each object of your team with a made-up name. Use
  `barney` and `fred`, as in `barney-addons@1.3.1` and `fred-edge@1.0.0`. This
  rule applies to a profile, a cluster, a project, a registry, and a pack of
  your own.
- Use one made-up name for one object in the whole report, so that a reader can
  follow it.
- Replace each uid, digest, and root with a made-up value. When two values are
  equal in the output, keep them equal in the report.
- Give no host, no address, no account, and no key.
- Give no name of a person.
- Give no values, no manifest, no record, and no log of a job. Copy only the
  lines of the output that show the finding.
- When you are not sure that a name is public, replace it.

A version number, such as `1.3.1`, can stay. A pack of a public registry, such
as `cert-manager` from the registry `Public Repo`, can keep its name.

## What the template asks for

| Field | What to give |
| --- | --- |
| The job or the command | The job of the drop-in, such as `strata-comment`, or the command line. |
| The output, with each name replaced | The lines that the job or the command printed. |
| What you expected | What the job or the command must show, and why. |
| The version of strata | The output of `strata --version`, or the value of `STRATA_VERSION` in the pipeline. |
| The rule of the names | A box to tick: the report obeys the rule above. |

## Where a report goes

The template gives the label `real-pipeline` to the issue. The review at the end
of each wave reads each issue of that label. A fault of strata becomes an item.
The lead records a fault of Palette in the findings log, outside this
repository.
