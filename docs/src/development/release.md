# Release

1. Set the new version. The recipe writes it and commits the change as
   `chore: release <version>`:

   ```bash
   just version <version>
   ```

   The version is in `Cargo.toml` and `Cargo.lock`, the README badge and its
   install command, the install page, the CI drop-ins, and the pages that show
   them. The recipe rewrites the version only where the version
   follows this repository. Prose that names a release for a reason keeps it:
   "use v0.5.2 or later, the first release with all four scripts" is a fact
   about v0.5.2. The recipe leaves the recorded live runs alone for the same
   reason, because each one says which build produced it. It refuses when a file
   it must rewrite already holds uncommitted changes, so the commit is the
   version and nothing else.
2. Tag the commit:

   ```bash
   just release        # tags HEAD as v<version>
   ```

   The recipe refuses a dirty tree or a tag that exists.
3. Push the commit and the tag. The tag starts the release workflow. A push to
   `main` with no pull request needs an admin of the repository: see
   [How a change lands](./changes.md#the-merge).

   ```bash
   git push origin main
   git push origin v<version>
   ```

The release workflow then:

1. runs every check of `checks.yml`, so a tag cannot ship what CI refuses
2. builds the four binaries
3. copies the CI scripts and the installer beside them, and writes `SHA256SUMS`
   over all of them
4. signs each file with cosign, and checks the signatures
5. publishes the GitHub release with generated notes
6. installs the published release with `install-strata.sh --require-signature`,
   which checks the signature of `SHA256SUMS`, and runs each script it installed

Users install it as [Install](../install.md) shows. The version rules are in
[Versions and compatibility](../reference/compatibility.md).
