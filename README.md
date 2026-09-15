# strata

A Spectro Cloud Palette cluster profile is a stack of layers: OS, Kubernetes,
CNI, CSI, and add-ons. Each layer is a pack with a version, a digest, and
values. Strata treats each published version of that stack as an immutable
release. It makes the release a thing a team can record, compare, and verify,
like any other build artifact.

- **record** a published version as a snapshot. The snapshot holds a Merkle
  root over the layers, three hashes per layer, the canonical contents, and
  the clusters that use the version.
- **compare** two versions. The report says what kind of change occurred in
  each layer, and what the change does to running clusters.
- **verify** that a published version still agrees with its record. A tool
  enforces immutability, not a sentence in a document.

Think of it as build, release, run. A pack is a build. A profile version is
a release. A cluster is a run. Strata is the record, the review, and the
audit of the release.

**Start here:** [docs/walkthrough.md](docs/walkthrough.md) shows a platform
team that uses strata through one release cycle. Each command in it runs
offline against `examples/`.

## Commands

```
strata my-profile@1.2.0 my-profile@1.3.0        # compare two versions
strata compare A B --format markdown            # the pull request comment
strata compare A B --format json                # for scripts and AI clients
strata snapshot my-profile@1.2.0 -o rel.json    # record a release
strata verify rel.json                          # the immutability check
strata schema diff                              # JSON Schema of the diff document
```

A ref is a profile uid, `<name>@<version>`, or a snapshot file. The exit
status is `0` for identical or unchanged, `1` for different or drifted, and
`2` for an error.

## Build and configure

1. Run `cargo build --release`, or `just build`. The binary is
   `target/release/strata`.
2. Set `PALETTE_API_KEY`.
3. Set `PALETTE_PROJECT_UID` for project-scoped profiles.
4. Set `PALETTE_API` for a self-hosted Palette. The default is
   `https://api.spectrocloud.com`.

Strata reads values that are not in the environment from `~/.palette.env`.

## Where strata fits

The team describes each thing that Palette manages as a definition or an
instance. A definition has a name and a version. It is published and never
changed afterward. An instance is deployed and reconciled to a definition. A
pack is a build. A profile version is a release. A cluster is a run. A tool
enforces immutability. A consumer check runs before end of life.
[docs/way-of-working.md](docs/way-of-working.md) describes the model and the
delivery pipeline.

| moment                          | what strata does                                                                 |
|---------------------------------|----------------------------------------------------------------------------------|
| a profile bump is under review  | `compare --format markdown` between the last published version and the new one. The pull request comment is the evidence the approver reads. |
| a profile version is published  | `snapshot`. The profile repository keeps the file as the record of the release. |
| a cluster tier is about to move | `compare` between the current and the proposed versions. The report shows the repave impact. |
| on a schedule                   | `verify` over each recorded snapshot. This enforces the immutability rule.      |
| before end of life              | `snapshot`, then read `source.consumers`. Delete only when the count is zero.   |

## Documentation

- [docs/walkthrough.md](docs/walkthrough.md): one team, one profile, one
  release cycle, with each command and its output.
- [docs/way-of-working.md](docs/way-of-working.md): the model and the
  delivery pipeline that strata supports.
- [docs/model.md](docs/model.md): the three hashes, the Merkle root, the
  canonical form, the impact levels, and the snapshot document.
- [docs/output.md](docs/output.md): the terminal report, the markdown
  comment, the JSON document for scripts and AI clients, and colors.
- [examples/README.md](examples/README.md): real packs and profiles for
  offline use.

## Tests

`cargo test`, or `just test`, runs unit tests and end-to-end tests over
fixtures with the shape of the API responses. The tests do not use the
network.

## Known limits

- Strata has not compared the pack-ref digest with the registry digest on a
  live profile. No tenant with a cluster profile has been available. Each
  snapshot records which field fed `UPSTREAM` in `hash.upstreamSource`. For
  packs that the pipeline builds, the OCI manifest digest in the immutable
  registry is the better source. It is the planned replacement.
- Refs are profiles only. The applied profile templates of a cluster use the
  same pack shape. They are the planned next input, for release-versus-run
  drift.
- Strata does not hash pack presets or the pack schema. A selected preset
  shows as a values change.
- The text reports truncate values longer than 120 characters. The JSON
  document and `--full` carry the full value.
