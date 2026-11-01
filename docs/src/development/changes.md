# How a change lands

A change goes to `main` through a pull request, and a second person checks it
before it merges. GitHub does the merge, not a person. GitHub merges when the
checks pass and the second person approves. This page states the steps, and what
each one needs.

## The issue

Each change starts from an issue that states the problem, a plan, and a
`Done when:` line. Each item of that line names a command to run or a file to
read, and the result that it must give. A second person can then test it. If an
issue has no such line, write one before the work starts. The person who writes
the line does not do the work. A second person also reads the line before the
work starts.

## The work

1. Start from a fresh `main`: `git fetch`, then a branch from `origin/main`.
2. Make the change, and the related change to the book: see
   [Write the documentation](./documentation.md). Each new output of the book
   comes from `just docs-examples`.
3. Give each item of the `Done when:` line a test, when a test can show it.
   Remove the new code temporarily. Make sure that the test fails. Then restore
   the code.
4. Run each check of CI in a clean worktree of the commit, not only in the tree
   you work in:

   ```bash
   just lint && just test && just library-example && just examples && just docs-check && just docs-lint && just docs
   ```

5. Rebase on a fresh `origin/main`. Then push the branch.
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
- each new test: remove the new code, see the test fail, and restore the code;
- each statement about Palette, against the Spectro Cloud source or a live read;
- the book, against [the style rules](./documentation.md#the-style-rules).

The verifier writes the result as a comment on the pull request, and sets a
verify status on the head commit: success or failure. The status holds for that
commit only. A new push needs a new verify.

The tool that sets the status reads the check `Gate` of the head commit first,
and sets no success while `Gate` has none. `Gate` is a job of the `ci`
workflow. It needs the four checks and does no other work. GitHub runs it only
when each check passed, and skips it when one failed. So one check run names
the full result of a commit, and the ruleset of `main` needs no change for it.

## The merge

A ruleset on `main` applies these rules to each change:

| Rule | What it needs |
| --- | --- |
| Pull request | Each change to `main` comes from a pull request, and merges with a squash. |
| Checks | `checks / lint and test`, `checks / the lowest Rust version`, `checks / docs`, and `checks / build x86_64-unknown-linux-musl` pass. |
| Verify | The verify status of the head commit is success. |
| History | No force push to `main`, and no delete of it. |

With auto-merge on, GitHub merges the pull request when the checks pass and the
verify status is success, and deletes the branch. A verify that fails stops the
merge: the author fixes the change, pushes, and asks for a new verify.

An admin of the repository can bypass each rule of the ruleset. For example, an
admin pushes the commit of a release to `main` with no pull request (see
[Release](./release.md)). The rules do not stop an admin, so an admin follows
the steps of this page for each other change.

## The evidence

After the merge, the author puts one comment on the issue: the evidence. It
starts with `Evidence for the acceptance criteria: the commands run, from the
repo root, and the output they printed.` It names the merge commit. It gives one
heading for each item of the `Done when:` line, with the commands and the output
that they printed. Then the author ticks each item that the evidence shows.

If an item needs a build in the live example, which costs money, the author names
the item in the pull request. A person who runs the live example does that item.
The owner of the issue closes it.

## A fault of Palette

When the work shows a fault of Palette, or a possible improvement to Palette,
the author writes it in a report outside this repository. The report gives the
source line and the evidence. It is not an issue of this repository. A fault of
strata gets an issue of its own.

## The session image

The work and the verify can run in a container of the session image.
`riff.toml` names the image by its digest, and `deploy/session/Dockerfile`
builds it. The image has the Rust of CI, the Rust of `rust-version`, and each
tool of the checks above. It has no key of Palette and no AWS profile, so a
session in the container makes no live request.

To change a tool or a version:

1. Change `deploy/session/Dockerfile` in a pull request. The workflow
   `Session image` builds the image, and pushes it to
   `ghcr.io/como-technologies/strata-session`.
2. Copy the line `image = ...` from the summary of that run into `riff.toml`, in
   the same pull request.

CI builds with the latest stable Rust. When Rust releases a new stable version,
change `RUST_VERSION` in the same way. Then clippy finds the same lints in the
container and in CI.
