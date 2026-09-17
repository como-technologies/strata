# Promote a release to another project

## The problem

Each Palette project holds its own copy of a profile version, with its own uid.
The projects `dev` and `prod` both have `aws-stack@1.1.1`. The name and the version
are the same, but nothing in Palette makes the content the same. A person who
builds the prod copy by hand can change one value.

## 1. Record the version in each project

```text
{{#include ../output/promote-record.txt}}
```

The roots already differ: `088136c929bd` and `78a86f401545`.

## 2. Check that the records are one release

```text
{{#include ../output/promote-different.txt}}
```

Exit `1`. The reason names the pack and the value path.

## 3. See the difference

```text
{{#include ../output/promote-compare.txt}}
```

The prod copy keeps its volumes after a delete: `Retain` in place of `Delete`.
The impact is `cluster-wide`.

## 4. Replace the prod copy

Do not edit the prod copy in place. Make sure that no cluster uses it:

```text
{{#include ../output/promote-consumers.txt}}
```

Delete it, and publish it again from the same source as dev. Palette gives the
new copy a new uid. Record it again:

```text
{{#include ../output/promote-again.txt}}
{{#include ../output/promote-same.txt}}
```

`IDENTICAL`: prod has the release that dev approved.

## What to do

- Publish each project from one source: a candidate file or one OpenTofu module.
- Run `same-release` after each promotion, before a cluster moves. Give the
  records in promotion order: `dev.json test.json prod.json`.
- When a cluster already uses the wrong copy, do not delete it. Publish a new
  version from the approved source, and move the cluster to it.

A pair can also differ for a reason that is not a content change:

- The prod copy takes its packs from another registry:
  [A pack comes from another registry](./registry-move.md).
- The prod copy pins a pack under another tag name:
  [One pack build under two tag names](./tag-name.md).
- The profile has a manifest pack: [One manifest in two profiles](./manifest-pack.md).
