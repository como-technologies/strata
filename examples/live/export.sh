#!/usr/bin/env bash
# The profile export test (#156): Palette exports a profile version as one file, and
# `snapshot --from-candidate` reads that file as a candidate. Stage 4 reads the download of each
# version, the file that `same-release --allow renumber` reads (#751). It builds no cluster, creates no
# profile, and needs no OpenTofu: each request is a GET. So it runs whenever the key works.
#
#   examples/live/export.sh [STAGE]
#
#   1  for each version: the record of the version, the export with format=yaml, the record of
#      the export, and the two roots. A version with a manifest pack differs only by the pack uid
#      that Palette mints.
#   2  the export with format=json gives the same record as the export with format=yaml
#   3  what the export route answers: a request that accepts JSON gets no file; and three
#      exports with no wait between them, whose refusals the run records
#   4  the download of each version: the layout of the file, the archive of each pack and its
#      top folder, the packs with no archive, and what the route answers to a request that
#      accepts JSON and to two downloads at once
#
# PROFILES holds the versions, by <name>@<version>, in the project of the key. The default is the
# three versions that `just live-up` publishes. Palette can refuse an export with HTTP 429 and
# the message "only 1 request in 10 seconds is allowed", so the script waits 11 seconds before
# each export. Palette takes one download of a version in 10 seconds too, so the script waits 11
# seconds before each download.
#
# Results go under runs/<UTC time>/export/, or OUT. Exit 0 only when no check fails.
set -uo pipefail
script=export
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
[ -x "$strata" ] || die "no strata binary at $strata"

stage=${1:-all}
profiles=${PROFILES:-strata-live-profile@1.0.0 strata-live-registries@1.0.0 strata-live-addon@1.0.0}

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/export}
mkdir -p "$run" && cd "$run" || exit 2

summary() {
  write_summary "profile export test" ": \`$profiles\`."
}
trap summary EXIT

# export_file ID WHAT UID FORMAT: one export, after the wait that the limit of Palette needs
export_file() {
  sleep 11
  check "$1" 0 "$2" -- "$live/api.sh" export "$3" "$4"
}

# download_layout FILE RECORD LIST: the one metadata.json at the top of the download FILE names the
# list of the packs, which goes to LIST. The list names each pack of RECORD with a registry at its
# version, and no manifest pack by its name
download_layout() {
  local top path
  top=$(tar -tzf "$1" | sed -n 's|^\([^/]*\)/metadata\.json$|\1|p')
  [ "$(printf '%s\n' "$top" | grep -c .)" = 1 ] || { echo "no one metadata.json at the top" >&2; return 1; }
  path=$(tar -xzOf "$1" "$top/metadata.json" | jq -r '.profiles[0].packsMetaFilePath') || return 1
  tar -xzOf "$1" "$top/$path" > "$3" || return 1
  echo "$top/$path" > "$3.path"
  jq -e --slurpfile record "$2" '.packs as $list | $record[0].packs
    | all(if .type == "manifest" then $list[.name] == null else $list[.name].version == .version end)' "$3"
}

# archive_tops FILE PATH: the top folders of the archive PATH in the download FILE, one on each line
archive_tops() {
  tar -xzOf "$1" "$2" | tar -tz | cut -d/ -f1 | sort -u
}

# download_archives FILE LIST: each archive of a pack that the list LIST of the download FILE names
# has one top folder, and its pack.json names the version of its entry. A chart of a Helm registry
# has no pack.json, so the check reads the archives of the other packs
download_archives() {
  local folder name path version top
  folder=$(dirname "$(cat "$2.path")")
  while IFS=$'\t' read -r name path version; do
    top=$(archive_tops "$1" "$folder/$path")
    [ "$(printf '%s\n' "$top" | grep -c .)" = 1 ] || { echo "$name: no one top folder" >&2; return 1; }
    [ "$(tar -xzOf "$1" "$folder/$path" | tar -xzO "$top/pack.json" | jq -r .version)" = "$version" ] \
      || { echo "$name: pack.json does not name $version" >&2; return 1; }
    echo "$name $version: $top/pack.json"
  done < <(jq -r '.packs[] | select(.filePath != "" and (.manifestName // "") == "" and .type != "helm")
    | [.name, .filePath, .version] | @tsv' "$2")
}

# download_tops LIST...: the top folders of each archive of a pack that each list LIST names, a Helm
# chart too. One line for each archive: the list, the pack, its type, and its top folders. The
# download of the list NAME.list.json is NAME.tgz
download_tops() {
  local list folder name type path
  for list in "$@"; do
    folder=$(dirname "$(cat "$list.path")")
    while IFS=$'\t' read -r name type path; do
      printf '%s\t%s\t%s\t%s\n' "$list" "$name" "$type" \
        "$(archive_tops "${list%.list.json}.tgz" "$folder/$path" | paste -sd, -)"
    done < <(jq -r '.packs[] | select(.filePath != "" and (.manifestName // "") == "")
      | [.name, .type, .filePath] | @tsv' "$list")
  done
}

# download_at_once UID: two downloads of the version UID at once. It starts the two before it waits
# for either. It prints the HTTP status of each, and the code and the message of a refusal. Exit 2
# when Palette refused one or both
download_at_once() {
  local i status reason refused=0 pids=()
  for i in 1 2; do
    SHOW_STATUS=1 "$live/api.sh" download "$1" > "download-limit.$i" &
    pids+=("$!")
  done
  wait "${pids[@]}"
  for i in 1 2; do
    status=$(head -n 1 "download-limit.$i")
    case "$status" in
      "HTTP 2"*) echo "download $i: $status" ;;
      *)
        refused=2
        reason=$(tail -n +2 "download-limit.$i" | jq -r '"\(.code): \(.message)"' 2>/dev/null) \
          || reason=$(tail -n +2 "download-limit.$i" | head -c 300)
        echo "download $i: ${status:-no answer} $reason"
        ;;
    esac
    rm -f "download-limit.$i"
  done
  return "$refused"
}

# the record of each version gives its uid, and the root that the export must give
for ref in $profiles; do
  name=${ref%@*}
  "$strata" snapshot "$ref" -q -o "$name.version.json" || die "no record of $ref"
done

if [ "$stage" = all ] || [ "$stage" = 1 ]; then
  echo "== the export of each version, read as a candidate (#156)" | tee -a steps.log
  for ref in $profiles; do
    name=${ref%@*}
    uid=$(jq -r .source.uid "$name.version.json")
    export_file "export-$name" "#156: GET /v1/clusterprofiles/{uid}/export?format=yaml gives the file of $ref" "$uid" yaml
    cp "export-$name.out" "$name.yaml"
    check "export-record-$name" 0 "#156: snapshot --from-candidate reads the export of $ref" -- \
      "$strata" snapshot --from-candidate "$name.yaml" -o "$name.export.json"
    check "export-same-$name" 0 "#156: same-release passes the record of the export and the record of $ref" -- \
      "$strata" same-release "$name.export.json" "$name.version.json"
    if jq -e 'any(.packs[]; .type == "manifest")' "$name.version.json" > /dev/null; then
      check "export-minted-$name" 0 "#156: the roots differ only by the uid that Palette mints for the manifest pack" -- \
        bash -c '"$1" same-release "$2" "$3" --json | jq -e ".pairs[0].mintedUidOnly | length > 0"' _ \
        "$strata" "$name.export.json" "$name.version.json"
    else
      check "export-root-$name" 0 "#156: the record of the export has the root of $ref" -- \
        test "$(jq -r .root "$name.export.json")" = "$(jq -r .root "$name.version.json")"
    fi
  done
fi

if [ "$stage" = all ] || [ "$stage" = 2 ]; then
  echo "== the export as JSON (#156)" | tee -a steps.log
  ref=${profiles%% *}
  name=${ref%@*}
  uid=$(jq -r .source.uid "$name.version.json")
  export_file "export-json-$name" "#156: format=json gives the same file as JSON" "$uid" json
  cp "export-json-$name.out" "$name.export-file.json"
  check "export-json-record-$name" 0 "#156: snapshot --from-candidate reads the JSON export" -- \
    "$strata" snapshot --from-candidate "$name.export-file.json" -o "$name.export-json.json"
  check "export-json-root-$name" 0 "#156: the JSON export gives the root of $ref" -- \
    test "$(jq -r .root "$name.export-json.json")" = "$(jq -r .root "$name.version.json")"
fi

if [ "$stage" = all ] || [ "$stage" = 3 ]; then
  echo "== what the export route answers (#156)" | tee -a steps.log
  ref=${profiles%% *}
  name=${ref%@*}
  uid=$(jq -r .source.uid "$name.version.json")
  sleep 11
  check export-accepts-json 0 '#156: a request that accepts JSON gets an answer' -- \
    "$live/api.sh" get "/v1/clusterprofiles/$uid/export?format=yaml"
  check export-accepts-json-no-file 0 '#156: the answer is {"Reader":{}}, and not the file' -- \
    grep -qx '{"Reader":{}}' export-accepts-json.out
  sleep 11
  # three exports with no wait between them: some runs see HTTP 429 TooManyRequests, and some do
  # not, so the check records the exit code and the answers, and does not judge them
  check export-limit '?' '#156: three exports with no wait between them; exit 2 when Palette refused one or more' -- \
    bash -c 'refused=0; for i in 1 2 3; do "$1" export "$2" yaml > /dev/null || refused=2; done; exit $refused' _ \
    "$live/api.sh" "$uid"
fi

if [ "$stage" = all ] || [ "$stage" = 4 ]; then
  echo "== the download of each version (#751)" | tee -a steps.log
  for ref in $profiles; do
    name=${ref%@*}
    uid=$(jq -r .source.uid "$name.version.json")
    sleep 11
    check "download-$name" 0 "#751: GET /v1/clusterprofiles/{uid}/spc/download gives the file of $ref" -- \
      "$live/api.sh" download "$uid"
    cp "download-$name.out" "$name.tgz"
    check "download-layout-$name" 0 "#751: metadata.json at the top names the list of the packs, and the list names each pack of $ref with a registry at its version" -- \
      download_layout "$name.tgz" "$name.version.json" "$name.list.json"
    check "download-archive-$name" 0 "#751: each archive of a pack of $ref has one top folder, and its pack.json names the version of its entry" -- \
      download_archives "$name.tgz" "$name.list.json"
  done
  # the form of the top folder of each archive: the check records it, and does not judge it
  check download-top-folders '?' '#815: the top folders of each archive of a pack in each download, a Helm chart too' -- \
    download_tops ./*.list.json
  # which packs have no archive, and why: the records name the registry of each pack, and the
  # check records the entries and does not judge them
  check download-no-archive '?' '#751: the entry of each pack with no archive: its name, its type, and isRegistryPrivate' -- \
    bash -c 'for f in "$@"; do jq -r --arg f "$f" ".packs[] | select(.filePath == \"\" and (.manifestName // \"\") == \"\") | [\$f, .name, .type, .isRegistryPrivate] | @tsv" "$f"; done' _ ./*.list.json
  ref=${profiles%% *}
  name=${ref%@*}
  uid=$(jq -r .source.uid "$name.version.json")
  sleep 11
  check download-accepts-json 0 '#751: a request that accepts JSON gets an answer' -- \
    "$live/api.sh" get "/v1/clusterprofiles/$uid/spc/download"
  check download-accepts-json-no-file 0 '#751: the answer is {"Reader":{}}, and not the file, as for the export' -- \
    grep -qx '{"Reader":{}}' download-accepts-json.out
  sleep 11
  # two downloads of one version at once: Palette takes one in 10 seconds, and can refuse one with
  # HTTP 429 TooManyRequests. A refusal depends on the server of the API that each request reaches,
  # so a run can see none. The check records the status of each, and does not judge them
  check download-limit '?' '#751, #815: two downloads of one version at once; the status of each, and exit 2 when Palette refused one or both' -- \
    download_at_once "$uid"
fi

echo
echo "$script: $(grep -c PASS steps.log) passed, $failures failed; results in $run"
[ "$failures" = 0 ]
