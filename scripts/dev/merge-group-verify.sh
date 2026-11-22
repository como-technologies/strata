#!/usr/bin/env bash
# Set the status riff/verify on the commit of a merge group, from the status riff/verify of the head
# commit of its pull request. The merge queue of main runs the required checks again on the merge
# group commit. The verify is a status of the head commit only. The job `verify of the merge group`
# of the ci workflow runs this script. It starts no verify. "The merge" in
# docs/src/development/changes.md says why.
#
# Usage: merge-group-verify.sh HEAD_REF GROUP_SHA
#
# HEAD_REF is the branch of the merge group, such as refs/heads/gh-readonly-queue/main/pr-12-<sha>.
# Its last part names the pull request. GROUP_SHA is the merge group commit. GH_REPO names the
# repository as OWNER/REPO, and gh signs in with GH_TOKEN, which can write a status.
#
# When the status of the head commit is success, the script sets success on GROUP_SHA, with the link
# of the verify, and exits with 0. Else it sets failure, and exits with 1, so the queue removes the
# pull request. A wrong use sets no status, and exits with 2.
set -euo pipefail

context=riff/verify

die() {
  echo "merge-group-verify: $*" >&2
  exit 2
}

head_ref=${1-}
group_sha=${2-}
[ -n "${GH_REPO-}" ] || die "GH_REPO names no repository"
# The branch of each entry of the queue names its own pull request. The group of an entry also
# holds the entries before it, and each of them has a group of its own.
if ! [[ $head_ref =~ /pr-([0-9]+)-[0-9a-f]+$ ]] || [ -z "$group_sha" ]; then
  die "give the branch of a merge group and its commit, not \"$head_ref\" and \"$group_sha\""
fi
pull=${BASH_REMATCH[1]}

# Set the status $1 on the merge group commit, with the description $2, and the link $3 when it has
# one.
set_status() {
  local link=()
  [ -z "${3-}" ] || link=(-f "target_url=$3")
  gh api -X POST "repos/$GH_REPO/statuses/$group_sha" -f "state=$1" -f "context=$context" \
    -f "description=$2" ${link[@]+"${link[@]}"} > /dev/null
}

head=$(gh api "repos/$GH_REPO/pulls/$pull" --jq .head.sha)
# The combined status gives the newest status of each context. A head with none gives a tab alone.
found=$(gh api "repos/$GH_REPO/commits/$head/status" \
  --jq "[.statuses[] | select(.context == \"$context\")][0] | [.state // \"\", .target_url // \"\"] | @tsv")
state=${found%%$'\t'*}
link=${found#*$'\t'}

if [ "$state" = success ]; then
  set_status success "The verify of pull request #$pull passed at ${head:0:7}" "$link"
  echo "merge-group-verify: $context is success on $group_sha: pull request #$pull passed its verify at $head."
else
  set_status failure "Pull request #$pull has no passed verify at ${head:0:7}"
  echo "merge-group-verify: pull request #$pull has no passed $context at $head (state: ${state:-none}), so $context is failure on $group_sha." >&2
  exit 1
fi
