# Walking a use case

One use case at a time. Serve its chapter, run every step against the served
tenant, and let the real output decide what the page says. Do not write a claim
that a run has not shown.

This file is not part of the book. It is the procedure and the progress.

## The ten items

Do them in order. Item 10 is the only one that writes a commit.

1. **Serve the chapter.** `docs/examples/run.sh --serve <chapter>`, and export
   every line it prints. Note the chapter the page belongs to, and whether the
   page spans more than one, or continues past `serve_here`.
2. **The problem.** Read `## The problem`, then state in one sentence what the
   operator knows and what they are afraid of. Check the fixture creates that
   situation. A page whose gate cannot fail is a fixture bug, not a wording bug.
3. **Walk it.** Run every command of the page in order, against the served
   tenant. Keep the real output. A step that cannot run is a finding.
4. **The actor of each step.** Person or job, taken from what the step needs:
   a key, a checkout, a merge request, a schedule. Not from what reads well.
   A step that a job runs names the recipe, or says none ships.
5. **The decision chain.** For each step, name what in its output decides the
   next step. When nothing in the output decides it, the page is missing a
   command, or the step is in the wrong place. Two steps that run one command
   and get two answers must show what changed between them: a `quiet` line in
   the chapter hides the cause from the reader.
5a. **The words and the tenant.** A term the page uses and never explains stops
   a reader who does not have it already. A tenant the page describes and never
   says how to serve stops one who wants to follow along.
6. **The claims.** Every sentence that asserts something about the output, one
   at a time, against the run. Delete or correct the rest.
7. **The excerpts.** Where a claim covers a few lines of a long block, make it
   an `excerpt` so the claim becomes an assertion. Where the page teaches most
   of the block, leave the block whole.
8. **The exit codes.** The exits the page states are the exits `run.sh` asserts.
9. **The links.** Every link resolves, and a named recipe exists.
10. **Verify and commit.** `just docs-examples`, `just docs-check`, `just docs`,
    `just test`, `just lint`, each as its own step, then one commit.

## The use cases

In the order the book teaches them.

| # | Page | Chapter | State |
| --- | --- | --- | --- |
| 1 | `review.md` | `review` | walked 2026-09-19: steps 1 to 5 run; which steps need a key is now in the page |
| 2 | `pack-versions.md` | `packs` | walked 2026-09-19: 4 steps run; step 1 now names the impact line |
| 3 | `tag-name.md` | `tag-name` | walked 2026-09-19: 3 steps run; the two notes are the triage; the registry-move mention went |
| 4 | `manifest-pack.md` | `manifest-pack` | walked 2026-09-19: found the empty-uid regression; --full names the line |
| 5 | `registry-move.md` | `registry-move` | walked 2026-09-19: records now shown; step 5 reachable; every line replayed |
| 6 | `promote.md` | `promote` | walked 2026-09-19: two promoted versions, one right and one hand-built; every command replays |
| 7 | `same-names.md` | `same-names` | walked 2026-09-19: all 7 commands replay; the verify claim is now shown, not asserted |
| 8 | `edited-in-place.md` | `edited-in-place` | walked 2026-09-19: the two verdicts are a pair |
| 9 | `overwritten.md` | `overwritten` | |
| 10 | `cluster-release.md` | `cluster-release` | steps run; excerpt added. Items 4 to 9 open |
| 11 | `cluster-values.md` | `cluster-values` | |
| 12 | `find-a-pack.md` | `list` | |
| 13 | `retire.md` | `retire` | |
| 14 | `fan-out.md` | several | |
| 15 | `publish-fan-out.md` | several | |

## What a walk cannot reach

`serve_here` stops the playground before the change that the page's later steps
describe. To walk those steps, make the change against the served tenant the way
the chapter does, and say so in the notes. `run.sh` holds the change: read the
chapter.
