# Release

1. Set the new version. The recipe writes it and commits the change as
   `chore: release <version>`:

   ```bash
   just version <version>
   ```

   The version is in ten files: `Cargo.toml` and `Cargo.lock`, the README badge
   and its install command, the install page, the CI drop-ins, and the pages
   that show them. The recipe rewrites the version only where the version
   follows this repository, so prose that names a release for a reason keeps it:
   "use v0.5.2 or later, the first release with all four scripts" is a fact
   about v0.5.2. It leaves the recorded live runs alone for the same reason,
   because each one says which build produced it. It refuses when a file it must
   rewrite already holds uncommitted changes, so the commit is the version and
   nothing else.
2. Tag the commit:

   ```bash
   just release        # tags HEAD as v<version>
   ```

   The recipe refuses a dirty tree or a tag that exists.
3. Push the commit and the tag. The tag starts the release workflow:

   ```bash
   git push origin main
   git push origin v<version>
   ```

The release workflow then:

1. builds the four binaries
2. writes `SHA256SUMS`
3. signs each file with cosign
4. checks the signatures
5. publishes the GitHub release with generated notes

Users install it as [Install](../install.md) shows. The version rules are in
[Versions and compatibility](../reference/compatibility.md).
