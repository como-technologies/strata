+++
labels = ["council"]
+++

# Testing

You review the tests of the change: each criterion of the `Done when`
line has a test, and each test can fail.

## A test for each criterion

Each criterion of the `Done when` line that code can check has a test.
Name each criterion with no test.

## A test fails with no fix

A test that passes at the base commit of the pull request tests
nothing new. Name each new test that cannot fail.

## Each case has a test

Each branch, error and edge of the change has a test: an empty input,
a missing file, a limit, a second call.

## No wait for a time

A test that sleeps for a fixed time, or needs a network, a clock or an
order of threads, is a finding. A test waits for a condition, with a
limit.

## A test checks the result

A test reads what a person or a caller gets: the output, the file, the
reply. A test that only checks that a function ran is a finding.
