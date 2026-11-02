#!/usr/bin/env bash
# The upstream test (#79): `strata behind` against the registries, and scripts/ci/upstream-report.sh
# around a real arrival: a version that the example pushes into its own OCI pack registry between
# two runs.
#
#   just live-profiles && just live-upstream
#
#   0  an empty registry
#   1  push 1.0.0 and 1.1.0 of the example's own pack: a record of 1.0.0, a record of csi-aws-ebs
#      1.63.1 from the Public Repo, and a record of the release version. strata behind names the
#      newer versions of each pin and the impact of each bump
#   2  upstream-report.sh opens one issue for each pack that is behind
#   3  push 1.2.0: strata behind --since the first run names 1.2.0 as arrived, and the second run
#      of upstream-report.sh names it in the issue, from the state that the issue keeps
#   4  records at the newest versions: upstream-report.sh closes each issue; then the registry is
#      emptied again
#
# Needs the profiles and the registry of the example (just live-profiles), the strata release
# binary, aws with the profile terraform.tfvars names, oras, jq, tar, and gh logged in to this
# repository: the issues are this repository's, with the label strata-live-behind. Results go under
# runs/<UTC time>/upstream/, or OUT. Exit 0 only when no check fails.
set -uo pipefail
script=upstream
flag=false
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
. "$live/github.sh"
[ -x "$strata" ] || die "no strata binary at $strata; run just build"
for tool in aws oras jq tar gh; do
  command -v "$tool" > /dev/null || die "$tool is not installed"
done

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/upstream}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

load_outputs
REG=$(out owned_registry_uid)
REG_NAME=$(out owned_registry_name)
HOST=$(out owned_registry_host)
REPO=$(out owned_repository)
PACK=$(out owned_pack_name)
REGION=$(out region)
R=$(out release_profile_uid)
AWS_PROFILE=$(out aws_profile)
PALETTE_PROJECT_UID=$(out project_uid)
export AWS_PROFILE PALETTE_PROJECT_UID
for v in REG REG_NAME HOST REPO PACK REGION R AWS_PROFILE PALETTE_PROJECT_UID; do
  [ -n "${!v}" ] || die "the tofu output behind $v is empty; run just live-profiles"
done
GH_REPO=$(remote_repo "$live")
[ -n "$GH_REPO" ] || die "the remote origin of $live is not on GitHub, and the issues of this test are on GitHub"
LABEL=strata-live-behind
report=("$live/../../scripts/ci/upstream-report.sh")
export STRATA_BIN="$strata"

summary() {
  write_summary "upstream test" ", registry \`$REG_NAME\`," \
    "pack \`$PACK\`, issues of \`$GH_REPO\` with the label \`$LABEL\`."
}
trap summary EXIT

# build VERSION: PACK-VERSION.tar.gz from the pack source, with pack.json at VERSION
build() {
  local dir="pack-$1"
  rm -rf "$dir" && mkdir -p "$dir" && cp -r "$live/pack/$PACK" "$dir/$PACK" || return 2
  jq --arg v "$1" '.version = $v' "$live/pack/$PACK/pack.json" > "$dir/$PACK/pack.json" || return 2
  tar -czf "$PACK-$1.tar.gz" -C "$dir" "$PACK"
}
push() { oras push "$REPO:$1" "$PACK-$1.tar.gz" > "oras-$1.out" 2>&1; }
# empty: remove each image of the repository, and have Palette read the registry again
empty() {
  local repository=${REPO#*/} images
  images=$(aws ecr list-images --region "$REGION" --repository-name "$repository" --query 'imageIds[*]' --output json) || return 2
  if [ "$(jq length <<<"$images")" != 0 ]; then
    echo "$images" > images.json
    aws ecr batch-delete-image --region "$REGION" --repository-name "$repository" --image-ids file://images.json > /dev/null || return 2
  fi
  "$live/api.sh" sync-oci "$REG"
}
# pin FILE NAME: the pin of pack NAME in a strata behind --json in FILE
pin() { jq -c --arg n "$2" '[.pins[] | select(.name == $n)] | first' "$1"; }
# issues N: the open issues of this test, as [{number, title, body}], once GitHub lists N of them:
# it lists a new or closed issue some seconds later. Up to a minute.
issues() {
  local i list
  for i in $(seq 1 12); do
    list=$(gh issue list --repo "$GH_REPO" --label "$LABEL" --state open --json number,title,body) || return 2
    [ "$(jq length <<<"$list")" = "$1" ] && break
    sleep 5
  done
  printf '%s\n' "$list"
}

aws ecr get-login-password --region "$REGION" | oras login --username AWS --password-stdin "$HOST" > login.out 2>&1 \
  || die "oras login to $HOST failed: $(cat login.out)"

echo "== 0. an empty registry" | tee -a steps.log
check empty 0 "the repository of $REG_NAME is empty, and Palette read it again" -- empty
check no-issues 0 "no open issue has the label $LABEL" -- bash -c "[ \"\$(gh issue list --repo $GH_REPO --label $LABEL --state open --json number | jq length)\" = 0 ]"

echo "== 1. two versions of the pack, and strata behind" | tee -a steps.log
for v in 1.0.0 1.1.0; do
  build "$v" || die "building $PACK $v failed"
  check "push-$v" 0 "push $PACK $v into $REG_NAME" -- push "$v"
done
check sync-1 0 "Palette reads $REG_NAME again" -- "$live/api.sh" sync-oci "$REG"
mkdir -p records
check record-own 0 "record $PACK 1.0.0" -- "$strata" snapshot "pack:$REG_NAME/$PACK@1.0.0" -o records/own.json
check record-csi 0 'record csi-aws-ebs 1.63.1 of the Public Repo' -- "$strata" snapshot "pack:$public_repo/csi-aws-ebs@1.63.1" -o records/csi.json
check record-release 0 'record the release version' -- "$strata" snapshot "$R" -o release.json
check behind-1 1 '#79: strata behind names the pins that are behind: exit 1' -- \
  "$strata" behind records/own.json records/csi.json release.json --json
cp behind-1.out run-1.json
check behind-schema 0 '#79: the document is strata-behind/1' -- jq -e '.schema == "strata-behind/1"' run-1.json
check behind-own 0 "#79: $PACK 1.0.0 in $REG_NAME is behind 1.1.0, with the impact of the bump" -- \
  bash -c "jq -e '[.newer[].version] == [\"1.1.0\"] and .behind and (.bump.to == \"1.1.0\") and (.bump.impact | length > 0)' <<<'$(pin run-1.json "$PACK")'"
check behind-csi 0 '#79: csi-aws-ebs 1.63.1 is behind the newer versions of the Public Repo, and the bump names its flags' -- \
  bash -c "jq -e '.behind and ([.newer[].version] | index(\"1.65.0\")) and (.bump.flags | index(\"IDENTITY\"))' <<<'$(pin run-1.json csi-aws-ebs)'"
check behind-release 0 '#79: each pin of the release version is checked: no error' -- \
  jq -e '[.pins[] | select(.pinnedBy | any(.path == "release.json"))] | length > 0 and all(.error == null)' run-1.json
check behind-says 0 '#79: the report names each pin that is behind with its newer versions' -- \
  bash -c "\"$strata\" behind records/own.json records/csi.json --no-impact | grep -Eq '^~ \\[addon\\] $PACK +1.0.0 +$REG_NAME +behind: 1.1.0$'"

echo "== 2. the upstream report opens an issue for each pack behind" | tee -a steps.log
check report-1 1 '#79: upstream-report.sh opens one issue for each pack behind: exit 1' -- \
  "${report[@]}" records/own.json records/csi.json --repo "$GH_REPO" --label "$LABEL"
issues 2 > issues-1.json
check report-1-issues 0 "#79: one open issue for $PACK, and one for csi-aws-ebs" -- \
  jq -e --arg p "$PACK" 'length == 2 and any(.title | startswith("strata behind: " + $p + " 1.0.0 behind 1.1.0")) and any(.title | startswith("strata behind: csi-aws-ebs 1.63.1 behind"))' issues-1.json
check report-1-first 0 '#79: each issue says that this run is the first to find the pack behind' -- \
  jq -e 'length == 2 and all(.body | contains("this run is the first to find it behind"))' issues-1.json

echo "== 3. a version arrives" | tee -a steps.log
build 1.2.0 || die "building $PACK 1.2.0 failed"
pushed_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
check push-1.2.0 0 "push $PACK 1.2.0 into $REG_NAME" -- push 1.2.0
check sync-3 0 "Palette reads $REG_NAME again" -- "$live/api.sh" sync-oci "$REG"
check behind-since 1 '#79: strata behind --since the first run' -- \
  "$strata" behind records/own.json records/csi.json --since run-1.json --no-impact --json
check behind-arrived 0 "#79: 1.2.0 of $PACK arrived since the first run, and nothing of csi-aws-ebs" -- \
  bash -c "jq -e --arg p '$PACK' '([.pins[] | select(.name == \$p) | .arrived] == [[\"1.2.0\"]]) and ([.pins[] | select(.name == \"csi-aws-ebs\") | .arrived] == [[]]) and .summary.arrived == 1' behind-since.out"
check behind-listed 0 "#79: Palette lists 1.2.0 with the time it first listed it, after the push" -- \
  jq -e --arg p "$PACK" --arg t "$pushed_at" '[.pins[] | select(.name == $p) | .newer[] | select(.version == "1.2.0") | .listed]
    | length == 1 and ((.[0] | sub("\\.[0-9]+Z$"; "Z") | fromdateiso8601) >= ($t | fromdateiso8601))' behind-since.out
check report-2 1 '#79: the second run of upstream-report.sh updates the issues' -- \
  "${report[@]}" records/own.json records/csi.json --repo "$GH_REPO" --label "$LABEL"
issues 2 > issues-2.json
check report-2-arrived 0 "#79: the issue of $PACK names 1.2.0 as arrived, from the state the issue kept" -- \
  jq -e --arg p "$PACK" '[.[] | select(.title | startswith("strata behind: " + $p + " "))] | length == 1 and (.[0].title | contains("behind 1.2.0")) and (.[0].body | contains("| 1.2.0 | `"))' issues-2.json

echo "== 4. the pins move to the newest versions" | tee -a steps.log
newest_csi=$(jq -r '.newer[0].version' <<<"$(pin run-1.json csi-aws-ebs)")
check record-own-new 0 "record $PACK 1.2.0" -- "$strata" snapshot "pack:$REG_NAME/$PACK@1.2.0" -o records/own.json
check record-csi-new 0 "record csi-aws-ebs $newest_csi" -- "$strata" snapshot "pack:$public_repo/csi-aws-ebs@$newest_csi" -o records/csi.json
check report-3 0 '#79: with each pin current, upstream-report.sh closes the issues: exit 0' -- \
  "${report[@]}" records/own.json records/csi.json --repo "$GH_REPO" --label "$LABEL"
issues 0 > issues-3.json
check report-3-closed 0 "#79: no open issue has the label $LABEL" -- jq -e 'length == 0' issues-3.json
check empty-end 0 "the repository of $REG_NAME is emptied again" -- empty

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]
