# A pull request comment and a script

The same comparison, for a pull request and for a job. [strata compare](../compare.md)
gives the other questions, the notes, what to expect, and the options.

## A pull request comment

`--format markdown` writes the same comparison as tables, with one
collapsible block for each changed pack:

```text
{{#include ../../output/compare-markdown.txt:1:2}}
```

{{#include ../../output/compare-markdown.txt:3:}}

`scripts/ci/pr-comment.sh` posts it: see
[Pull request comments](../../automation/pr-comments.md).

## A script

`--json` writes [`strata-diff/1`](../../reference/documents.md#strata-diff1).
`summary` gives the result in one object:

```text
{{#include ../../output/compare-json.txt}}
```
