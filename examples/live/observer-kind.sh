#!/usr/bin/env bash
# The observer on kind (#644): the manifests of deploy/observer, the records that observer-records.sh
# writes, the pod that runs, and the page, as a person with only the reader role sees it.
#
#   examples/live/observer-kind.sh [FROM_STAGE]
#
#   1  a kind cluster, with the image of the observer loaded (just observer-image builds it)
#   2  the install: the namespace, the ConfigMap, the Secret of the key, the records of the
#      Git revision that is checked out, and the manifests with the image tag of this run. Each
#      output of observer-records.sh goes through kubectl apply --server-side. Then the script
#      waits for the Deployment
#   3  the pod: Ready, with runAsNonRoot and a root file system that is read-only, and with no token
#      of the service account in it
#   4  the page, through kubectl port-forward deploy/strata-observer, as a user with only the role
#      deploy/observer/reader-role.yaml. The page answers and shows the records. The role gives no
#      Secret and no exec
#   5  another pod in the cluster does not reach the page
#   6  the records of --first-seen apply too, and their meta.json says first-seen
#   7  the end: the kind cluster is deleted, also when a check fails; KEEP_CLUSTER=1 keeps it
#
# The image is distroless: it has no shell, so the checks of stage 3 read the spec of the pod, and
# the kubelet enforces it. The cluster that the observer watches is cluster A of the live example.
# Its records are the release profile and the add-on profile, read from the tenant. The observer
# runs its first cycle with the key, so the script needs a network that reaches Palette from the
# nodes. Stage 5 needs a kind that enforces network policy; the observer also listens on
# 127.0.0.1 only, so the connection fails in either case.
#
# Needs the live example at its baseline with cluster A (just live-up), the strata release binary,
# docker, kind, kubectl, curl, jq, and the key of verify.sh. IMAGE names the image (default
# strata-observer:dev). PORT is the local port of the port-forward (default 18080). The key goes to
# the Secret of the cluster through standard input, and it is not in a log. Results go under
# runs/<UTC time>/observer-kind/, or OUT; FROM_STAGE resumes a run in OUT. Exit 0 only when no check
# fails.
set -uo pipefail
script=observer-kind
from=${1:-1}
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
case "$from" in 1|2|3|4|5|6|7) ;; *) usage; exit 2 ;; esac
[ -x "$strata" ] || die "no strata binary at $strata"
for tool in docker kind kubectl curl jq git; do
  command -v "$tool" > /dev/null || die "$tool is needed"
done
image=${IMAGE:-strata-observer:dev}
port=${PORT:-18080}
docker image inspect "$image" > /dev/null 2>&1 || die "no image $image; run just observer-image"

run=${OUT:-$live/runs/$(date -u +%Y%m%dT%H%M%SZ)/observer-kind}
mkdir -p "$run" && cd "$run" || exit 2
export STRATA_QUIET=1

load_outputs
C=$(out cluster_uid)
[ -n "$C" ] || die "no cluster A; run just live-up"
export PALETTE_PROJECT_UID=$(out project_uid)
REL=$(out release_profile_uid)
AD=$(out addon_profile_uid)
export STRATA_BIN=$strata
root=$(cd "$live/../.." && pwd)
records_script=$root/scripts/ci/observer-records.sh
revision=$(git -C "$root" rev-parse HEAD)

# the key and the address for the pod: as api.sh reads them
from_env_file() {
  [ -f "$HOME/.palette.env" ] || return 0
  (set -a; . "$HOME/.palette.env" >/dev/null 2>&1; printf %s "${!1:-}")
}
palette_key=${PALETTE_API_KEY:-$(from_env_file PALETTE_API_KEY)}
[ -n "$palette_key" ] || die "set PALETTE_API_KEY or put it in ~/.palette.env"
API=${PALETTE_API:-$(from_env_file PALETTE_API)}
API=${API:-https://api.spectrocloud.com}

cluster=strata-observer-test
ns=strata-observer
reader=system:serviceaccount:$ns:reader
pf_pid=
deleted=
# kc ARGS...: kubectl in the kind cluster of this run. Exported, so that a check in bash -c has it.
kc() { kubectl --context "kind-$cluster" "$@"; }
export -f kc
export cluster ns reader port revision C palette_key

summary() {
  write_summary "the observer on kind" ", cluster \`$C\`, image \`$image\`, revision \`$revision\`."
}
# the port-forward and the kind cluster go at the end, also when a check stops the script
cleanup() {
  [ -z "$pf_pid" ] || kill "$pf_pid" 2> /dev/null
  [ -n "$deleted" ] || [ "${KEEP_CLUSTER:-}" = 1 ] || kind delete cluster --name "$cluster" >> steps.log 2>&1
  summary
}
trap cleanup EXIT

if stage 1 "a kind cluster with the image of the observer"; then
  check kind-create 0 '#644: a kind cluster' -- kind create cluster --name "$cluster" --wait 120s
  check image-load 0 '#644: the image of the observer is in the nodes' -- \
    kind load docker-image "$image" --name "$cluster"
fi

if stage 2 "the install: the namespace, the settings, the records, and the manifests"; then
  check namespace 0 '#644: the namespace holds the label that makes the restricted standard' -- \
    kc apply --server-side -f "$root/deploy/observer/namespace.yaml"
  check config 0 '#644: the ConfigMap holds the cluster, the address of Palette, and the project' -- \
    kc -n "$ns" create configmap strata-observer --from-literal=cluster-uid="$C" \
      --from-literal=palette-api="$API" --from-literal=project-uid="$PALETTE_PROJECT_UID"
  check key 0 '#644: the Secret holds the key, which comes through standard input' -- \
    bash -c "printf %s \"\$palette_key\" | kc -n \"\$ns\" create secret generic strata-observer-key --from-file=api-key=/dev/stdin"
  check record-release 0 '#644: a record of the release profile' -- "$strata" snapshot "$REL" -o release.json -q
  check record-addon 0 '#644: a record of the add-on profile' -- "$strata" snapshot "$AD" -o addon.json -q
  check records-apply 0 '#644: the output of the records script applies with --server-side' -- \
    bash -c "\"$records_script\" --revision \"\$revision\" release.json addon.json | kc apply --server-side -f -"
  check records-four 0 '#644: four Secrets of records exist' -- \
    bash -c "kc -n \"\$ns\" get secret -l app.kubernetes.io/name=strata-observer -o name | grep -c records | grep -qx 4"
  check records-meta 0 '#644: meta.json says approved, with the revision' -- \
    bash -c "kc -n \"\$ns\" get secret strata-observer-records-0 -o jsonpath='{.data.meta\\.json}' | base64 -d | jq -e --arg r \"\$revision\" '.baseline == \"approved\" and .revision == \$r'"
  check manifests-apply 0 '#644: the manifests apply with --server-side, with the image of this run' -- \
    bash -c "kubectl kustomize \"$root/deploy/observer\" | sed 's#ghcr.io/como-technologies/strata-observer:VERSION#$image#' | kc apply --server-side -f -"
  check rollout 0 '#644: the Deployment becomes available' -- \
    kc -n "$ns" rollout status deploy/strata-observer --timeout=180s
fi

selector=app.kubernetes.io/name=strata-observer
if stage 3 "the pod"; then
  check pod-ready 0 '#644: the pod is Ready' -- \
    kc -n "$ns" wait --for=condition=Ready pod -l "$selector" --timeout=120s
  kc -n "$ns" get pod -l "$selector" -o json > pod.json
  check pod-non-root 0 '#644: the pod runs as non-root' -- \
    jq -e '.items[0].spec | .securityContext.runAsNonRoot == true and .securityContext.runAsUser == 65532 and .containers[0].securityContext.allowPrivilegeEscalation == false' pod.json
  check pod-read-only-root 0 '#644: the root file system is read-only, and no capability is left' -- \
    jq -e '.items[0].spec.containers[0].securityContext | .readOnlyRootFilesystem == true and .capabilities.drop == ["ALL"]' pod.json
  check pod-no-token 0 '#644: no token of the service account is in the pod' -- \
    jq -e '.items[0].spec | .automountServiceAccountToken == false and ([.volumes[]?.name | select(startswith("kube-api-access"))] | length == 0) and ([.containers[].volumeMounts[]?.mountPath | select(startswith("/var/run/secrets/kubernetes.io/serviceaccount"))] | length == 0)' pod.json
  check pod-ready-status 0 '#644: the container is ready and has not restarted' -- \
    jq -e '.items[0].status.containerStatuses[0] | .ready == true and .restartCount == 0' pod.json
fi

if stage 4 "the page, as a user with only the reader role"; then
  check reader-role 0 '#644: the reader role and a binding for the account reader' -- \
    bash -c "kc apply --server-side -f \"$root/deploy/observer/reader-role.yaml\" && kc -n \"\$ns\" create serviceaccount reader && kc -n \"\$ns\" create rolebinding reader --role=strata-observer-reader --serviceaccount=\$ns:reader"
  check reader-can-forward 0 '#644: the reader can open a port-forward' -- \
    kc -n "$ns" auth can-i create pods --subresource=portforward --as="$reader"
  check reader-no-secret 1 '#644: the reader can read no Secret' -- \
    kc -n "$ns" auth can-i get secrets --as="$reader"
  check reader-no-exec 1 '#644: the reader can run no exec' -- \
    kc -n "$ns" auth can-i create pods --subresource=exec --as="$reader"
  kubectl --context "kind-$cluster" -n "$ns" --as="$reader" port-forward deploy/strata-observer "$port:8080" > port-forward.log 2>&1 &
  pf_pid=$!
  for _ in $(seq 30); do
    grep -q 'Forwarding from' port-forward.log && break
    sleep 1
  done
  check reader-forward 0 '#644: kubectl port-forward deploy/strata-observer works for the reader' -- \
    grep -q 'Forwarding from' port-forward.log
  check reader-health 0 '#644: the health route answers ok' -- \
    bash -c "curl -fsS http://127.0.0.1:$port/healthz | grep -qx ok"
  check reader-page 0 '#644: the route / serves the page' -- \
    bash -c "curl -fsS -o page.html -w '%{http_code}' http://127.0.0.1:$port/ | grep -qx 200 && grep -qi '<html' page.html"
  # the summary names the records after the first content cycle, which waits for the requests to Palette
  for _ in $(seq "${CYCLE_WAIT:-60}"); do
    curl -fsS "http://127.0.0.1:$port/v1/cluster" | jq -e '.records.count > 0' > /dev/null 2>&1 && break
    sleep 5
  done
  check reader-cluster 0 '#644: the summary names the cluster and the approved records of the revision' -- \
    bash -c "curl -fsS http://127.0.0.1:$port/v1/cluster | jq -e --arg c \"\$C\" --arg r \"\$revision\" '.cluster.uid == \$c and .records.count == 2 and .records.baseline == \"approved\" and .records.revision == \$r'"
  [ -z "$pf_pid" ] || { kill "$pf_pid" 2> /dev/null; pf_pid=; }
fi

if stage 5 "another pod"; then
  pod_ip=$(kc -n "$ns" get pod -l "$selector" -o jsonpath='{.items[0].status.podIP}')
  # the pod prints "refused" when wget fails, so a pod that did not start is no refusal
  check other-pod-refused 0 '#644: a pod of another namespace cannot connect to the page' -- \
    bash -c "kc -n default run observer-probe --rm -i --restart=Never --image=busybox:1.37 --command -- sh -c 'wget -T 5 -qO- http://$pod_ip:8080/healthz && echo reached || echo refused' | grep -qx refused"
fi

if stage 6 "the records of --first-seen"; then
  check first-seen-apply 0 '#644: the output of --first-seen applies with --server-side' -- \
    bash -c "\"$records_script\" --first-seen \"\$C\" | kc apply --server-side -f -"
  check first-seen-meta 0 '#644: meta.json says first-seen and names the cluster' -- \
    bash -c "kc -n \"\$ns\" get secret strata-observer-records-0 -o jsonpath='{.data.meta\\.json}' | base64 -d | jq -e --arg c \"\$C\" '.baseline == \"first-seen\" and .firstSeenFrom == \$c and (has(\"revision\") | not)'"
fi

if stage 7 "the end: the kind cluster goes"; then
  if [ "${KEEP_CLUSTER:-}" = 1 ]; then
    echo "KEEP_CLUSTER=1: the kind cluster $cluster stays" | tee -a steps.log
  else
    check kind-delete 0 '#644: the kind cluster is deleted' -- kind delete cluster --name "$cluster"
    deleted=1
  fi
fi

echo "observer-kind: $failures check(s) failed; results in $run"
[ "$failures" = 0 ]
