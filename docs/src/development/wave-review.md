# The review at the end of a wave

A wave is the set of items of one milestone. At the end of each wave, the lead
reviews the wave. The review reads the reports of real pipelines, and the
lists of the book that name the review. This page gives its steps.

## Read the reports of real pipelines

Each issue of the template "A finding from a real pipeline" is one report: see
[A finding from a real pipeline](./real-pipelines.md). The template gives the
issue the label `real-pipeline`. The reports are the log of the review, and no
other log exists. Each report shows what a team sees from the comment, the
gates, the preflight comment, and the drift and upstream reports.

1. List the reports that no review read. A review gives each report a
   milestone, so such a report has none:

   ```bash
   gh issue list --repo como-technologies/strata --label real-pipeline --state all \
     --search "no:milestone"
   ```

2. Make sure that each report obeys
   [the rule of the names](./real-pipelines.md#the-rule-of-the-names). If a
   report holds data of a client, remove the data from the issue at once. Then
   delete the old revisions from the edit history of the issue. Each reader can
   open a revision that stays.
3. Compare the output of the report with the page of its job or command in the
   book.
4. Write the result on the report, in a comment:
   - A fault of strata: make an item for it, and name the item. The item names
     its question, as [the rules of the scope](../boundary.md#the-rules-of-the-scope)
     tell.
   - A fault of Palette: write that the fault is in Palette. Record the fault in
     the findings log, outside this repository.
   - No fault: link the section of the book that states what the job does.
5. Give the report the milestone of the wave:

   ```bash
   gh issue edit <number> --repo como-technologies/strata --milestone "<milestone>"
   ```

6. Close the report when its comment names no item. Close it with its item
   when the comment names one.

GitHub gives the label of a template to an issue only when the label exists.
Make the label one time, before the first report:

```bash
gh label create real-pipeline --repo como-technologies/strata \
  --description "A finding from a real pipeline, with made-up names"
```

## Read the lists of the review

- [The rules of the scope](../boundary.md#the-rules-of-the-scope): each item of
  the wave obeys the five rules.
- [The gaps that strata fills](../internals/palette/profiles.md#the-gaps-that-strata-fills):
  when Palette adds a route that answers a check of the list, make an item that
  removes the check.
