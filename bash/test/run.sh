#!/usr/bin/env bash
# Offline tests for palette-profile-diff: generates fixtures, snapshots them, asserts on the report.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
tool="$here/../palette-profile-diff"
tmp=$(mktemp -d "${TMPDIR:-/tmp}/ppd-test.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

pass=0; fail=0
ESC=$(printf '\033')
check() { # name, shell condition
  if eval "$2"; then pass=$((pass + 1)); else fail=$((fail + 1)); echo "FAIL: $1"; fi
}

"$here/make-fixtures.sh" "$tmp/fx" >/dev/null
export PPD_QUIET=1
"$tool" snapshot --from-raw "$tmp/fx/a"  -o "$tmp/a.json"
"$tool" snapshot --from-raw "$tmp/fx/a2" -o "$tmp/a2.json"
"$tool" snapshot --from-raw "$tmp/fx/b"  -o "$tmp/b.json"

# snapshot shape
check "snapshot has schema marker" '[ "$(jq -r .schema "$tmp/a.json")" = palette-profile-snapshot/1 ]'
check "snapshot has 7 packs"       '[ "$(jq ".packs | length" "$tmp/a.json")" = 7 ]'
check "os pack falls back to registry digest" '[ "$(jq -r ".packs[] | select(.name==\"ubuntu-aws\") | .hash.upstreamSource" "$tmp/a.json")" = packSummary.digest ]'
check "k8s pack uses pack ref digest"         '[ "$(jq -r ".packs[] | select(.name==\"kubernetes\") | .hash.upstreamSource" "$tmp/a.json")" = packRef.digest ]'
check "unparseable values recorded"           'jq -e ".packs[] | select(.name==\"metrics-server\") | .valuesParseError" "$tmp/a.json" >/dev/null'

check "comment-only values canonicalize to null" '[ "$(jq -c ".packs[] | select(.name==\"ubuntu-aws\") | .valuesCanon" "$tmp/a.json")" = null ]'

# canonicalization: reordered YAML must not move the root
check "reordered YAML gives identical root" '[ "$(jq -r .root "$tmp/a.json")" = "$(jq -r .root "$tmp/a2.json")" ]'

# identical compare
set +e; "$tool" compare "$tmp/a.json" "$tmp/a2.json" > "$tmp/same.txt"; rc=$?; set -e
check "identical compare exits 0"        '[ "$rc" -eq 0 ]'
check "identical report says IDENTICAL"  'grep -q IDENTICAL "$tmp/same.txt"'
check "identical report has no ~ packs"  '! grep -qE "^~ \[" "$tmp/same.txt"'

# changed compare
set +e; "$tool" compare "$tmp/a.json" "$tmp/b.json" > "$tmp/diff.txt"; rc=$?; set -e
check "changed compare exits 1"          '[ "$rc" -eq 1 ]'
check "report says CHANGED"              'grep -q "CHANGED" "$tmp/diff.txt"'
check "k8s: version bump flagged IDENTITY UPSTREAM AUTHORED" 'grep -E "^~ \[k8s  \] kubernetes +1\.29\.4 -> 1\.30\.2 +IDENTITY UPSTREAM AUTHORED" "$tmp/diff.txt" -q'
check "k8s: values path diff shown"      'grep -qF "~ kubeadmconfig.apiServer.extraArgs.audit-log-maxage: \"30\" -> \"90\"" "$tmp/diff.txt"'
check "k8s: added value shown"           'grep -qF "+ kubeadmconfig.apiServer.extraArgs.profiling: \"false\"" "$tmp/diff.txt"'
check "k8s: false-valued leaf diffed"   'grep -qF "~ pack.k8sHardening: true -> false" "$tmp/diff.txt"'
check "consumers line"                   'grep -qE "^consumers +0 -> 2 clusters$" "$tmp/diff.txt"'
check "k8s: repave impact"               'grep -q "impact: node repave (Kubernetes version change)" "$tmp/diff.txt"'
check "cilium: same tag, UPSTREAM only"  'grep -E "^~ \[cni  \] cni-cilium-oss +1\.15\.3 +UPSTREAM$" "$tmp/diff.txt" -q'
check "cilium: digest line"              'grep -q "upstream digest  sha256:cni-a -> sha256:cni-b   (packRef.digest)" "$tmp/diff.txt"'
check "cilium: under-the-hood impact"    'grep -q "pack content changed under the same tag" "$tmp/diff.txt"'
check "os/csi unchanged"                 'grep -qE "^  \[os   \] ubuntu-aws .*unchanged$" "$tmp/diff.txt" && grep -qE "^  \[csi  \] csi-aws-ebs .*unchanged$" "$tmp/diff.txt"'
check "prometheus: AUTHORED only"        'grep -E "^~ \[addon\] prometheus-operator +55\.8\.3 +AUTHORED$" "$tmp/diff.txt" -q'
check "prometheus: manifest changed"     'grep -q "~ manifest extra-rules" "$tmp/diff.txt"'
check "prometheus: manifest added"       'grep -q "+ manifest dashboards" "$tmp/diff.txt"'
check "cert-manager ADDED"               'grep -qE "^\+ \[addon\] cert-manager +1\.14\.0 +ADDED$" "$tmp/diff.txt"'
check "external-dns REMOVED"             'grep -qE "^- \[addon\] external-dns +0\.14\.0 +REMOVED$" "$tmp/diff.txt"'
check "metrics-server: text-only note"   'grep -q "values changed (not parseable as YAML" "$tmp/diff.txt"'
check "variables CHANGED"                'grep -q "variables CHANGED" "$tmp/diff.txt"'
check "variable default changed"         'grep -qF "~ cluster_env.defaultValue: \"dev\" -> \"staging\"" "$tmp/diff.txt"'
check "variable added"                   'grep -q "+ db_password.name" "$tmp/diff.txt"'

# --full unified diffs
set +e; "$tool" compare --full "$tmp/a.json" "$tmp/b.json" > "$tmp/full.txt"; set -e
check "--full shows values unified diff"   'grep -q -- "--- a: addon/metrics-server values" "$tmp/full.txt" && grep -q -- "+      - --metric-resolution=15s" "$tmp/full.txt"'
check "--full shows manifest unified diff" 'grep -q -- "--- a: addon/prometheus-operator manifest extra-rules" "$tmp/full.txt" && grep -q -- "+          for: 10m" "$tmp/full.txt"'

# color: off when piped, on when forced, never in --json, and escapes are the only difference
check "no escapes when piped"            '! grep -q "$ESC\[" "$tmp/diff.txt"'
set +e; "$tool" compare --color=always "$tmp/a.json" "$tmp/b.json" > "$tmp/color.txt"; set -e
check "--color=always emits escapes"     'grep -q "$ESC\[" "$tmp/color.txt"'
check "colored report matches plain once stripped" '[ "$(sed "s/$ESC\[[0-9;]*m//g" "$tmp/color.txt")" = "$(cat "$tmp/diff.txt")" ]'
set +e; "$tool" compare --color=never "$tmp/a.json" "$tmp/b.json" > "$tmp/nocolor.txt"; set -e
check "--color=never emits none"         '! grep -q "$ESC\[" "$tmp/nocolor.txt"'
set +e; CLICOLOR_FORCE=1 "$tool" compare "$tmp/a.json" "$tmp/b.json" > "$tmp/force.txt"; set -e
check "CLICOLOR_FORCE turns color on"    'grep -q "$ESC\[" "$tmp/force.txt"'
set +e; NO_COLOR=1 CLICOLOR_FORCE=1 "$tool" compare "$tmp/a.json" "$tmp/b.json" > "$tmp/nc.txt"; set -e
check "NO_COLOR wins over CLICOLOR_FORCE" '! grep -q "$ESC\[" "$tmp/nc.txt"'
set +e; "$tool" compare --color=always --json "$tmp/a.json" "$tmp/b.json" > "$tmp/cj.json"; set -e
check "--json never colored"             '! grep -q "$ESC\[" "$tmp/cj.json" && [ "$(jq -r .same "$tmp/cj.json")" = false ]'
set +e; "$tool" compare --color=bogus "$tmp/a.json" "$tmp/b.json" >/dev/null 2>"$tmp/err.txt"; rc=$?; set -e
check "--color=bogus rejected (exit 2)"  '[ "$rc" -eq 2 ] && grep -q "auto, always, or never" "$tmp/err.txt"'

# --json
set +e; "$tool" compare --json "$tmp/a.json" "$tmp/b.json" > "$tmp/diff.json"; set -e
check "--json is valid and same=false"   '[ "$(jq -r .same "$tmp/diff.json")" = false ]'
check "--json cilium flags"              '[ "$(jq -c ".packs[] | select(.name==\"cni-cilium-oss\") | [.identity,.upstream,.authored]" "$tmp/diff.json")" = "[false,true,false]" ]'
check "--json k8s valuesDiff count"      '[ "$(jq ".packs[] | select(.name==\"kubernetes\") | .valuesDiff | length" "$tmp/diff.json")" = 4 ]'

# bad inputs
set +e; "$tool" compare "$tmp/fx/a/profile.json" "$tmp/b.json" >/dev/null 2>"$tmp/err.txt"; rc=$?; set -e
check "non-snapshot file rejected (exit 2)" '[ "$rc" -eq 2 ] && grep -q "not a palette-profile-diff snapshot" "$tmp/err.txt"'

# verify: argument validation only (live verification needs a tenant)
set +e; "$tool" verify "$tmp/fx/a/profile.json" >/dev/null 2>"$tmp/err.txt"; rc=$?; set -e
check "verify rejects non-snapshot (exit 2)" '[ "$rc" -eq 2 ] && grep -q "not a palette-profile-diff snapshot" "$tmp/err.txt"'
set +e; env -u PALETTE_API_KEY "$tool" verify "$tmp/a.json" >/dev/null 2>"$tmp/err.txt"; rc=$?; set -e
check "verify without API key fails cleanly (exit 2)" '[ "$rc" -eq 2 ] && grep -q "PALETTE_API_KEY is required" "$tmp/err.txt"'

echo "passed: $pass  failed: $fail"
[ "$fail" -eq 0 ]
