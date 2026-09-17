# Quick start

## Without a tenant

The repository holds real pack data from the Public Repo registry. In a
checkout, with Rust and `just`, this builds three versions of the profile
`aws-stack` from that data, with no network:

```bash
just examples
```

```text
{{#include output/quickstart-offline.txt}}
```

The add-on `cert-manager` has a new manifest, so its line shows `AUTHORED`. The
version also adds a required variable. The impact is `in-place`, so Palette
replaces no node.

## With a tenant

The project `dev` holds the versions 1.0.0, 1.1.0, and 1.1.1 of `aws-stack`. The
cluster `payments` runs 1.1.0.

```bash
export PALETTE_PROJECT=dev
```

### 1. Find the refs

```text
{{#include output/quickstart-list.txt}}
```

The first column is a ref. Each command accepts it.

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

Exit `0`. `DRIFT` and exit `1` tell you that a person edited the version after
you recorded it. Run this check on a schedule:
[A published version is edited in place](./use-cases/edited-in-place.md).

### 4. Make sure that a cluster runs it

```text
{{#include output/quickstart-cluster.txt}}
```

`IDENTICAL`: the cluster runs the recorded release.

### 5. See what the next version changes

```text
{{#include output/quickstart-versions.txt}}
```

Exit `1`: the versions differ. The report names each changed layer and the
impact on running clusters.

## Next

- [Use cases](./use-cases/index.md): a problem, the commands, and the fix.
- [Commands](./commands/index.md): each command, its output, and its options.
