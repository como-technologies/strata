#!/usr/bin/env bash
# Set the version of this repository: scripts/dev/set-version.sh <version>
#
# The version is in Cargo.toml, and it is also in the README badge, the install commands, the CI
# drop-ins, and the pages that show them. One command moves all of them and commits the move. So a
# release is a version and a tag, not an edit of a dozen files:
#
#   just version 1.2.3     # rewrites and commits
#   just release           # tags the commit
#
# The outputs of the book print the version in some places, such as the column "strata" of
# `strata report`. A build of the new version writes them, so the command runs `just docs-examples`
# and commits the outputs that change. The release commit then passes `just docs-check` without a
# manual step. It refuses to commit when an output that did not name the old version changes,
# because the book was not current before this command.
#
# It refuses when a file it must rewrite already has uncommitted changes, so the commit it writes
# is the version and nothing else.
#
# It rewrites the version only where the version is meant to follow this repository. These places
# are the manifest, the badge, an install command, a pinned workflow ref, and the variables the
# drop-ins set. Prose that names a version names it for a reason. The sentence "use v0.5.2 or
# later: it is the first release with all four scripts" is a fact about v0.5.2. It is not a fact
# about the version now. So free text is never rewritten, whatever version it holds.
#
# Recorded evidence is never touched, because it says which build produced a result:
#   examples/live/runs/                   the raw output of each live run
#   docs/src/development/live-runs.md     the page that reports those runs
#   docs/book/                            the built book
#
# `just test` checks the result: the CI drop-ins and the README badge must name the version in
# Cargo.toml.
#
# Write an example of this command with a version that this repository never has. Otherwise a run
# rewrites the example too: `just version 1.2.3`, not `just version <the version now>`.
set -euo pipefail

here=$(cd "$(dirname "$0")/../.." && pwd)
cd "$here"

new=${1:-}
case "$new" in
  "" | -h | --help)
    sed -n '2,/^set -euo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'
    exit 2
    ;;
esac
case "$new" in
  [0-9]*.[0-9]*.[0-9]*) ;;
  *) echo "set-version: '$new' is not a version of the form X.Y.Z" >&2; exit 2 ;;
esac

old=$(sed -n '/^version = /{s/^version = "\(.*\)"$/\1/p;q;}' Cargo.toml)
[ -n "$old" ] || { echo "set-version: no version line in Cargo.toml" >&2; exit 2; }
if [ "$old" = "$new" ]; then
  echo "set-version: already at $new"
  exit 0
fi

# every tracked file that names the old version, less the recorded runs, the built book, and the
# outputs of the book. A build of the new version writes the outputs below
mapfile -t files < <(git grep -l -F "$old" -- \
  ':!examples/live/runs' \
  ':!docs/src/development/live-runs.md' \
  ':!docs/book' \
  ':!docs/src/output' \
  ':!Cargo.lock' || true)
# the outputs of the book that name the old version
mapfile -t outputs < <(git grep -l -F "$old" -- docs/src/output || true)

# the commit must hold the version and nothing else
dirty=()
checked=("${files[@]}" Cargo.lock)
[ ${#outputs[@]} -eq 0 ] || checked+=(docs/src/output)
for f in "${checked[@]}"; do
  git diff --quiet -- "$f" && git diff --cached --quiet -- "$f" || dirty+=("$f")
done
if [ ${#dirty[@]} -gt 0 ]; then
  echo "set-version: these hold uncommitted changes, and this command must rewrite them:" >&2
  printf '  %s\n' "${dirty[@]}" >&2
  echo "set-version: commit or stash them first" >&2
  exit 1
fi

# the contexts the version follows, and nothing else
for f in "${files[@]}"; do
  before=$(cat "$f")
  sed -i \
    -e "s/^version = \"${old}\"$/version = \"${new}\"/" \
    -e "s|badge/version-${old}-blue|badge/version-${new}-blue|g" \
    -e "s/v=v${old}\b/v=v${new}/g" \
    -e "s/--version v${old}\b/--version v${new}/g" \
    -e "s/@v${old}\b/@v${new}/g" \
    -e "s/strata-version: v${old}\b/strata-version: v${new}/g" \
    -e "s/^\( *\)STRATA_VERSION: v${old}\b/\1STRATA_VERSION: v${new}/g" \
    -e "s/tag = \"v${old}\"/tag = \"v${new}\"/g" \
    "$f"
  if [ "$before" = "$(cat "$f")" ]; then
    echo "set-version: $f names $old, and in no context this command rewrites; read it" >&2
  else
    echo "set-version: $f"
  fi
done

# Cargo.lock carries the version of each crate of the workspace; cargo writes it
if ! cargo update --offline --workspace >/dev/null 2>&1; then
  echo "set-version: cargo cannot update Cargo.lock; nothing is committed. See: cargo update --offline --workspace" >&2
  exit 1
fi

# The outputs of the book print the version of strata. A build of the new version writes them
# again. Only an output that named the old version can change. Another change means that the book
# was not current before this command, and the release commit must not hide that.
added=("${files[@]}" Cargo.lock)
if [ ${#outputs[@]} -gt 0 ]; then
  if ! just docs-examples; then
    echo "set-version: just docs-examples failed; nothing is committed" >&2
    exit 1
  fi
  stale=()
  while IFS= read -r f; do
    case " ${outputs[*]} " in *" $f "*) ;; *) stale+=("$f") ;; esac
  done < <(git status --porcelain -- docs/src/output | cut -c4-)
  if [ ${#stale[@]} -gt 0 ]; then
    echo "set-version: these outputs of the book changed, and they did not name $old:" >&2
    printf '  %s\n' "${stale[@]}" >&2
    echo "set-version: nothing is committed, and the files above stay rewritten. Restore them, run just docs-examples, commit the outputs, and run this command again" >&2
    exit 1
  fi
  added+=(docs/src/output)
fi

git add -- "${added[@]}"
git commit -q -m "chore: release $new"
echo "set-version: $old -> $new in ${#files[@]} files, committed as 'chore: release $new'"
echo "set-version: tag it with: just release" >&2
