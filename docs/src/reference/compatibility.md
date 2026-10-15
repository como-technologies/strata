# Versions and compatibility

Strata has three interfaces. Each has its own version rules.

## The Rust library

The crate follows semantic versioning over the public API that `cargo doc` shows
under `strata::`. While the version is 0.x, a minor release, such as 0.4 to
0.5, can change that API. A patch release, such as 0.4.0 to 0.4.1, can add to
it. A patch release removes nothing from it, and changes nothing in it. Code
inside the crate, such as the canonical form, is not part of the API. Each
release has the tag `v<version>`. The crate builds with Rust 1.88 or later. The
crate is not on crates.io, so a dependency names the tag: see
[The library](../automation/library.md#add-the-dependency).

Release 0.22 moves five public modules under new parents. Each old path stays as
a `pub use` until release 0.23:

- `strata::color` is now `strata::term::color`.
- `strata::progress` is now `strata::term::progress`.
- `strata::layer` is now `strata::content::layer`.
- `strata::impact` is now `strata::diff::impact`.
- `strata::version` is now `strata::reference::version`.

## The documents

Each JSON document has a schema id, such as `strata-snapshot/1`. The id changes
only when a reader of the old id reads the document wrong:

- a field is removed, renamed, or gets a new meaning, or
- a hash is calculated differently.

A new field keeps the id. Strata ignores fields that it does not know, so it
reads a record of its own id that a later release wrote. `source.projectUid` and
`source.cluster` joined `strata-snapshot/1` in this way. After 0.20.0,
`consumers` of `strata-list/1` counts the cluster templates with the clusters,
and `consumersByKind` joined it with the count of each kind.

Two values changed after 0.21.0 with no new id. In `strata-packs/1`, a version
that the listing gives no type has the `type` `spectro`, where 0.21.0 wrote
empty text. The schema now gives the field the pack types. A record of a
cluster leaves out a label whose value is not text, where 0.21.0 kept it with
empty text. The labels are outside the root, so no root changes.

A field can also become absent, and that keeps the id as well. For most fields
absent is a value: a field that is false, empty, or zero is left out, and each
such field says so at its own definition in
[JSON documents](./documents.md). Eight fields are different, and for these
absent means unknown: `source.consumers`, `rootLive`, `maxImpact`,
`packsChanged`, `error`, `updatesPending`, `repavePending`, and the `value`,
`old`, or `new` of a sensitive variable. `updatesPending` joined
`source.cluster` in 0.10.0, and `strata-list/1` in the same release: absent
means that strata could not read the notifications, and `updatesPendingError`
says why. `repavePending` joined `source.cluster` after 0.17.0: absent means a
record of an older release, or of an object that is not a cluster. In 0.7.0
`source.consumers` became absent on a record that no consumers request
produced, where 0.6.3 wrote `count: 0` and stated a count that nothing counted.
A reader that takes absent for zero reads such a record wrong, so this page
names each field that changed this way, with the release that changed it. The
id stays, because the root does not cover these fields and every record of the
id still compares.

One root changed after 0.17.0 with no new id. A record of an app profile now
holds the install order of each tier, and lists the tiers in that order. See
[Edge hosts, cluster groups, and app profiles](../commands/snapshot/other-objects.md).
So when the tiers of an app profile have an install order, its root differs
from the root that 0.17.0 gives. `verify` of such an older record shows a
change. An app profile whose tiers have no install order keeps its root.

Roots are comparable only within one snapshot schema id. Strata refuses a
snapshot of another id with `unsupported-schema`, which names both ids. A file
whose id names another document, or no document, is not a snapshot. The report
that `behind --since` reads follows the same rule. The JSON Schema of each
document holds its id as a `const`. So a validator that reads
`strata schema <kind>` refuses a document of another id too.

### A value that strata does not know

A later release can also add a value to a closed set, such as a new `template`,
and keep the id. Strata reads such a value with no error, and keeps its text.
Each document that strata writes from the record holds the same text. These
fields take a value of a later release:

- `tool.impl` of a record and of `strata-behind/1`.
- `source.template`, `source.cluster.kind`, the name of each tier in
  `source.cluster.tiers`, and `upstreamSource` in the `hash` of each pack, in
  `strata-snapshot/1`.
- `bump.impact` and each value of `bump.flags[]`, in `strata-behind/1`.

A release does not rename or remove a value of these sets and keep the id.

The commands do this with such a value:

- `compare` compares the text. Two records of the same new `kind` compare their
  tiers, as two cluster records do. Two records that carry the same new tier
  compare its `fields`. Only two cluster records compare the values of their
  variables, so two records of a new `kind` do not.
- `verify` refuses a record whose `source.cluster.kind` it does not know, with
  `usage` and exit `2`. Strata cannot read such an object from Palette, so it
  sends no request.
- `verify` does not compare a tier that it does not know, because Palette gives
  strata no such tier. A note in the report names the tier.
- A record with a new `template` is not a draft. A pack with a new
  `upstreamSource` has a digest, so strata does not call its upstream unknown.
- `behind --since` reads the time of the previous report, and the pinned and
  the newer versions of each pin. It does not read the `bump` of a pin.

`strata schema` agrees: each of these fields takes each value of its set, or
any other text. The other closed sets of these two documents are text, so
strata reads each value of them:

- the `type` and the `layer` of a pack
- the `type` of a profile
- `pinnedState`, and the `state` of a newer version
- the `status` of a sync

Strata does not read its other documents again, such as `strata-packs/1` and
`strata-diff/1`, so their closed sets stay closed. A program can read such a
document with the types of the library, such as `ops::PackBuild`. Then a value
of a later release gives an error.

## The command line

These do not change in a release, so a script can use them:

- the exit codes `0`, `1`, and `2`
- the JSON documents
- the error `kind` names: see [Errors](./errors.md)

After 0.21.0, the kinds `transport` and `not-json` give the field `method`, and
their message names the method of the request. In 0.21.0, the message said
`GET` also for a `POST`.

The terminal text and the markdown can change in any release. The candidate
file rejects fields that it does not know. So a file with a field from a later
release fails on an older strata with `candidate-invalid`. A new value of a
closed set, such as a new `type` of a layer, fails in the same way. A person
writes the file, so strata refuses the value and does not guess.
