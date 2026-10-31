#!/usr/bin/env bash
# The draft test (#155): what a draft of a published version is to Palette and to strata. A PUT
# of a published version with no publish writes the draft; `<ref>@draft` records it; the publish
# makes it the version.
#
#   examples/live/draft.sh
#
# It records the add-on version, and edits its manifest through the API with no publish. It
# records `<uid>@draft` and the version again, and compares the two. It reads what each route
# of Palette gives while the draft waits. Then it publishes the draft, verifies the record of
# the draft against the version, and puts the edit back with a publish. Last, it creates a
# copy of the add-on with no publish, lists it, and deletes it (#114).
#
# Needs the profiles (just live-profiles) and no cluster: no cost. It leaves the add-on version
# with the content it found. Needs the strata release binary, jq, and the key as verify.sh needs
# it. Results go under runs/<UTC time>/draft/, or OUT. Exit 0 only when no check fails.
set -uo pipefail
script=draft
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
case "${1:-}" in "") ;; *) usage; exit 2 ;; esac
[ -x "$strata" ] || die "no strata binary at $strata"

addon=strata-live-addon
unpublished=strata-live-unpublished

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/draft}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

summary() {
  write_summary "draft test" ", add-on" \
    "\`$addon@1.0.0\` (\`${AD:-}\`)."
}
trap summary EXIT

AD=$("$strata" list --json 2> /dev/null | jq -r --arg n "$addon" '.profiles[] | select(.name == $n and .version == "1.0.0") | .uid' | head -1)
[ -n "$AD" ] || die "no $addon@1.0.0 in the project; run just live-profiles"
echo "AD=$AD" > ids.env

# undo: put the edit back with a publish, when the add-on version differs from its record
undo() {
  "$strata" compare addon.json "$AD" > /dev/null 2>&1 && return 0
  "$live/api.sh" publish "$AD" >> steps.log 2>&1
  "$live/api.sh" edit "$AD" "$addon_to" "$addon_from" >> steps.log 2>&1
}

check record 0 "record $addon@1.0.0" -- "$strata" snapshot "$AD" -o addon.json
check no-draft 2 '#155: a version with no draft: <uid>@draft exits 2 with no-draft' -- \
  "$strata" snapshot "$AD@draft"
check no-draft-says 0 '#155: the error says that the version has no draft' -- \
  grep -q 'has no draft' no-draft.err
check api-draft 0 "#155: edit the manifest of $addon@1.0.0 through the API, with no publish" -- \
  "$live/api.sh" draft "$AD" "$addon_from" "$addon_to"
check list-published 0 '#114: list --packs gives published true for a published version while its draft waits' -- \
  bash -c "\"$strata\" list --packs --json | jq -e --arg u '$AD' '[.profiles[] | select(.uid == \$u)][0].published == true'"
check version-holds 0 '#155: the version keeps its content while the draft waits: its record verifies' -- \
  "$strata" verify addon.json
check draft-record 0 '#155: <uid>@draft records the draft' -- "$strata" snapshot "$AD@draft" -o draft.json
check draft-template 0 '#155: the record says source.template draft, and holds the edit in the manifest' -- \
  jq -e --arg to "$addon_to" '.source.template == "draft" and ([.packs[].manifests[].content] | any(contains($to)))' draft.json
check version-record 0 '#155: the record of the version, read while the draft waits, holds the published manifest' -- \
  bash -c "\"$strata\" snapshot \"$AD\" -o version.json && jq -e --arg from '$addon_from' \
    '.source.template == \"published\" and ([.packs[].manifests[].content] | any(contains(\$from)))' version.json"
check compare 1 '#155: compare of the version and its draft exits 1' -- \
  "$strata" compare "$AD" "$AD@draft"
check compare-names 0 '#155: the comparison names the manifest pack that the publish changes' -- \
  grep -q 'AUTHORED' compare.out
check route-list 0 '#155: packs/manifests gives the published content of the manifest while the draft waits' -- \
  bash -c "\"$live/api.sh\" get \"/v1/clusterprofiles/$AD/packs/manifests\" \
    | jq -e --arg from '$addon_from' '[.spec.packs[].spec.manifests[]?.content] | any(contains(\$from))'"
check route-entity 0 '#155: the route of one manifest gives the draft content beside the published content' -- \
  bash -c "pack=\$(jq -r '[.spec.draft.packs[] | select((.manifests // []) | length > 0)][0].name' \
      <(\"$live/api.sh\" get \"/v1/clusterprofiles/$AD\")); \
    muid=\$(\"$live/api.sh\" get \"/v1/clusterprofiles/$AD\" | jq -r --arg p \"\$pack\" \
      '.spec.draft.packs[] | select(.name == \$p) | .manifests[0].uid'); \
    \"$live/api.sh\" get \"/v1/clusterprofiles/$AD/packs/\$pack/manifests/\$muid\" \
      | jq -e --arg to '$addon_to' --arg from '$addon_from' \
        '(.spec.draft.content | contains(\$to)) and (.spec.published.content | contains(\$from))'"
check no-has-draft 0 '#155: the profile document gives no status.hasDraft' -- \
  bash -c "\"$live/api.sh\" get \"/v1/clusterprofiles/$AD\" | jq -e '.status | has(\"hasDraft\") | not'"
check draft-before-publish 1 '#155: the record of the draft reports DRIFT against the version before the publish' -- \
  "$strata" verify draft.json
check api-publish 0 '#155: publish the draft through the API' -- "$live/api.sh" publish "$AD"
check draft-after-publish 0 '#155: after the publish, the record of the draft verifies against the version' -- \
  "$strata" verify draft.json
check version-after-publish 1 '#155: the record of the version from before reports DRIFT' -- \
  "$strata" verify addon.json
check no-draft-after-publish 2 '#155: the publish empties the draft: <uid>@draft exits 2 again' -- \
  "$strata" snapshot "$AD@draft"
undo
check restored 0 'the edit is put back, and the first record verifies again' -- "$strata" verify addon.json

# a version that nobody published (#114): a create with no publish
"$live/api.sh" delete-named "$unpublished" >> steps.log 2>&1
check unpublished-create 0 "#114: create $unpublished with the packs of the add-on, and no publish" -- \
  "$live/api.sh" create-unpublished "$AD" "$unpublished"
UN=$(cat unpublished-create.out)
check unpublished-listed 0 '#114: list --packs gives published false for the version, with the packs of its draft' -- \
  bash -c "\"$strata\" list --packs --json | jq -e --arg u '$UN' \
    '[.profiles[] | select(.uid == \$u and .published == false and (.packs | length) > 0)] | length == 1'"
check unpublished-mark 0 '#114: the terminal row of the version says draft' -- \
  bash -c "\"$strata\" list --packs --no-color | grep -qE '^  $unpublished@1\.0\.0 .* draft\$'"
check unpublished-delete 0 "#114: delete $unpublished" -- "$live/api.sh" delete-named "$unpublished"

echo
echo "$script: $(grep -c '^PASS' steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]
