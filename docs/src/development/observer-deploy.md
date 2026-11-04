# The image and the manifests of the observer

The observer ships as a signed image with the manifests in `deploy/observer/`.
The pipeline of a team writes the records into four Secrets with
`scripts/ci/observer-records.sh`. This page is for a maintainer. It tells what
each file does and how to check it. For the code, see [The checks of the
observer](./observer.md).

## The image

`deploy/observer/Dockerfile` builds the image in two stages. The first stage
builds `strata-observer` from `observer/`, for the platform of the image. The
second stage is `distroless/static`, with the `strata` binary of one release
beside the observer. Each base has a digest in the file, and the version of Go
is the version of `observer/go.mod`. The image has no shell, so a probe runs
`strata-observer health` in the container.

The image holds the binary of a release, never a build of the same commit. The
binary is the static `musl` build that the release signed. The version of the
image is the version of that binary.

The workflow `observer-image.yml` builds the image for `linux/amd64` and
`linux/arm64`, and pushes it to `ghcr.io/como-technologies/strata-observer`.
It takes two inputs. `version` is the release of the binaries, such as
`v0.29.0`, or `latest`. `tag` is the tag of the image.

| Start | `version` | `tag` |
| --- | --- | --- |
| The job `observer-image` of `release.yml`, after the release is published | The tag of the release | The tag of the release |
| A pull request that changes `observer/`, `deploy/observer/`, or the workflow | `latest` | `test-` and the first seven characters of the commit |
| A manual run (`workflow_dispatch`) | `latest`, or your value | Your value, or the tag of a pull request |

A manual run starts only when the workflow is on the default branch, so it
cannot test a branch. Use it after a merge to build a test tag of `main`. A
test tag stays in the registry. Delete it when the check ends.

The workflow does these steps in order:

1. Install `cosign`, and install the binaries of both platforms with
   `install-strata.sh --bin-only --require-signature`. The installer checks the
   signature and the checksums. The workflow has no copy of that check.
2. Build both platforms and push the image.
3. Make sure that the image lists both platforms.
4. Sign the digest with `cosign sign`, and check the signature with `cosign
   verify`. The check names the workflow that signs.

To check a signature by hand, give the digest of the image:

```bash
cosign verify ghcr.io/como-technologies/strata-observer@sha256:<digest> \
  --certificate-identity-regexp '^https://github.com/como-technologies/strata/\.github/workflows/observer-image\.yml@' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
```

> **Set the permissions of the job that calls the workflow.** A called workflow
> cannot have more permission than the job that calls it. When the job grants
> less, GitHub refuses the whole run at its start, and the release does not ship.
> The job `observer-image` of `release.yml` sets `contents: read`, `packages:
> write`, and `id-token: write`. `tests/workflows.rs` requires the three.

The image is in a private repository. A team that pulls it makes a Secret of the
registry and names it in `imagePullSecrets`. The token in that Secret needs only the
scope `read:packages`. Steps 2 and 6 of [Install](../automation/observer.md#install)
make the Secret and name it.

## The manifests

`kubectl kustomize deploy/observer` renders four objects.

| File | Object | What it does |
| --- | --- | --- |
| `namespace.yaml` | `Namespace` | Sets `pod-security.kubernetes.io/enforce: restricted`. |
| `serviceaccount.yaml` | `ServiceAccount` | Has no role. The pod gets no token, because the base calls no Kubernetes API. [The component](#the-component-that-reads-the-pods) adds both. |
| `networkpolicy.yaml` | `NetworkPolicy` | Lets no pod connect to the observer. The observer connects to DNS and to HTTPS only. A team can narrow the HTTPS rule to the address of Palette. |
| `deployment.yaml` | `Deployment` | Runs one pod with `Recreate`, as non-root, with a read-only root and no capability. |

`reader-role.yaml` is outside the kustomization, because a team binds it to its
own group. The role gives `get` on the Deployment, `get` and `list` on pods, and
`create` on `pods/portforward`. It gives no Secret and no `exec`. A person with
this role opens the page with `kubectl port-forward deploy/strata-observer
8080`.

The pod mounts three volumes:

- `records` is a projected volume of the four Secrets
  `strata-observer-records-0` to `strata-observer-records-3`. It is read-only,
  and each Secret is optional.
- `work` is in memory. A cycle writes the values of a cluster there and removes
  them.
- `state` is on the disk of the node. It holds no value.

A team makes three objects that the kustomization does not hold:

- The pull Secret `ghcr-pull`, of the type `kubernetes.io/dockerconfigjson`.
- The ConfigMap `strata-observer` with the keys `cluster-uid`, `palette-api`,
  and `project-uid`.
- The Secret `strata-observer-key` with the key `api-key`.

A kustomization of the team sets the image tag and a patch that adds
`imagePullSecrets`. The key of Palette is never a profile variable. A profile
variable is in clear in the copy of the cluster.

## The component that reads the pods

The base of `deploy/observer` calls no Kubernetes API. The component
`deploy/observer/pods/` lets the observer read the pods of the cluster, for the
digest of each pod image. It is optional. A team lists it under `components:` of
its own kustomization:

```yaml
resources:
  - deploy/observer
components:
  - deploy/observer/pods
```

`deploy/observer/pods/example/kustomization.yaml` is such a kustomization.
`just observer-manifests-pods` renders it, and makes sure that it holds six
objects and the flag `--read-pods`.

| File | What it adds |
| --- | --- |
| `clusterrole.yaml` | A `ClusterRole` with one rule: `get` and `list` on `pods`. It has no other verb, resource, or group. |
| `clusterrolebinding.yaml` | A binding of that role to the service account `strata-observer` of the namespace `strata-observer`. |
| `kustomization.yaml` | A patch of the Deployment and a patch of the NetworkPolicy. |

The patch of the Deployment adds the flags `--read-pods` and `--kube-dir`. It
adds a `projected` volume, mounted read-only at
`/var/run/secrets/kubernetes.io/serviceaccount`. The volume holds a token of the
service account and `ca.crt` of the ConfigMap `kube-root-ca.crt`. The token
expires in 3600 seconds, and the kubelet renews it. The observer reads both files
again at each call.

The pod keeps `automountServiceAccountToken: false`. The volume of the component
is the only token in the pod, and its name does not start with `kube-api-access`.

The patch of the NetworkPolicy adds an egress rule for TCP 6443 beside 443. The
service of the API server uses port 443. A CNI can apply the policy after the
address of the service changes to the address of a node, and that address uses
port 6443. The lab cluster runs Cilium.

> **Ask the owner of the cluster before you list the component.** The verb `list`
> on pods gives the whole spec of each pod to the holder of the token. The spec
> holds the `env` of each container. The observer decodes no `env`, but the token
> can read it.

A delete of the namespace does not delete the role or the binding, because they
are not in a namespace. Delete both by name:

```bash
kubectl delete clusterrolebinding,clusterrole strata-observer-pods
```

With the component, the observer shows the digest that each pod runs, and the
findings `image-moved` and `image-split`. With no component, `/v1/images` says
that the observer does not read pods, and the observer makes no call to
Kubernetes.

## The records

`observer-records.sh` writes the records as Secrets on standard output. It
calls no Kubernetes API, so the pipeline applies the output:

```bash
scripts/ci/observer-records.sh --revision "$GIT_SHA" releases/*.json \
  | kubectl apply --server-side -f -
scripts/ci/observer-records.sh --first-seen <cluster uid> \
  | kubectl apply --server-side -f -
```

Use `--server-side`. A client-side apply copies each Secret to an annotation,
and that annotation has a limit of 256 KiB.

| Option | Meaning |
| --- | --- |
| `--revision REV` | The records are the files that you name. `REV` is the Git revision that a review approved. `meta.json` says `approved`. |
| `--first-seen UID` | The records are what each profile of the cluster holds now. No person approved that content, so `meta.json` says `first-seen`, and each screen of the observer says so. |
| `--namespace NAME` | The namespace of the Secrets. The default is `strata-observer`. |

The script gzips each record. It puts the records in four Secrets, each in the
first Secret that has room, with a limit of 900 000 characters for each Secret.
`meta.json` is in the first Secret. A Secret with no record is empty, and the
apply then removes the content that an older apply put there.

The script stops with exit `2`, and writes nothing to standard output, in these
cases:

| Case | Message |
| --- | --- |
| No flag, or both flags | `give --revision or --first-seen, and not both` |
| `--first-seen` with a file | `--first-seen takes no record file` |
| A file that is not a record of a profile | `<file> is not a profile record` |
| Two records of one profile version | `two records are for <key>` |
| No record | `no record` |
| Records that do not fit in four Secrets | `the records do not fit in four Secrets: <key>` |

A record of a cluster is never in a Secret. It holds the clear values of the
variables, so the script refuses it. `--first-seen` reads the cluster, and it
writes only the records of its profiles.

## Check the files

```bash
just observer-manifests     # kubectl kustomize deploy/observer holds four objects
just observer-manifests-pods # the same with the component: six objects and the flag --read-pods
just observer-image         # strata-observer:dev, from the static binary of this tree
just live-observer-kind     # the observer on kind
just live-observer          # the observer on cluster A, from a kubectl apply and from a profile
just live-observer-images   # the observer on cluster A reads the digest of each pod image
```

`just observer-image` needs `cross` and Docker. `just live-observer-kind` runs
[`observer-kind.sh`](./live-runs.md#observer-kindsh) on a kind cluster. It makes
sure of these facts:

- The pod is Ready, runs as non-root, has a read-only root, and has no token.
- The output of the records script applies with `--server-side`, also for
  `--first-seen`.
- The page answers through `kubectl port-forward deploy/strata-observer` for a
  user with only the reader role.
- Another pod does not reach the page.
- A mirror for `vendor.example` serves a pod, and the component lets the observer read
  the pods (stage 7).

`tests/observer_deploy.rs` ties each manifest and the component to the code of the
observer and to the records script. `tests/ci_scripts.rs` tests the script with a stand-in
for `strata`. `tests/workflows.rs` tests the steps of the workflow.

## Check a real cluster

`just live-observer` runs [`observer.sh`](./live-runs.md#observersh) on cluster
A of the live example. It needs `kubectl`, `gh`, `go`, and a token file for the
pull Secret. The file holds a token with only the scope `read:packages`.
`OBSERVER_IMAGE` names the image. The default is the image of the version in
`Cargo.toml`.

The script installs the observer in two ways, one after the other. Way 1 is a
plain `kubectl apply` of the output of `kubectl kustomize`. An overlay in the
folder of the run sets the image, the pull Secret, and two short periods. Way 2
is an add-on profile in Palette. The profile holds the rendered manifests as one
manifest layer. The namespace and the Secrets are made with `kubectl` first,
because the key must not be in a profile.

The script makes sure of these facts:

- The first cycle of the pod reaches Palette from a node of the cluster.
- The page holds no finding at the baseline, and its status of each profile is
  the status of `strata verify` and `strata compare`.
- A user with only the reader role opens the page. A user with no role does not.
  A pod of another namespace cannot connect.
- An edit in place of the add-on version gives the finding `published-changed`
  with the actor. No route and no line of the log shows the value of a variable.
- A wrong key sends the page to `lost`, and the right key returns the page to
  `ok`. A restart starts the state again.
- A profile of the observer shows as `no-record` under "Not checked". It is not a
  drift, and the other profiles of the cluster stay the same.

The script counts the requests of a content cycle and of a versions cycle.
`count-requests` sits between `strata` and Palette, and writes one line for each
request. The lines hold the method and the path, and no query and no key.

The undo runs at the end, also after a failed check. The edit goes back, the
profile leaves the cluster, and the namespace goes. The script deletes no
cluster. `just live-down` deletes it.

## Check the digests on a real cluster

`just live-observer-images` runs
[`observer-images.sh`](./live-runs.md#observer-imagessh) on cluster A. It needs
`kubectl`, `gh`, `crane`, and a token file for the pull Secret. `OBSERVER_IMAGE`
names the image of the observer, and the image must hold the code of the commit
that you test. The script installs the observer with the component. Cluster A is
billable until `just live-down`.

The image of the observer stays on its private package. The nodes pull it with the
Secret `ghcr-pull`, as in [Check a real cluster](#check-a-real-cluster). The script
never pushes that image to another registry. A pull request that changes the
observer builds the image with the tag `test-<short sha>`.

A throwaway tag on `ttl.sh` holds one image, and then another. The images on
`ttl.sh` are copies of public images. The script runs pods on the tag and makes
sure of these facts:

- The first pod shows the first digest, and no finding names the tag.
- A new pod on the moved tag gives the finding `image-split`. When the first pod
  is replaced, the finding is `image-moved`.
- A container that never started reads Not checked. A container in a crash loop
  keeps its digest.
- The images that match no pack, such as the images of the agents of Palette,
  make no finding.
- The token of the observer gets and lists pods, and does nothing else.

The check `pull-policy` shows what the cluster does with a pod made with
`IfNotPresent`. The undo removes the pods, the observer, the role, and the
binding, and it puts the values of the add-on back.

[`observer-kind.sh`](./live-runs.md#observer-kindsh) has a stage with a local
registry and a containerd mirror for `vendor.example`. It shows how pods reach a
mirror.
