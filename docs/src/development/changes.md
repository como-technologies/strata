# How a change lands

A change goes to `main` through a pull request, and a second person checks it
before it merges. The merge is the work of GitHub, not of a person: it comes when
the checks pass and the second person has said so. This page states the steps,
and what each one needs.

## The issue

Each change starts from an issue. The issue states the problem, a plan, and a
`Done when:` line. Each item of that line names what to run or what to read,
and what the result must be, so that a second person can test it. An issue with
no such line gets one before any work starts. The person who writes the line
does not do the work: a second person reads the line before the work, too.

## The work

1. Start from a fresh `main`: `git fetch`, then a branch from `origin/main`.
2. Make the change, and the change of the book that goes with it: see
   [Write the documentation](./documentation.md). Each new output of the book
   comes from `just docs-examples`.
3. Give each item of the `Done when:` line a test, when a test can show it.
   Remove the new code for a moment, and make sure that the test fails. Then put
   the code back.
4. Run each check of CI in a clean worktree of the commit, not only in the tree
   you work in:

   ```bash
   just lint && just test && just library-example && just examples && just docs-check && just docs-lint && just docs && cargo test -p xtask
   ```

5. Rebase on a fresh `origin/main`, and push the branch.
6. Open the pull request with auto-merge on, and with a squash:

   ```bash
   gh pr create --title "<title>" --body-file summary.md
   gh pr merge --auto --squash
   ```

   The body names the issue with `Refs #N`, so that the merge leaves the issue
   open. The issue stays open until the evidence is on it.

## The verify

A second person, not the author, checks the pull request against the
`Done when:` line of the issue:

- each item, with the command that shows it and its output;
- each new test: remove the new code, see the test fail, and put it back;
- each statement about Palette, against the Spectro Cloud source or a live read;
- the book, against [the style rules](./documentation.md#the-style-rules).

The verifier writes the result as a comment on the pull request, and sets a
verify status on the head commit: success or failure. The status holds for that
commit only. A new push needs a new verify.

## The merge

A ruleset on `main` holds each change to these rules:

| Rule | What it needs |
| --- | --- |
| Pull request | Each change to `main` comes from a pull request, and merges with a squash. |
| Checks | `checks / lint and test`, `checks / the lowest Rust version`, `checks / docs`, and `checks / build x86_64-unknown-linux-musl` pass. |
| Verify | The verify status of the head commit is success. |
| History | No force push to `main`, and no delete of it. |

With auto-merge on, GitHub merges the pull request when the checks pass and the
verify status is success, and deletes the branch. A verify that fails stops the
merge: the author fixes the change, pushes, and asks for a new verify.

An admin of the repository can push to `main` with no pull request: a change of
the book, or the commit of a release (see [Release](./release.md)).

## The evidence

After the merge, the author puts one comment on the issue: the evidence. It
starts with `Evidence for the acceptance criteria: the commands run, from the
repo root, and the output they printed.`, names the merge commit, and gives one
heading for each item of the `Done when:` line, with the commands and the output
they printed. Then the author ticks each item that the evidence shows. An item
that needs a build in the live example, which costs money, is named in the pull
request and left for a person who runs it. The owner of the issue closes it.

## A fault of Palette

When the work shows that Palette itself is wrong, or could do better, the fault
goes into a report kept outside this repository, with the source line and the
evidence. It is not an issue of this repository. A gap of strata gets an issue
of its own.
