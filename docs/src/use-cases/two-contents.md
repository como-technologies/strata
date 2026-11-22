# One version label holds two contents

## The problem

The platform team publishes `aws-stack@1.1.1` at tenant scope, so each project
lists it. The team of `prod` made a copy of the version in its own project. The
copy has the same name and the same version, and one changed value. The cluster
`payments` of prod runs the copy.

A name and a version are a label, not an identity. Here one label holds two
contents, and the label does not show it. A field test found copies of this
kind in one project: some of them held other content, and clusters ran them.

**Who runs this.** A person at a terminal who asks if dev and prod have the
same release, and a job that checks each project.

<div class="playground-demo">

`just playground two-contents` serves this tenant on this machine, with no key and no cost.

</div>

## 1. See the two versions

```text
{{#include ../output/two-contents-list.txt}}
```

The listing of prod holds two versions `aws-stack@1.1.1`, with two uids. The
mark `tenant` names the version of the tenant, and the other row is the copy of
prod. See [A profile of the tenant](../commands/list.md#a-profile-of-the-tenant).

## 2. Do dev and prod have the same release?

Each ref names its project. In prod, the label names two versions, so strata
refuses it:

```text
{{#include ../output/two-contents-ambiguous.txt}}
```

Exit `2`. The error names the uid and the scope of each version. Give the uid
of the copy, in its project:

```text
{{#include ../output/two-contents-different.txt}}
```

Exit `1`: one label, two roots. Dev has no copy, so the ref of dev reads the
version of the tenant. The reason names the pack and the value path.

## 3. What does the copy change?

```text
{{#include ../output/two-contents-compare.txt}}
```

The copy keeps the volumes after a delete: `Retain` in place of `Delete`.
`compare` shows what differs. It does not show who made the copy, or why.

## 4. Which clusters run the copy?

```text
{{#include ../output/two-contents-consumers.txt}}
```

Exit `1`: the cluster `payments` of prod runs the copy. So a delete of the copy
must wait until each cluster of the copy runs another version.

## The fix

One label holds one content. Decide which content is the release:

- When prod does not need the change, move each cluster of the copy to the
  version of the tenant.
- When prod needs the change, publish it as a new version of the tenant, such
  as `aws-stack@1.1.2`. Then move each cluster of the copy to it.

<div class="playground-demo">

`just playground two-contents-2` serves the tenant after the team of prod moved
`payments` to the version of the tenant.

</div>

The cluster now runs the release of dev:

```text
{{#include ../output/two-contents-fixed.txt}}
```

`IDENTICAL`: the cluster holds the root of the version of the tenant. No
cluster runs the copy now:

```text
{{#include ../output/two-contents-free.txt}}
```

Exit `0`: the copy has no consumers. Delete it in Palette, so the label of
prod names one version again. Strata deletes nothing. See
[Retire a version](./retire.md).

## What to do

- Give each content its own version. A copy with the name and the version of a
  profile of the tenant puts a difference behind one label.
- Before a publish in a project, run
  `strata list profiles --project-name <project>`. A row with the mark `tenant`
  and the same label tells you to use another version.
- When the label of a project names two versions, give the uid in the ref:
  `project:prod/<uid>`. See
  [The same names in two projects](./same-names.md).
- Run `strata same-release` with a ref in each project in the promotion gate.
  Then a label with two contents fails the gate: see
  [Promote a release to another project](./promote.md).
- `strata list profiles --json` gives the ref, the uid, and the scope of each
  version. A script can find each label with two scopes in it.
- A project lists the version of the tenant beside its own version, with one
  name and one version. [`scope.sh`](../development/live-runs.md#scopesh) shows
  it on a live tenant: see
  [The scope of a version](../internals/palette/profiles.md#the-scope-of-a-version).
