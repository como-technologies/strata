#!/usr/bin/env bash
# The observer on kind (#644): the manifests of deploy/observer, the records that observer-records.sh
# writes, the pod that runs, and the page, as a person with only the reader role sees it.
#
#   examples/live/observer-kind.sh [FROM_STAGE]
#
#   1  a kind cluster, whose containerd reads the folder certs.d, with the image of the observer loaded
#      (just observer-image builds it)
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
#   7  the pods and a mirror (#646): a local registry, and a containerd mirror for vendor.example on
#      each node. One pod names vendor.example, and one names the registry itself. The component
#      deploy/observer/pods applies, and the observer reads the pods. The checks mirror-image-id
#      and mirror-image-string record what each pod reports and what the observer shows for it
#   8  the alerts (#648): a receiver pod in the cluster, which takes a POST on port 443 and keeps each
#      body. The component deploy/observer/alerts and the file of examples/observer-alerts apply, with
#      --alerts-allow-http. An edit in place of the add-on version opens a finding, and the undo
#      clears it. The receiver gets one message of the open and one of the clear, and none of them holds
#      the text of the edit. The script puts the edit back at its end, also when a check fails
#   9  the end: the kind cluster is deleted, also when a check fails; KEEP_CLUSTER=1 keeps it
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
# strata-observer:dev). PORT is the local port of the port-forward (default 18080). REGISTRY_PORT is
# the local port of the registry of stage 7 (default 5001). The key goes to the Secret of the
# cluster through standard input, and it is not in a log. Results go under
# runs/<UTC time>/observer-kind/, or OUT; FROM_STAGE resumes a run in OUT. Exit 0 only when no check
# fails.
set -uo pipefail
script=observer-kind
from=${1:-1}
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
case "$from" in 1|2|3|4|5|6|7|8|9) ;; *) usage; exit 2 ;; esac
[ -x "$strata" ] || die "no strata binary at $strata"
for tool in docker kind kubectl curl jq git; do
  command -v "$tool" > /dev/null || die "$tool is needed"
done
image=${IMAGE:-strata-observer:dev}
port=${PORT:-18080}
tries=${CYCLE_WAIT:-60}
registry=strata-observer-registry
registry_port=${REGISTRY_PORT:-5001}
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

palette_settings

cluster=strata-observer-test
ns=strata-observer
reader=system:serviceaccount:$ns:reader
deleted=
mirror_made=
edited=
# kc ARGS...: kubectl in the kind cluster of this run. Exported, so that a check in bash -c has it.
kc() { kubectl --context "kind-$cluster" "$@"; }
# cluster_kubeconfig: pf_start of lib.sh opens its port-forward in the kind cluster of this run.
cluster_kubeconfig() { kind get kubeconfig --name "$cluster"; }
export -f kc
export cluster ns reader port revision C

# kind_create: a kind cluster whose containerd reads the folder /etc/containerd/certs.d. A node of
# kind sets config_path only when the config says so, and the mirror of stage 7 needs the folder.
kind_create() {
  cat > kind-config.yaml << 'EOF'
kind: Cluster
apiVersion: kind.x-k8s.io/v1alpha4
containerdConfigPatches:
  - |-
    [plugins."io.containerd.grpc.v1.cri".registry]
      config_path = "/etc/containerd/certs.d"
EOF
  kind create cluster --name "$cluster" --wait 120s --config kind-config.yaml
}

summary() {
  write_summary "the observer on kind" ", cluster \`$C\`, image \`$image\`, revision \`$revision\`."
}
# the port-forward and the kind cluster go at the end, also when a check stops the script
cleanup() {
  pf_stop
  [ -z "$kubeconfig" ] || rm -f "$kubeconfig"
  [ -z "$mirror_made" ] || mirror_remove >> steps.log 2>&1
  # an edit in place goes back also when a check of stage 8 fails
  [ -z "$edited" ] || "$live/api.sh" edit "$AD" "$addon_to" "$addon_from" >> steps.log 2>&1
  [ -n "$deleted" ] || [ "${KEEP_CLUSTER:-}" = 1 ] || kind delete cluster --name "$cluster" >> steps.log 2>&1
  summary
}
trap cleanup EXIT

if stage 1 "a kind cluster with the image of the observer"; then
  check kind-create 0 '#644: a kind cluster, whose containerd reads the folder certs.d' -- kind_create
  check image-load 0 '#644: the image of the observer is in the nodes' -- \
    kind load docker-image "$image" --name "$cluster"
fi

if stage 2 "the install: the namespace, the settings, the records, and the manifests"; then
  check namespace 0 '#644: the namespace holds the label that makes the restricted standard' -- \
    kc apply --server-side -f "$root/deploy/observer/namespace.yaml"
  check config 0 '#644: the ConfigMap holds the cluster, the address of Palette, and the project' -- observer_config
  check key 0 '#644: the Secret holds the key, which comes through standard input' -- observer_key "$palette_key"
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

# records_named: the summary of the cluster names the records, which the first content cycle reads
records_named() { page /v1/cluster 2> /dev/null | jq -e '.records.count > 0' > /dev/null 2>&1; }

if stage 4 "the page, as a user with only the reader role"; then
  check reader-role 0 '#644: the reader role and a binding for the account reader' -- \
    bash -c "kc apply --server-side -f \"$root/deploy/observer/reader-role.yaml\" && kc -n \"\$ns\" create serviceaccount reader && kc -n \"\$ns\" create rolebinding reader --role=strata-observer-reader --serviceaccount=\$ns:reader"
  check reader-can-forward 0 '#644: the reader can open a port-forward' -- \
    kc -n "$ns" auth can-i create pods --subresource=portforward --as="$reader"
  check reader-no-secret 1 '#644: the reader can read no Secret' -- \
    kc -n "$ns" auth can-i get secrets --as="$reader"
  check reader-no-exec 1 '#644: the reader can run no exec' -- \
    kc -n "$ns" auth can-i create pods --subresource=exec --as="$reader"
  check reader-forward 0 '#644: kubectl port-forward deploy/strata-observer works for the reader' -- pf_start "$reader"
  check reader-health 0 '#644: the health route answers ok' -- \
    bash -c "curl -fsS http://127.0.0.1:$port/healthz | grep -qx ok"
  check reader-page 0 '#644: the route / serves the page' -- \
    bash -c "curl -fsS -o page.html -w '%{http_code}' http://127.0.0.1:$port/ | grep -qx 200 && grep -qi '<html' page.html"
  # the summary names the records after the first content cycle, which waits for the requests to Palette
  poll "$tries" records_named
  check reader-cluster 0 '#644: the summary names the cluster and the approved records of the revision' -- \
    bash -c "curl -fsS http://127.0.0.1:$port/v1/cluster | jq -e --arg c \"\$C\" --arg r \"\$revision\" '.cluster.uid == \$c and .records.count == 2 and .records.baseline == \"approved\" and .records.revision == \$r'"
  pf_stop
fi

if stage 5 "another pod"; then
  check other-pod-refused 0 '#644: a pod of another namespace cannot connect to the page' -- other_pod_refused
fi

if stage 6 "the records of --first-seen"; then
  check first-seen-apply 0 '#644: the output of --first-seen applies with --server-side' -- \
    bash -c "\"$records_script\" --first-seen \"\$C\" | kc apply --server-side -f -"
  check first-seen-meta 0 '#644: meta.json says first-seen and names the cluster' -- \
    bash -c "kc -n \"\$ns\" get secret strata-observer-records-0 -o jsonpath='{.data.meta\\.json}' | base64 -d | jq -e --arg c \"\$C\" '.baseline == \"first-seen\" and .firstSeenFrom == \$c and (has(\"revision\") | not)'"
fi

# ---- the pods and a mirror (stage 7) ---------------------------------------------------------------

# The registry is a container on the network of kind. A node reaches it by its name. vendor.example
# stands for the host of a vendor: containerd on each node asks the registry first for it. The image is
# one small image under two names. mirror-id names vendor.example, as a pack does, and the node mirror
# serves it. mirror-string names the registry, as an admission rule that rewrites the string does.
vendor=vendor.example
mirror_pods="mirror-id mirror-string"

# mirror_image POD: the image that the pod names
mirror_image() {
  case "$1" in
    mirror-id) echo "$vendor/product:1.0" ;;
    mirror-string) echo "$registry:5000/product:1.0" ;;
  esac
}
# mirror_registry: start the registry, and push the image product:1.0 to it. The registry needs a
# moment before it takes a push.
mirror_registry() {
  local _
  mirror_made=1
  # a registry of an earlier run that stopped goes first
  docker rm -f "$registry" > /dev/null 2>&1
  docker run -d --name "$registry" --network kind -p "127.0.0.1:$registry_port:5000" registry:2.8.3 \
    && docker pull registry.k8s.io/pause:3.10 \
    && docker tag registry.k8s.io/pause:3.10 "localhost:$registry_port/product:1.0" || return 1
  for _ in $(seq 20); do
    docker push "localhost:$registry_port/product:1.0" && return 0
    sleep 1
  done
  return 1
}
# mirror_hosts NODE: write the two files of containerd on NODE. vendor.example goes to the registry
# first. The registry has no TLS, so its own name needs a file that says http.
mirror_hosts() {
  local node=$1
  docker exec "$node" grep -q config_path /etc/containerd/config.toml \
    || { echo "$node: containerd reads no folder certs.d"; return 1; }
  docker exec -i "$node" sh -c "mkdir -p /etc/containerd/certs.d/$vendor && cat > /etc/containerd/certs.d/$vendor/hosts.toml" << EOF
server = "https://$vendor"

[host."http://$registry:5000"]
  capabilities = ["pull", "resolve"]
EOF
  docker exec -i "$node" sh -c "mkdir -p /etc/containerd/certs.d/$registry:5000 && cat > /etc/containerd/certs.d/$registry:5000/hosts.toml" << EOF
server = "http://$registry:5000"

[host."http://$registry:5000"]
  capabilities = ["pull", "resolve"]
EOF
}
mirror_nodes() {
  local node
  for node in $(kind get nodes --name "$cluster"); do
    mirror_hosts "$node" || return 1
  done
}
# mirror_pod POD: a pod in the namespace default that names the image of POD, and waits to be Ready
mirror_pod() {
  kc apply --server-side -f - << EOF || return 1
apiVersion: v1
kind: Pod
metadata:
  name: $1
  namespace: default
spec:
  containers:
    - name: pause
      image: $(mirror_image "$1")
      imagePullPolicy: Always
EOF
  kc -n default wait --for=condition=Ready "pod/$1" --timeout=120s
}
mirror_pods_ready() {
  local pod
  for pod in $mirror_pods; do
    mirror_pod "$pod" || return 1
  done
}
# component_apply: examples/observer-pods, which lists the base and the component deploy/observer/pods
# as a team does. The first cycle of the new pod reads the pods, so the pods of the mirror are in the
# first read.
component_apply() {
  kubectl kustomize "$root/examples/observer-pods" | sed "s#ghcr.io/como-technologies/strata-observer:VERSION#$image#" \
    | kc apply --server-side -f - \
    && kc -n "$ns" rollout status deploy/strata-observer --timeout=180s
}
# mirror_report POD: what the pod reports for its image, and the row that the observer shows for the
# string of the pod. It succeeds when the row holds a digest. It exits 2 when the pod or the row is
# missing.
mirror_report() {
  local ref
  kc -n default get pod "$1" -o json > "$1.json" || return 2
  jq -r '.spec.containers[0].image as $ref | .status.containerStatuses[0] | "spec.image:   \($ref)\nstatus.image: \(.image)\nimageID:      \(.imageID)"' "$1.json"
  ref=$(jq -r '.spec.containers[0].image' "$1.json")
  page "/v1/images/$(uri "$ref")" > "$1-row.json" \
    || { echo "the observer shows no row for $ref"; return 2; }
  jq '.image | {ref, status, match, digests}' "$1-row.json"
  jq -e '[.image.digests[] | select(.digest != null)] | length > 0' "$1-row.json" > /dev/null
}
# mirror_remove: the pods, the files of containerd, and the registry go. It is safe to run again.
mirror_remove() {
  local node pod bad=0
  for pod in $mirror_pods; do
    kc -n default delete pod "$pod" --ignore-not-found --wait=false || bad=1
  done
  for node in $(kind get nodes --name "$cluster" 2> /dev/null); do
    docker exec "$node" rm -rf "/etc/containerd/certs.d/$vendor" "/etc/containerd/certs.d/$registry:5000" || bad=1
  done
  docker rm -f "$registry" || bad=1
  mirror_made=
  return "$bad"
}

if stage 7 "the pods and a mirror"; then
  check mirror-registry 0 '#646: a registry on the network of kind holds product:1.0' -- mirror_registry
  check mirror-nodes 0 '#646: containerd on each node asks the registry first for vendor.example' -- mirror_nodes
  check mirror-pods 0 '#646: a pod that names vendor.example and a pod that names the registry are Ready' -- mirror_pods_ready
  check component-apply 0 '#646: deploy/observer/pods applies, and the Deployment becomes available' -- component_apply
  check component-read 0 '#646: the observer reads the pods through the token of the component: /v1/images says read' -- pods_read
  check mirror-image-id ? '#646: the pod that names vendor.example: what it reports, and the row of the observer. 0 says the row holds a digest' -- \
    mirror_report mirror-id
  check mirror-image-string ? '#646: the pod that names the registry: what it reports, and the row of the observer. 0 says the row holds a digest' -- \
    mirror_report mirror-string
  pf_stop
  check mirror-clean 0 '#646: the pods, the files of containerd, and the registry go' -- mirror_remove
fi

# ---- the alerts (stage 8) ---------------------------------------------------------------------------

# The receiver is a pod of the namespace default. busybox httpd runs a CGI script for a POST to
# /cgi-bin/hook, and the script appends the body to /data/bodies.log. The pod listens on port 443, so
# the network policy of the observer, which lets it connect to DNS and to port 443, lets a message
# through. The address uses http, so the observer runs with --alerts-allow-http. The rules are the file
# of examples/observer-alerts, and the content cycle gets a short period.
receiver=alert-receiver
receiver_url=http://$receiver.default.svc.cluster.local:443/cgi-bin/hook

# alert_profile: the add-on profile, as name@version. Its record is addon.json.
alert_profile() { jq -r '.source.name + "@" + .source.version' addon.json; }
# alert_receiver: the CGI script in a ConfigMap, the pod, and a Service on port 443. The script reads the
# bytes of the body, as many as CONTENT_LENGTH says, and writes one line for each message.
alert_receiver() {
  cat > hook.cgi << 'EOF'
#!/bin/sh
head -c "$CONTENT_LENGTH" >> /data/bodies.log
echo >> /data/bodies.log
printf 'Status: 200 OK\r\nContent-Type: text/plain\r\n\r\nok\r\n'
EOF
  kc -n default create configmap "$receiver" --from-file=hook=hook.cgi || return 1
  kc apply --server-side -f - << EOF || return 1
apiVersion: v1
kind: Pod
metadata:
  name: $receiver
  namespace: default
  labels:
    app: $receiver
spec:
  containers:
    - name: httpd
      image: busybox:1.37
      command: [httpd, -f, -p, "443", -h, /www]
      volumeMounts:
        - { name: cgi, mountPath: /www/cgi-bin }
        - { name: data, mountPath: /data }
  volumes:
    - name: cgi
      configMap: { name: $receiver, defaultMode: 0755 }
    - name: data
      emptyDir: {}
---
apiVersion: v1
kind: Service
metadata:
  name: $receiver
  namespace: default
spec:
  selector:
    app: $receiver
  ports:
    - { port: 443, targetPort: 443 }
EOF
  kc -n default wait --for=condition=Ready "pod/$receiver" --timeout=120s
}
# alert_secret: the Secret of the addresses. The two channels of the example name the receiver.
alert_secret() {
  kc -n "$ns" create secret generic strata-observer-alert-channels \
    --from-literal=platform-oncall="$receiver_url" --from-literal=platform-planning="$receiver_url"
}
# alert_overlay: base/ is a copy of deploy/observer, alerts/ is a copy of the component, and
# kustomization.yaml lists both as a team does, with the file of the example. The patch gives the pod a
# short period and the flag that allows an http address.
alert_overlay() {
  mkdir -p base alerts && cp "$root"/deploy/observer/*.yaml base/ && cp "$root"/deploy/observer/alerts/*.yaml alerts/ \
    && cp "$root/examples/observer-alerts/alerts.json" . || return 1
  cat > kustomization.yaml << 'EOF'
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization
namespace: strata-observer
resources:
  - base
components:
  - alerts
configMapGenerator:
  - name: strata-observer-alerts
    files:
      - alerts.json
generatorOptions:
  disableNameSuffixHash: true
patches:
  - target:
      kind: Deployment
      name: strata-observer
    patch: |-
      - op: add
        path: /spec/template/spec/containers/0/args/-
        value: --content-every=1m
      - op: add
        path: /spec/template/spec/containers/0/args/-
        value: --alerts-allow-http
EOF
}
# alert_apply: render the overlay, make sure that the flag --alerts is in it, and apply it with the image of this run
alert_apply() {
  alert_overlay && kubectl kustomize . | sed "s#ghcr.io/como-technologies/strata-observer:VERSION#$image#" > observer.yaml \
    && grep -q -e '- --alerts=/alerts/alerts.json' observer.yaml && kc apply --server-side -f observer.yaml \
    && kc -n "$ns" rollout status deploy/strata-observer --timeout=180s
}
# alert_set: /v1/alerts says that the observer sends alerts, with the five rules of the example. alerts-page.json keeps it.
alert_set() { page /v1/alerts > alerts-page.json 2> /dev/null && jq -e '.set == true and (.rules | length) == 5' alerts-page.json > /dev/null; }
# alert_route: open the port-forward, wait for the route, and show it. The route names the channels, and it shows no address.
alert_route() {
  pf_start || { cat port-forward.log; return 1; }
  poll "$tries" alert_set || { cat alerts-page.json 2> /dev/null; return 1; }
  jq '{set, channels, digestEvery, waiting, rules: [.rules[] | {level, match, news, send, to, state}]}' alerts-page.json
  case "$(cat alerts-page.json)" in
    *"$receiver"*) echo "the route shows the address"; return 1 ;;
  esac
}
# alert_finding: the finding published-changed names the add-on profile and a pack. findings.json keeps it.
alert_finding() {
  page /v1/findings > findings.json 2> /dev/null \
    && jq -e --arg p "$(alert_profile)" '.findings[] | select(.kind == "published-changed" and .profile == $p and (.packs | length) > 0)' findings.json > /dev/null
}
# alert_no_finding: /v1/findings is empty
alert_no_finding() { page /v1/findings > findings.json 2> /dev/null && jq -e '.findings | length == 0' findings.json > /dev/null; }
# alert_edit: an edit in place of the add-on version. The flag comes first, so that the undo at the end knows
# of the edit also when the call fails.
alert_edit() {
  edited=1
  "$live/api.sh" edit "$AD" "$addon_from" "$addon_to" && poll "$tries" alert_finding
}
# alert_texts: the body of each message that the receiver got, one JSON object on each line
alert_texts() { kc -n default exec "$receiver" -- cat /data/bodies.log; }
# count_messages REGEX: the messages whose text matches REGEX
count_messages() { alert_texts | jq -s --arg re "$1" '[.[] | select(.text | test($re))] | length'; }
# open_regex and clear_regex: the lines that a message of the open and a message of the clear start with after the cluster
open_regex() { printf '· Drift\n%s: ' "$(alert_profile)"; }
clear_regex() { printf '· Drift cleared\n%s\n' "$(alert_profile)"; }
alert_opened() { [ "$(count_messages "$(open_regex)")" -ge 1 ] 2> /dev/null; }
alert_cleared() { [ "$(count_messages "$(clear_regex)")" -ge 1 ] 2> /dev/null; }
# alert_open: the receiver gets one message of the open, with the name of the cluster. The output shows it.
alert_open() {
  local name
  poll "$tries" alert_opened || { alert_texts | jq -c .; return 1; }
  name=$(page /v1/cluster | jq -r '.cluster.name') && [ -n "$name" ] || return 1
  alert_texts | jq -e --arg c "$name" --arg re "$(open_regex)" 'select(.text | test($re)) | .text | startswith("strata · " + $c + " · Drift\n")' > /dev/null \
    || { echo "no message starts with the name of the cluster, $name"; return 1; }
  alert_texts | jq -r --arg re "$(open_regex)" 'select(.text | test($re)) | .text'
  # one finding is one message
  [ "$(count_messages "$(open_regex)")" = 1 ]
}
# alert_no_value: the detail route holds the text of the edit, so the search proves something. The receiver
# holds none of it, in the raw form or in the JSON form with the quotes escaped, and none of the text it replaced.
alert_no_value() {
  local profile pack body marker encoded
  profile=$(alert_profile)
  pack=$(jq -r --arg p "$profile" '[.findings[] | select(.kind == "published-changed" and .profile == $p) | .packs[]] | first // empty' findings.json)
  [ -n "$pack" ] || { echo "the finding names no pack"; return 1; }
  curl -fsS --max-time 60 "http://127.0.0.1:$port/v1/packs/$profile/$pack/diff?side=published" > diff.json \
    || { echo "no answer of the detail route for $pack"; return 1; }
  jq -e --arg m "$addon_to" '[.values.unified, (.manifests[]? | .unified)] | map(select(type == "string")) | any(contains($m))' diff.json > /dev/null \
    || { echo "the detail route holds no text of the edit for $pack, so the search proves nothing"; return 1; }
  echo "the detail route holds the text of the edit for $pack"
  body=$(alert_texts) || return 1
  for marker in "$addon_to" "$addon_from"; do
    encoded=$(jq -rn --arg m "$marker" '$m | tojson | .[1:-1]') || return 1
    case "$body" in
      *"$marker"* | *"$encoded"*) echo "a message holds: $marker"; return 1 ;;
    esac
  done
  echo "$(printf '%s\n' "$body" | grep -c .) message(s) hold no text of the edit"
}
# alert_undo: restore the add-on version, and wait for the cycle that finds no finding
alert_undo() {
  "$live/api.sh" edit "$AD" "$addon_to" "$addon_from" && edited= && poll "$tries" alert_no_finding
}
# alert_clear: the receiver gets a message of the clear, after a check that proves it. The output shows it.
alert_clear() {
  poll "$tries" alert_cleared || { alert_texts | jq -c .; return 1; }
  alert_texts | jq -r --arg re "$(clear_regex)" 'select(.text | test($re)) | .text'
}
# checked_at: the end of the last content cycle
checked_at() { page /v1/cluster | jq -r '.content.checkedAt'; }
alert_next_cycle() { [ "$(checked_at)" != "$1" ]; }
# alert_one_each: after one more content cycle, the receiver holds one message of the open and one of the clear
alert_one_each() {
  local before opens clears
  before=$(checked_at) || return 1
  poll "$tries" alert_next_cycle "$before" || { echo "no content cycle ended after the clear"; return 1; }
  opens=$(count_messages "$(open_regex)") && clears=$(count_messages "$(clear_regex)") || return 1
  echo "$(alert_profile): $opens message of the open, $clears message of the clear"
  [ "$opens" = 1 ] && [ "$clears" = 1 ]
}
# alert_clean: the receiver goes. The kind cluster goes in stage 9.
alert_clean() { kc -n default delete pod,service,configmap "$receiver" --ignore-not-found; }

if stage 8 "the alerts"; then
  check alert-receiver 0 '#648: a pod takes a POST at /cgi-bin/hook on port 443 and keeps the body of each message' -- alert_receiver
  check alert-secret 0 '#648: the Secret strata-observer-alert-channels holds the address of the receiver for each channel of the example' -- alert_secret
  check alert-apply 0 '#648: the base with the component deploy/observer/alerts and the file of examples/observer-alerts applies, and the Deployment becomes available' -- alert_apply
  check alert-route 0 '#648: /v1/alerts says set, lists the five rules with their states and the names of the channels, and shows no address' -- alert_route
  check alert-edit 0 '#648: an edit in place of the add-on version opens a finding published-changed' -- alert_edit
  check alert-open 0 '#648: the receiver gets a message of the open: the cluster, Drift, the profile, and its packs' -- alert_open
  check alert-no-value 0 '#648: the detail route holds the text of the edit, and the receiver holds none of it' -- alert_no_value
  check alert-undo 0 '#648: the add-on version goes back, and the observer finds no finding' -- alert_undo
  check alert-clear 0 '#648: the receiver gets a message of the clear' -- alert_clear
  check alert-one-each 0 '#648: after one more content cycle, the receiver holds one message of the open and one of the clear' -- alert_one_each
  pf_stop
  check alert-clean 0 '#648: the receiver, its Service, and its ConfigMap go' -- alert_clean
fi

if stage 9 "the end: the kind cluster goes"; then
  if [ "${KEEP_CLUSTER:-}" = 1 ]; then
    echo "KEEP_CLUSTER=1: the kind cluster $cluster stays" | tee -a steps.log
  else
    check kind-delete 0 '#644: the kind cluster is deleted' -- kind delete cluster --name "$cluster"
    deleted=1
  fi
fi

echo "observer-kind: $failures check(s) failed; results in $run"
[ "$failures" = 0 ]
