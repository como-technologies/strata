# The way of working that strata supports

## The model

The team uses one way to describe everything that Palette manages. There are
two kinds of thing.

A *definition* has a name and a version. It is published. It is never changed
after publication. Packs and profiles are definitions.

An *instance* is deployed. It changes over time. Palette reconciles it to a
definition. Clusters are instances.

A pack is a build. A profile version is a release. A cluster is a run.
Configuration that changes per deployment stays out of the build. A published
artifact is immutable. A tool enforces that, not a sentence in a document.

Each level has an owner. Each level has a pipeline: build, test, sign,
publish, record. Each publication has an approval gate with named approvers
and named evidence. End of life has triggers and timers. A consumer check runs
before anything is deleted.

## The delivery pipeline

There is one repository per pack, one per profile, and one per cluster.

There are four tiers: dev, test, stage, and prod. Each tier is one Palette
project and one deployment environment. Each environment has its own
reviewers. Rules are loose in dev and strict in prod.

A merge to a profile repository publishes a new profile version to dev. It
never edits an existing version. A tag promotes the same version through
test, stage, and prod. Each step waits at an approval gate.

When a pack is published, each profile that uses it gets a pull request that
bumps the pin. When a profile version is published, each cluster that uses it
gets a pull request that bumps the tier.

Each pipeline step is a script. A developer runs the script on a laptop. CI
runs the same script. No logic lives only in the CI configuration.

## Where strata fits

Strata is one of those scripts. It supplies what the model asks for and
Palette does not give on its own.

| moment                          | what strata does                                                                 |
|---------------------------------|----------------------------------------------------------------------------------|
| a profile bump is under review  | `compare --format markdown` between the last published version and the new one. The pull request comment is the evidence the approver reads. |
| a profile version is published  | `snapshot`. The profile repository keeps the file as the record of the release. |
| a cluster tier is about to move | `compare` between the current and the proposed versions. The report shows the repave impact. |
| on a schedule                   | `verify` over each recorded snapshot. This enforces the immutability rule.      |
| before end of life              | `snapshot`, then read `source.consumers`. Delete only when the count is zero.   |

[walkthrough.md](walkthrough.md) shows each of these moments with the
commands and their output.
