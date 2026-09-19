# One pack build under two tag names

## The problem

A registry can give one pack build several tag names. The Public Repo gives
`ubuntu-aws` 24.04 the tags `LTS__24.x`, `LTS__24.4.x`, and `LTS__24.04`, all
with one pack uid. Palette stores the tag that the author of the profile picked:
a profile from the Spectro Cloud provider pins `24.04`. The tag is part of
`IDENTITY`, so two such roots differ, although the nodes run the same build.

In this example, `aws-stack@1.1.2` is `aws-stack@1.1.1` with the OS pack pinned as
`24.04` in place of `LTS__24.x`.

## 1. Compare the versions

```text
{{#include ../output/tag-compare.txt}}
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

`same-release` passes the pair with or without `--allow registry-move`, and
counts the packs with a new tag name.

## 3. A rename in place is still drift

Record 1.1.2 while it still pins the tag `24.04`:

```text
{{#include ../output/tag-record.txt}}
```

Somebody then renames that tag on the published version, to `LTS__24.04`. A
rename changes no build, but it edits a published version:

```text
{{#include ../output/tag-verify.txt}}
```

Exit `1`, with `SAME CONTENT` and no node impact.

## What to do

- Nothing, for the pair: the clusters run the same build. The live run moved a
  cluster between two such versions, and Palette replaced no node.
- Pick one tag style for a team, so the roots of equal versions are equal.
- A tag that names another build, with another version or pack uid, is an
  `IDENTITY` change with the impact `repave`. This rule also applies to a
  floating tag, because the profile stores a pack uid with it.

## Palette behavior

See [What Palette does: tag names](../internals/palette.md#tag-names).
