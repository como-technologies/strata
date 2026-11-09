# Quick start

## Without a tenant

The repository holds real pack data from the Public Repo registry. In a
checkout, with Rust and `just`, this builds three versions of the profile
`aws-stack` from that data, with no network:

```bash
just examples
```

Then compare two of the versions it built:

```text
{{#include output/quickstart-offline.txt}}
```

The add-on `cert-manager` has a new manifest, so its line shows `AUTHORED`. The
version also adds a required variable. The impact is `in-place`, so Palette
replaces no node.

## With a tenant

These steps use the tenant of this book. Its project `dev` holds the versions
1.0.0, 1.1.0, and 1.1.1 of `aws-stack`. Its cluster `payments` runs 1.1.0 and
three add-on profiles, as many clusters do. With your own tenant, put your project, profile, and cluster names in place of
these. `strata list` prints yours.

<div class="playground-demo">

`just playground quickstart` serves that tenant on this machine, with no key and no cost.

</div>

```bash
export PALETTE_PROJECT=dev
```

### 1. Find the refs

```text
{{#include output/quickstart-list.txt}}
```

The first column is a ref. `compare`, `snapshot`, `same-release` and
`consumers` read one. `verify` reads a record file, which step 2 writes. See
[Refs](./reference/refs.md).

### 2. Record a release

```text
{{#include output/quickstart-snapshot.txt}}
```

Keep the file with the release: in a repository, for example. The root
`8a8a64d3f95d` identifies the release.

### 3. Make sure that Palette still has it

```text
{{#include output/quickstart-verify.txt}}
```

Exit `0`. `DRIFT` and exit `1` tell you that the published version changed after
you recorded it. [What Palette does](./internals/palette/profiles.md#edits-in-place)
names who can change it. Run this check on a schedule:
[A published version is edited in place](./use-cases/edited-in-place.md).

### 4. Make sure that a cluster runs it

```text
{{#include output/quickstart-cluster.txt}}
```

`IDENTICAL`: the cluster runs the recorded release. The record holds
`aws-stack` alone, so the cluster side holds `aws-stack` alone, and not the
three add-on profiles. See
[Compare a profile version with a cluster](./commands/compare/clusters.md#compare-a-profile-version-with-a-cluster).
A cluster has no consumers, so the `consumers` line gives the profile side
alone. Palette lists `payments` as one of the clusters that use the version.

### 5. See what the next version changes

```text
{{#include output/quickstart-versions.txt}}
```

Exit `1`: the versions differ. The report names each changed layer and the
impact on running clusters.

## Next

- [The everyday loop](./loop.md): when to use strata, how to add it to your
  pipeline one job at a time, and what a normal day holds.
- [Use cases](./use-cases/index.md): a problem, the commands, and the fix.
- [Commands](./commands/index.md): each command, its output, and its options.
- [The observer](./automation/observer.md): the same checks on a timer in one cluster, with a page.
- [Palette gives a value that strata does not know](./use-cases/unknown-value.md): what to do about a
  note of a value that this release does not know. It also shows how to tell the maintainers: by
  hand, with the setting `STRATA_REPORT`, or with a yes to the one question.
