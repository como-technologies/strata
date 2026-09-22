# Versions and compatibility

Strata has three interfaces. Each has its own version rules.

## The Rust library

The crate follows semantic versioning over the public API that `cargo doc` shows
under `strata::`. While the version is 0.x, a minor release, such as 0.4 to 0.5,
can change that API. A patch release, such as 0.4.0 to 0.4.1, does not. Code
inside the crate, such as the canonical form, is not part of the API. Each
release has the tag `v<version>`.

## The documents

Each JSON document has a schema id, such as `strata-snapshot/1`. The id changes
only when a reader of the old id reads the document wrong:

- a field is removed, renamed, or gets a new meaning, or
- a hash is calculated differently.

A new field keeps the id. Strata ignores fields that it does not know, so it
reads a record of its own id that a later release wrote. `source.projectUid` and
`source.cluster` joined `strata-snapshot/1` in this way.

A field can also become absent, and that keeps the id as well. A field is
absent when strata cannot state it, so absent is not a value: read it as
unknown, and not as zero or empty. In 0.7.0 `source.consumers` became absent on
a record that no consumers request produced, where 0.6.3 wrote `count: 0` and
stated a count that nothing counted. A reader that takes absent for zero reads
such a record wrong, so each release names the fields that changed this way.
The id stays, because the root does not cover these fields and every record of
the id still compares.

Roots are comparable only within one snapshot schema id. Strata refuses a
snapshot of another id with `unsupported-schema`, which names both ids. The
JSON Schema of each document holds its id as a `const`, so a validator that
reads `strata schema <kind>` refuses a document of another id too.

## The command line

These do not change in a release, so a script can use them:

- the exit codes `0`, `1`, and `2`
- the JSON documents
- the error `kind` names: a new kind can appear, and a kind keeps its meaning

The terminal text and the markdown can change in any release. The candidate
file rejects fields that it does not know. So a file with a field from a later
release fails on an older strata with `candidate-invalid`.
