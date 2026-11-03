# Contribute to strata

This project has a [code of conduct](CODE_OF_CONDUCT.md). Taking part here means
that you keep to it.

## Before you write code

Open an issue first, or write in the issue that you want to take. Two people
must not write the same change.

## Build and test

[Build and test](docs/src/development/build-test.md) gives the commands and the
tools that they need.

Run every check before you open a pull request:

```bash
just lint && just test && just library-example && just examples && just docs-check && just docs
```

Run them in a clean checkout of your commit, and not only in your working tree.
A file that you did not commit can make a broken commit look good.

## Write the documentation

The book in `docs/src/` holds each fact. Put a new fact in the book. Do not put
it in the README, and do not put it in `CLAUDE.md`.

[Write the documentation](docs/src/development/documentation.md) gives the
rules. They include Simplified Technical English (ASD-STE100), which this
repository writes in the book, in the README, in the help text of a command, in
the code comments, and in the commit messages.

Never paste the output of strata into a page. Add a step to a file of
`xtask/src/chapters/`, run `just docs-examples`, and include the file that it
writes under `docs/src/output/`.

## Commit messages

One line in Simplified Technical English (ASD-STE100), with no prefix such as
`feat:` or `fix:`. The squash merge takes the title of the pull request as the
subject, so the title follows the same rule. The release commit
`chore: release X.Y.Z`, which `just version` writes, is the one exception.

## Releases

[Release](docs/src/development/release.md) gives the steps. `just release` cuts
the tag, and the push of that tag builds, signs and publishes the release.

## The license of your contribution

strata is under the Apache License 2.0. When you send a pull request, you agree
that your contribution goes under that license. See [LICENSE](LICENSE).
