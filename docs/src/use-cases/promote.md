# Promote a release to another project

## The problem

Each Palette project holds its own copy of a profile version, with its own uid.
The projects `dev` and `prod` both have `aws-stack@1.1.0` and `aws-stack@1.1.1`.
The name and the version are the same in both projects, and nothing in Palette
makes the content the same.

A promotion moves one release along the environments, so each project should
hold the content that dev approved. Here `1.1.0` reached prod that way. Somebody
built prod's `1.1.1` by hand instead, and changed one value on the way.

**Who runs this.** The publish job records each project, and the promotion gate
checks the pairs: see
[The promotion gate and the retire gate](../automation/gates.md).

**Follow along.** `just playground promote` serves this tenant on this machine,
with no key and no cost.

## 1. Record each project's copy of each version

```text
{{#include ../output/promote-record.txt}}
```

The four roots say what the check will say. The `1.1.0` pair has one root, and
the `1.1.1` pair has two.

## 2. Check each promoted version

A promotion gate asks the same question of every version it promoted. The
`1.1.0` pair passes:

```text
{{#include ../output/promote-same.txt}}
```

`IDENTICAL`: prod holds the content that dev approved. The uids differ, and the
roots do not, because the root covers the content and not the uid.

The `1.1.1` pair does not:

```text
{{#include ../output/promote-different.txt}}
```

Exit `1`. The reason names the pack and the value path.

## 3. See what differs

```text
{{#include ../output/promote-compare.txt}}
```

The prod copy keeps its volumes after a delete: `Retain` in place of `Delete`.
The impact is `cluster-wide`.

## 4. Correct the copy that was built by hand

Do not correct it by editing prod's `1.1.1`. An edit keeps the version number
and the uid, and changes what they mean, so every record of `aws-stack@1.1.1` in
prod becomes a record of something that no longer exists. That is
[A published version is edited in place](./edited-in-place.md).

Promote `1.1.1` to prod from the source that dev came from, the way `1.1.0`
reached prod. Prod then holds one copy of one release, under the number the
release already has. Strata does not publish: use the Spectro Cloud provider,
the Palette API, or the console.

The copy that differs has to go, and Palette refuses to delete a version that a
cluster runs. Check before you delete:

```text
{{#include ../output/promote-consumers.txt}}
```

`no consumers` and exit `0`: nothing runs prod's copy, so the delete is safe,
and the promotion can take the number `1.1.1`.

Exit `1` lists each cluster that runs the copy. Palette will not delete it, so
the order changes: promote the approved content under the next free number,
move those clusters to it, and delete the old copy last. See
[Retire a version](./retire.md).

After the promotion, step 2 answers `IDENTICAL` for the `1.1.1` pair as well.

## What to do

- Publish each project from one source: a candidate file, or one Terraform
  module. A copy built by hand in one project is what this page is about, and
  no check prevents it. The check finds it afterwards.
- Run `same-release` on each promoted version, before a cluster moves. Give the
  records in promotion order, `dev.json prod.json`, and add a record for each
  project that a release passes through. To run it as a job, see
  [The promotion gate and the retire gate](../automation/gates.md).

A pair can also differ for a reason that is not a content change:

- The prod copy takes its packs from another registry:
  [A pack comes from another registry](./registry-move.md).
- The prod copy pins a pack under another tag name:
  [One pack build under two tag names](./tag-name.md).
- The profile has a manifest pack: [One manifest in two profiles](./manifest-pack.md).
