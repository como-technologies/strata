# Summary

[Introduction](./introduction.md)

# Getting started

- [Install](./install.md)
- [Configure](./configure.md)
- [Quick start](./quickstart.md)

# Commands

- [Overview](./commands/index.md)
- [list](./commands/list.md)
- [packs](./commands/packs.md)
- [snapshot](./commands/snapshot.md)
- [candidate](./commands/candidate.md)
- [compare](./commands/compare.md)
- [same-release](./commands/same-release.md)
- [verify](./commands/verify.md)
- [consumers](./commands/consumers.md)
- [schema](./commands/schema.md)

# Use cases

- [Overview](./use-cases/index.md)
- [What changed between two pack versions](./use-cases/pack-versions.md)
- [Review a change before you publish it](./use-cases/review.md)
- [The same names in two projects](./use-cases/same-names.md)
- [Find the profiles that carry a pack](./use-cases/find-a-pack.md)
- [A cluster does not run its release](./use-cases/cluster-release.md)
- [Clusters give variables different values](./use-cases/cluster-values.md)
- [Two clusters on one release differ](./use-cases/cluster-tiers.md)
- [One pack build under two tag names](./use-cases/tag-name.md)
- [One manifest in two profiles](./use-cases/manifest-pack.md)
- [A pack comes from another registry](./use-cases/registry-move.md)
- [A pack has no digest](./use-cases/no-digest.md)
- [Promote a release to another project](./use-cases/promote.md)
- [Retire a version](./use-cases/retire.md)
- [A published version is edited in place](./use-cases/edited-in-place.md)
- [A version bump overwrites a version](./use-cases/overwritten.md)
- [A macro value changes](./use-cases/macros.md)
- [A team pins each environment](./use-cases/fan-out.md)
- [A publish fans out to what pins it](./use-cases/publish-fan-out.md)

# Automation

- [Scripts and exit codes](./automation/scripts.md)
- [Pull request comments](./automation/pr-comments.md)
- [The merge gate](./automation/merge-gate.md)
- [Drift issues](./automation/drift-issues.md)
- [The promotion gate and the retire gate](./automation/gates.md)
- [GitLab CI](./automation/gitlab.md)
- [The Rust library](./automation/library.md)
- [The knurl adapter](./automation/knurl.md)

# Reference

- [Refs](./reference/refs.md)
- [Settings](./reference/settings.md)
- [Change flags and impact](./reference/impact.md)
- [Report formats](./reference/formats.md)
- [JSON documents](./reference/documents.md)
- [Errors](./reference/errors.md)
- [The candidate file](./reference/candidate-file.md)
- [Palette API requests](./reference/palette-api.md)
- [Versions and compatibility](./reference/compatibility.md)

# How it works

- [The hashes and the root](./internals/hashes.md)
- [What Palette does](./internals/palette.md)
  - [Packs and registries](./internals/palette/packs.md)
  - [Profiles](./internals/palette/profiles.md)
  - [Clusters](./internals/palette/clusters.md)
  - [What no run covers](./internals/palette/limits.md)

# Development

- [Build and test](./development/build-test.md)
- [Release](./development/release.md)
- [Write the documentation](./development/documentation.md)
- [The offline examples](./development/examples.md)
- [The live example](./development/live-example.md)
- [What the live example verifies](./development/live-runs.md)
