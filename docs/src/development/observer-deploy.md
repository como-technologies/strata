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

The image is in a private repository. A team that pulls it names a Secret of
the registry in `imagePullSecrets`.

## The manifests

`kubectl kustomize deploy/observer` renders four objects.

| File | Object | What it does |
| --- | --- | --- |
| `namespace.yaml` | `Namespace` | Sets `pod-security.kubernetes.io/enforce: restricted`. |
| `serviceaccount.yaml` | `ServiceAccount` | Has no role. The pod gets no token, because the observer calls no Kubernetes API. |
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

A team makes two objects that the kustomization does not hold:

- The ConfigMap `strata-observer` with the keys `cluster-uid`, `palette-api`,
  and `project-uid`.
- The Secret `strata-observer-key` with the key `api-key`.

A kustomization of the team sets the image tag. The key of Palette is never a
profile variable. A profile variable is in clear in the copy of the cluster.

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
just observer-image         # strata-observer:dev, from the static binary of this tree
just live-observer-kind     # the observer on kind
just live-observer          # the observer on cluster A, from a kubectl apply and from a profile
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

`tests/observer_deploy.rs` ties each manifest to the code of the observer and
to the records script. `tests/ci_scripts.rs` tests the script with a stand-in
for `strata`. `tests/workflows.rs` tests the steps of the workflow.

## Check a real cluster

`just live-observer` runs [`observer.sh`](./live-runs.md#observersh) on cluster
A of the live example. It needs `kubectl`, `gh`, and `go`. `OBSERVER_IMAGE`
names the image. The default is the image of the version in `Cargo.toml`.

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
- A wrong key sends the page to `lost`, and the right key brings it back to
  `ok`. A restart starts the state again.
- A profile of the observer shows as `no-record` under "Not checked". It is not a
  drift, and the other profiles of the cluster stay the same.

The script counts the requests of a content cycle and of a versions cycle.
`count-requests` sits between `strata` and Palette, and writes one line for each
request. The lines hold the method and the path, and no query and no key.

The undo runs at the end, also after a failed check. The edit goes back, the
profile comes off the cluster, and the namespace goes. The script deletes no
cluster. `just live-down` deletes it.
