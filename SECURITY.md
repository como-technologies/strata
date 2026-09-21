# Security

## Report a vulnerability

Do not open a public issue for a security problem, and do not put one in a pull
request.

This repository is private today, so send the report to
<security@comotechnologies.io>.

When the repository becomes public, use the private advisory form at
<https://github.com/como-technologies/strata/security/advisories/new>. GitHub
gives that form only to a public repository, and an owner must first turn on
private vulnerability reporting in the repository settings.

In a report, say what the problem is, which version of strata you found it in,
and the steps that show it.

## What strata reaches

strata reads the Palette API. Every request it makes is a GET, so it changes no
profile, no cluster and no pack. It writes the records and the reports that you
ask for, and nothing else.

The CI scripts that a release carries do write, but they write to your forge and
never to Palette. `pr-comment.sh` posts a comment and sets a label.
`verify-report.sh` opens and closes issues. Give the token that you hand to
these scripts the smallest access that does this work.

## Your API key

strata reads the Palette API key from the environment first, and then from
`~/.palette.env`. The key is the one secret that strata holds.

The `Debug` of `Config` prints `<redacted>` in place of the key, so a log line or
a crash report does not carry it. A record and a report never hold the key.

Give the key the smallest access that reads the profiles and the clusters that
you check.

## Releases

Each release carries a `SHA256SUMS` file, and a Sigstore signature for each
file.

The checksums show that a file arrived whole. They do not show where the file
came from, because the same release serves the file and the checksums. The
signature shows that the release workflow of this repository made the file.
[Install](docs/src/install.md) gives the command that checks a signature.
