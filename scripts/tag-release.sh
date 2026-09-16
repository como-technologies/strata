#!/usr/bin/env bash
# Tag the version in Cargo.toml as v<version>, as an annotated tag on HEAD. Refuses when the tree
# has uncommitted changes or the tag exists. Prints the tag. Pushing it is a separate step, and
# pushing it is what starts the release workflow.
set -euo pipefail

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)
[ -n "$version" ] || { echo "tag-release: no version line in Cargo.toml" >&2; exit 2; }
tag="v$version"

if [ -n "$(git status --porcelain)" ]; then
  echo "tag-release: the tree has uncommitted changes; commit or stash them first" >&2
  exit 1
fi
if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
  echo "tag-release: $tag already exists; bump the version in Cargo.toml first" >&2
  exit 1
fi

git tag -a "$tag" -m "$tag"
echo "$tag"
echo "tag-release: push it with: git push origin $tag" >&2
