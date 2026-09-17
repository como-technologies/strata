# Review a change before you publish it

## The problem

A pull request changes a profile: here, four infrastructure packs get new
versions. The new profile version is not in Palette yet. The reviewer must see
which layers change, and whether the change replaces nodes, before anybody
publishes it.
After the publication, the team must know that Palette has the version that the
reviewer approved.

The project `dev` has `aws-pxk@1.0.0`, recorded at `releases/1.0.0/dev.json`.

## 1. Write the candidate file

The pull request holds a candidate file. A layer with no values gets the default
values of its pack.

```text
{{#include ../output/review-candidate.txt}}
```

[The candidate file](../reference/candidate-file.md) gives each field. To start
from a published version, use [strata candidate](../commands/candidate.md).

## 2. Build its snapshot

```text
{{#include ../output/review-snapshot.txt}}
```

Strata finds each pack in its registry. The root is the root of the version
after somebody publishes it.

## 3. Compare it with the last release

````text
{{#include ../output/review-markdown.txt:1:18}}
...
````

Read the impact column first. Two layers say `node repave`: this release
replaces each node. A collapsible block below the table lists each changed
value, as `-` and `+` lines.

To list only the layers that replace nodes:

```text
{{#include ../output/review-repave.txt}}
```

## 4. Post the comparison on the pull request

```text
{{#include ../output/review-pr-comment.txt}}
```

Without `--dry-run`, the script posts the comment and adds the label `repave`.
A label can require a second approver. See
[Pull request comments](../automation/pr-comments.md).

## 5. After the publication, prove that it is the approved version

Record the published version, and compare it with the candidate:

```text
{{#include ../output/review-published.txt}}
{{#include ../output/review-same.txt}}
```

`IDENTICAL`: Palette has the version that the reviewer approved.

## Palette behavior

A published profile version has the same root as its candidate, because Palette
puts the registry digest on each pack reference. See
[What Palette does](../internals/palette.md#the-pack-ref-digest).

## What to do

- Keep the candidate file in the repository of the profile. Run steps 2 to 4 on
  each push to the pull request.
- Run step 5 in the job that publishes.
- When step 5 fails, somebody changed the version between the review and the
  publication. Do not use the version. Find the cause.
