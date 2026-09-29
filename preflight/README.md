# strata-preflight

Check that Spectro Cloud Palette can build a cluster, before the build. The
input is the body of the create, or a cluster as Palette holds it now. The
output is one line for each rule of a catalog: what will stop the build, found
in seconds, with the command that shows it by hand.

```bash
strata-preflight cluster.json --cloud eks           # the body of POST /v1/spectroclusters/eks
strata-preflight cluster:UID                        # an existing cluster
strata-preflight cluster:UID change.json            # a change of its pools or profiles
strata-preflight --watch cluster:UID                # during the build
strata-preflight --rules --cloud gke                # the rules
```

The book has the command page, the use case, the rule format, and how the
confidence of each rule is counted:
<https://legendary-train-pzq9ojj.pages.github.io/commands/preflight.html>.

`strata preflight ...` runs this binary when both are installed. Another
repository depends on the crate by its git address:

```toml
strata-preflight = { git = "ssh://git@github.com/como-technologies/strata.git", tag = "v0.16.0" }
```
