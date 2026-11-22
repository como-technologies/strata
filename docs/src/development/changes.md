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
   the code. When you remove the code in a temporary worktree, give the worktree
   its own target directory: see
   [A temporary worktree](./build-test.md#a-temporary-worktree).
4. Run each check of CI in a clean worktree of the commit, not only in the tree
   you work in. Give that worktree its own target directory too:

   ```bash
   just lint && just comment-words && just test && just library-example && just examples && just docs-check && just docs-lint && just docs
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
workflow. It needs the four checks and
[the scan of the deny list](./build-test.md#the-deny-list), and does no other
work. GitHub runs it only when each check passed, and skips it when one failed.
So one check run names the full result of a commit, and the ruleset of `main`
needs no change for it.

## The merge

A ruleset on `main` applies these rules to each change:

| Rule | What it needs |
| --- | --- |
| Pull request | Each change to `main` comes from a pull request, and merges with a squash. |
| Merge queue | Each pull request merges through the merge queue of `main`. |
| Checks | `checks / lint and test`, `checks / the lowest Rust version`, `checks / docs`, and `checks / build x86_64-unknown-linux-musl` pass. |
| Verify | The status `riff/verify` is success, on the head commit and on the merge group commit. |
| History | No force push to `main`, and no delete of it. |

With auto-merge on, GitHub adds the pull request to the merge queue when the
checks pass and `riff/verify` of the head commit is success. The queue makes a
merge group commit of `main`, each pull request before it in the queue, and the
change. The `ci` workflow runs on the event `merge_group`, and each check of the
ruleset runs again on that commit. When they pass, GitHub moves `main` to the
merge group commit, and deletes the branch. When one fails, the queue removes
the pull request, and `main` does not change.

A verify that fails stops the merge: the author fixes the change, pushes, and
asks for a new verify.

An admin of the repository can bypass each rule of the ruleset. For example, an
admin pushes the commit of a release to `main` with no pull request (see
[Release](./release.md)). The rules do not stop an admin, so an admin follows
the steps of this page for each other change.

### Why a merge queue

Two pull requests can each pass against a `main` that does not have the other.
When both merge, `main` can fail. The merge queue tests each pull request with
the pull requests that merge before it. So a fault that two changes make
together fails in the queue, not on `main`.

The ruleset does not use the rule "Require branches to be up to date before
merging". With that rule, a pull request that is behind `main` cannot merge. So
each merge stops each other pull request until its author merges `main` into it.
The new head commit then needs a new verify. The merge queue tests the merge
result, and keeps the verify of the head commit.

### The verify of a merge group

The verify is a status of the head commit of a pull request, so a merge group
commit has none. The queue waits for `riff/verify` on that commit, so the job
`verify of the merge group` of `ci` sets it. The job runs only on the event
`merge_group`, and starts no verify:

1. `scripts/dev/merge-group-verify.sh` reads the number of the pull request from
   the branch of the merge group.
2. It reads `riff/verify` of the head commit of that pull request.
3. When that status is success, it sets `riff/verify` to success on the merge
   group commit, with the link of the verify.
4. If not, it sets failure, and the queue removes the pull request.

The rule of the checks names no app for `riff/verify`, because two sources set
it. The verify sets it on the head commit, and GitHub Actions sets it on the
merge group commit. If the rule names the app of the verify, the status of the
merge group does not count, and no pull request merges.

### What runs on a merge group

| Job | On a merge group |
| --- | --- |
| `checks`: `lint and test`, `the lowest Rust version`, `docs`, and `build x86_64-unknown-linux-musl` | Runs. These are the checks of the ruleset. |
| `checks`: `benchmarks` | Skips. No rule needs it, and the push to `main` after the merge runs it. |
| `deny list` and `Gate` | Run, as on each event. No rule reads them on the merge group commit. |
| `verify of the merge group` | Runs only on a merge group. |
| `publish the book` | Skips, because it runs only on `main`. The push to `main` after the merge runs it. |

The workflows `codeql`, `Session image`, and `Observer image` have no event
`merge_group`, because no rule needs them. `codeql` runs again on the push to
`main`.

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
builds it. The image has the Rust of CI, the Rust of `rust-version`, the Go of
the observer, and each tool of the checks above. It has no key of Palette and no
AWS profile, so a session in the container makes no live request.

To change a tool or a version:

1. Change `deploy/session/Dockerfile` in a pull request. The workflow
   `Session image` builds the image, and pushes it to
   `ghcr.io/como-technologies/strata-session`.
2. Copy the line `image = ...` from the summary of that run into `riff.toml`, in
   the same pull request.

CI builds with the latest stable Rust. When Rust releases a new stable version,
change `RUST_VERSION` in the same way. Then clippy finds the same lints in the
container and in CI.

The image has the latest stable Go. When Go releases a new stable version, change
`GO_VERSION` and `GO_SHA256` in the same way. Take the checksum of the
`linux-amd64` archive from `https://go.dev/dl/?mode=json`. The image sets
`GOTOOLCHAIN=local` and `GOPROXY=off`. A build then fails when code needs another
Go, or a module outside the standard library.

When a pull request changes the `go` line of a `go.mod`, change `GO_VERSION` and
`GO_SHA256` in the same pull request. A Go update of Dependabot can be such a
change. `GO_VERSION` is then the version of that line, or a newer one. If it is
older, `go vet` stops in each session, because the image does not download
another Go. CI installs the Go of `observer/go.mod`, so only a test finds the
fault there. The test fails when `GO_VERSION` is older than the `go` line of a
module that Git tracks.

### Where the caches and the state go

`riff.toml` puts each cache of a session, and the state of strata, in a folder
that the session can write:

| Files | Folder | Users |
| --- | --- | --- |
| The cargo home, with the crates from crates.io | `$HOME/.cargo`, the volume `strata-cargo` | The sessions of a host |
| The build cache of Rust, from sccache | `$HOME/.cache/sccache`, the volume `strata-sccache` | The sessions of a host |
| The build cache of Go | `/tmp/cache/go-build` | One session |
| The cache of `gh`, with the logs of `gh run view` | `/tmp/cache/gh` | One session |
| The state of strata, with the log of the unknown values | `/tmp/state/strata` | One session |

The container runtime makes each absent folder above a mount, and root owns it.
It makes `$HOME/.cache` for the mount of sccache. riff mounts the clone under
`$HOME/.local/share/riff/`, so the runtime also makes `$HOME/.local`. A session
cannot write in these two folders.

So `riff.toml` sets `XDG_CACHE_HOME` to `/tmp/cache`, and Go and `gh` use that
folder. It sets `XDG_STATE_HOME` to `/tmp/state`, and strata keeps its state in
`/tmp/state/strata`. A session sets no `GOCACHE`, no `GOPATH`, and no
`XDG_STATE_HOME`. Go writes nothing to its `GOPATH`, `$HOME/go`, because
`GOPROXY=off` stops each download of a module. No other tool of the checks
writes under `$HOME/.local`. Each session has a container of its own, so its
`/tmp/cache` and its `/tmp/state` start empty.

## The code of Palette

A session reads the code of Palette at `$HOME/palette`. `riff.toml` mounts it
as the volume `strata-palette`. On the host of the lead, the volume holds a copy
of the repositories of Palette. On another host, the volume is empty. A session
writes nothing to the copy.

Read the route and the model in the API definitions. Read the behavior in the
API server.

A session on the host itself, not in a container, has no `$HOME/palette`. The
notes of the lead give the place of the clones on the host, and the command that
refreshes them. A session on the host runs that command first, then reads the
clones.

The lead refreshes the copy, so it can be some hours old. When a session needs a
newer copy, it asks the lead with `riff tell lead`. Rule 7 of
[Write the documentation](./documentation.md) tells you to read the code first,
and to cite none of it.
