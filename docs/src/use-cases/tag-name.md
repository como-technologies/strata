# One pack build under two tag names

## The problem

A registry can give one pack build several tag names. The Public Repo gives the
`ubuntu-aws` build of version 24.04 the tags `LTS__24.x`, `LTS__24.4.x`, and
`LTS__24.04`, all with one pack uid. Palette stores the tag that the author of
the profile picked. A tag can also read the same as the version: a profile from
the Spectro Cloud provider pins the tag `24.04`. The tag is part of `IDENTITY`,
so two such roots differ, although the nodes run the same build.

In this example, `aws-stack@1.1.3` is `aws-stack@1.1.1` with the OS pack pinned as
`24.04` in place of `LTS__24.x`.

**Who runs this.** A person at a terminal. The scheduled verify job reports the
rename of step 3: see [Drift issues](../automation/drift-issues.md).

<div class="playground-demo">

`just playground tag-name-1` serves this tenant on this
machine, with no key and no cost. The rename in step 3 changes the tenant, so
this page has two seeds. Each section names the one it needs, and each seed
serves on its own port.

</div>

## 1. Compare the versions

```text
{{#include ../output/tag-compare-os.txt}}
```

The result is `SAME CONTENT`. The OS line shows `tag LTS__24.x -> 24.04`, with
the impact `none`. `compare` still exits `1`, because the roots differ.

```text
{{#include ../output/tag-json.txt}}
```

## 2. The versions are one release

```text
{{#include ../output/tag-same.txt}}
```

`same-release` passes the pair, and counts the packs with a new tag name. It
needs no option, because a tag rename is `SAME CONTENT` and `same-release`
accepts that. A pack that moves to another registry is a different case, and it
needs `--allow registry-move`: see
[A pack comes from another registry](./registry-move.md).

## 3. A rename in place is still drift

Record 1.1.3 while it still pins the tag `24.04`:

```text
{{#include ../output/tag-record.txt}}
```

<div class="playground-demo">

`just playground tag-name-2` is the same tenant after the
rename. Stop the first seed, and export what this one prints. Keep the record
above.

</div>

Somebody then renames that tag on the published version, to `LTS__24.04`. A
rename changes no build, but it edits a published version:

```text
{{#include ../output/tag-verify-notes.txt}}
```

Exit `1`, with `SAME CONTENT` and no node impact.

Two notes appear together, and the pair is the triage. The first note says that
the version was edited in place. The second says that the edit is a tag rename,
with the same pack build. So no node is at risk, and a rule was still broken,
because a published version changed.

The record no longer matches, so the scheduled job keeps its issue open until
somebody records the version again. Record it again only after you confirm that
somebody renamed the tag on purpose.

## What to do

- Nothing, for the pair: the clusters run the same build. The live run moved a
  cluster between two such versions, and Palette replaced no node.
- Pick one tag style for a team, so the roots of equal versions are equal.
- A tag that names another version is an `IDENTITY` change with the impact
  `repave`. Another pack uid at the same version is not a repave: see
  [The impact levels](../reference/impact.md#the-impact-levels). A floating tag is not covered: no
  run shows whether Palette moves the pack uid of a profile when a new build
  lands under a tag that the profile already pins.

## Palette behavior

See [What Palette does: tag names](../internals/palette/packs.md#tag-names).
