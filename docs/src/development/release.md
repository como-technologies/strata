# Release

1. Set the new version. The recipe writes it and commits the change as
   `chore: release <version>`:

   ```bash
   just version <version>
   ```

   The version is in `Cargo.toml` and `Cargo.lock`, the README badge and its
   install command, the install page, the CI drop-ins, the chart of the observer
   and its `helm install` command, and the pages that show them. The recipe rewrites the version only where the version
   follows this repository. Prose that names a release for a reason keeps it:
   "use v0.5.2 or later, the first release with `merge-gate.sh`" is a fact
   about v0.5.2. The recipe leaves the recorded live runs alone for the same
   reason, because each one says which build produced it. It refuses when a file
   it must rewrite already holds uncommitted changes, so the commit is the
   version and nothing else.

   Some outputs of the book print the version of strata, such as the column
   `strata` of `strata report`. The recipe builds the new version, runs
   `just docs-examples`, and adds the outputs that change to the commit. So
   `just docs-check` passes on the release commit without a manual step. The
   step needs `jq`.

   The recipe commits nothing when an output that does not name the old version
   changes. The book was not current before the release in that case. Restore
   the files that the recipe rewrote. Run `just docs-examples` and commit the
   outputs. Then run the recipe again.
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
7. at the same time as step 6, builds the image of the observer from the signed
   binaries of the release, signs its digest, and pushes it as
   `ghcr.io/como-technologies/strata-observer:<tag>`, then packages the chart
   of the observer, pushes it to `oci://ghcr.io/como-technologies/charts`, and signs
   its digest

The transparency log of Sigstore can refuse one try of step 4. So the script
`scripts/ci/sign-blobs.sh` tries each file up to 5 times, with 30 seconds
between two tries. It removes a partial bundle before each try. After the last
try of a file, it stops with the last error of cosign, and it does not sign the
next file. The step that checks each signature stays after it.

The job of step 7 calls the workflow `observer-image.yml`: see [The
image](./observer-deploy.md#the-image). A fault in the `permissions` of that job
stops the whole run at its start, so change them with care. If the job fails after
the release is published, the release stays. Start the workflow by hand to build
the image again.

Users install it as [Install](../install.md) shows. The version rules are in
[Versions and compatibility](../reference/compatibility.md).
