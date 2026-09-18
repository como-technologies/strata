# Release

1. Set the new version, and commit:

   ```bash
   just version <version>   # Cargo.toml, the README badge, the install commands, the drop-ins
   ```

   The version is written in more than a dozen files. The recipe moves all of
   them, and leaves the recorded live runs alone, because each one says which
   build produced it. `just test` fails when a file is left behind.
2. Tag the commit:

   ```bash
   just release        # tags HEAD as v<version>
   ```

   The recipe refuses a dirty tree or a tag that exists.
3. Push the tag.

The release workflow then:

1. builds the four binaries
2. writes `SHA256SUMS`
3. signs each file with cosign
4. checks the signatures
5. publishes the GitHub release with generated notes

Users install it as [Install](../install.md) shows. The version rules are in
[Versions and compatibility](../reference/compatibility.md).
