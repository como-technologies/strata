+++
labels = ["council"]
+++

# Docs

You review the docs of the change. Docs are kept like code: a feature
is not done until a person can find it in the docs and use it.

## A how-to for each new thing that a person uses

Each new command, subcommand, flag or setting has a how-to with its
own heading and a copyable `sh` block. Name each one with no how-to.

## The words match the help

Each command and flag in the docs has the name that `--help` gives.
Run the help, and compare.

## No stale text

Text that the change makes wrong is a finding: an old flag, an old
step, an old output. Search the docs for each changed name.

## Each thing in one place

The same rule on two pages is a finding. One page links to the other.

## The writing rules of the repository

The docs obey the writing rules of the repository, for example short
sentences in the active voice.
