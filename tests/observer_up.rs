//! The install of the observer on a test cluster: `examples/live/observer-up.sh` and the recipes
//! `observer-up`, `observer-open`, and `observer-down`. No cluster and no Palette: a stand-in
//! `kubectl`, `strata`, `gh`, and `api.sh` stand for them, and the script runs from a copy of the tree.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::common::{
    CommandExt, FAKE_PORT_FORWARD, copy_scripts, executable, fake_bin, on_path, path_with,
    repo_file, text, walk,
};

/// The key of Palette and the pull token of a run. A run must show neither in an argument of
/// `kubectl` or in a file.
const KEY_MARKER: &str = "key-marker-8f3a1c";
const TOKEN_MARKER: &str = "token-marker-91bc7e";

/// The `api.sh` of the tree: it gives a kubeconfig for a cluster uid, and logs the call with the
/// project of the request. `FAKE_NO_KUBECONFIG` makes Palette refuse.
const FAKE_API: &str = r##"#!/usr/bin/env bash
echo "$* project=${PALETTE_PROJECT_UID-<unset>}" >> "$FAKE/api.log"
case "$1" in
  get-kubeconfig)
    [ -z "${FAKE_NO_KUBECONFIG-}" ] || { echo "no kubeconfig for $2: HTTP 404" >&2; exit 2; }
    printf 'apiVersion: v1\nkind: Config\nclusters: []  # the kubeconfig of %s\n' "$2" ;;
esac
"##;

/// The `strata` of the run. `list clusters` gives `$FAKE/list.json`. A snapshot of the cluster lists
/// one profile, and a snapshot of the profile is a record of it.
const FAKE_STRATA: &str = r##"#!/usr/bin/env bash
echo "$*" >> "$FAKE/strata.log"
out=""; prev=""
for a in "$@"; do [ "$prev" = -o ] && out=$a; prev=$a; done
case "$1 $2" in
  "list clusters") cat "$FAKE/list.json" ;;
  "snapshot cluster:"*)
    jq -n --arg u "${2#cluster:}" '{schema: "strata-snapshot/1",
      source: {cluster: {uid: $u, profiles: [{name: "aws-stack", version: "1.0.0"}]}}}' > "$out" ;;
  "snapshot aws-stack@1.0.0")
    jq -n '{schema: "strata-snapshot/1", source: {name: "aws-stack", version: "1.0.0"}}' > "$out" ;;
  *) echo "unexpected strata call: $*" >&2; exit 3 ;;
esac
"##;

/// The `gh` of the run: the user of the pull Secret, and the newest release (`v9.9.9`).
/// `FAKE_NO_RELEASE` makes the read of the release fail.
const FAKE_GH: &str = r##"#!/usr/bin/env bash
echo "$*" >> "$FAKE/gh.log"
case "$*" in
  "api user --jq .login") echo tester ;;
  "release view --json tagName --jq .tagName")
    [ -z "${FAKE_NO_RELEASE-}" ] || { echo "no release" >&2; exit 1; }
    echo v9.9.9 ;;
  *) echo "unexpected gh call: $*" >&2; exit 3 ;;
esac
"##;

/// The `kubectl` of the run. It logs each call with the `KUBECONFIG` of the call to `$FAKE/args.log`,
/// and it looks in `$HOME` and `$TMPDIR` for the two markers at each call: a file that holds one is
/// in `$FAKE/leak.log`. What a `create` reads from standard input goes to `$FAKE/stdin.log`, and what
/// an `apply` sends goes to `$FAKE/applied.yaml`: both stand for the cluster. The state of the cluster
/// is in the files `ns`, `deploy`, `clusterrole`, and `clusterrolebinding` of `$FAKE`. A delete of a
/// name that is absent fails with `NotFound` unless `--ignore-not-found` is there. A delete with a
/// selector never fails, as `kubectl` does.
///
/// A port-forward runs [`FAKE_PORT_FORWARD`].
const FAKE_KUBECTL: &str = r##"#!/usr/bin/env bash
fake=$FAKE
echo "[${KUBECONFIG-}] $*" >> "$fake/args.log"
grep -rlF -e "$KEY_MARKER" -e "$TOKEN_MARKER" "$HOME" "$TMPDIR" >> "$fake/leak.log" 2> /dev/null
notfound() { echo "Error from server (NotFound): $1 \"$2\" not found" >&2; exit 1; }
case "$*" in
  "kustomize .")
    rm -rf "$fake/overlay"; cp -r . "$fake/overlay"
    for f in base/*.yaml; do cat "$f"; echo "---"; done
    if [ -d pods ]; then for f in pods/*.yaml; do cat "$f"; echo "---"; done; fi ;;
  *"create "*"--dry-run=client -o yaml"*)
    all="$*"; kind=${all#*create }; kind=${kind%% *}
    body=$(cat)
    if [ "$kind" = secret ]; then
      printf '%s' "$body" >> "$fake/stdin.log"
      echo "kind: Secret"
    else
      echo "kind: ConfigMap"; echo "data:"
      for a in "$@"; do case "$a" in --from-literal=*) kv=${a#--from-literal=}; echo "  ${kv%%=*}: ${kv#*=}" ;; esac; done
    fi ;;
  "apply -f -")
    cat >> "$fake/applied.yaml"; echo "---" >> "$fake/applied.yaml" ;;
  "apply --server-side --force-conflicts -f -")
    cat > "$fake/records.yaml" ;;
  *"apply -f "*"/namespace.yaml") touch "$fake/ns" ;;
  "apply -f observer.yaml")
    touch "$fake/ns" "$fake/deploy"
    ! grep -qx 'kind: ClusterRole' observer.yaml || touch "$fake/clusterrole"
    ! grep -qx 'kind: ClusterRoleBinding' observer.yaml || touch "$fake/clusterrolebinding" ;;
  *"rollout restart"*|*"rollout status"*)
    [ -f "$fake/deploy" ] || notfound deployments.apps strata-observer ;;
  *"delete namespace strata-observer"*)
    if [ -f "$fake/ns" ]; then
      rm -f "$fake/ns" "$fake/deploy"; echo 'namespace "strata-observer" deleted'
    else
      case "$*" in *--ignore-not-found*) ;; *) notfound namespaces strata-observer ;; esac
    fi ;;
  *"delete clusterrole,clusterrolebinding"*)
    case "$*" in *"-l app.kubernetes.io/name=strata-observer"*) ;; *) echo "delete without the label" >&2; exit 1 ;; esac
    found=
    for kind in clusterrole clusterrolebinding; do
      [ ! -f "$fake/$kind" ] || { rm -f "$fake/$kind"; echo "$kind \"strata-observer-pods\" deleted"; found=1; }
    done
    [ -n "$found" ] || echo "No resources found" ;;
  *"get deploy/strata-observer -o name"*)
    [ -f "$fake/deploy" ] || notfound deployments.apps strata-observer
    echo deployment.apps/strata-observer ;;
  *port-forward*) exec fake-port-forward ;;
  *) echo "unexpected kubectl call: $*" >&2; exit 3 ;;
esac
"##;

/// A run of the script in a tree of its own, with the stand-ins. The tree holds the script, the files
/// that it sources, the records script, the manifests, and `Cargo.toml`.
struct Up {
    dir: tempfile::TempDir,
}

impl Up {
    /// A run whose project has the clusters `clusters`: each is a name, a uid, and a project uid.
    fn new(clusters: &[(&str, &str, &str)]) -> Self {
        let up = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        let tree = up.path("tree");
        copy_scripts(
            &tree,
            &[
                "examples/live/observer-up.sh",
                "examples/live/lib.sh",
                "examples/live/common.sh",
                "scripts/ci/observer-records.sh",
            ],
        );
        executable(&tree.join("examples/live/api.sh"), FAKE_API);
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"),
            tree.join("Cargo.toml"),
        )
        .unwrap();
        let deploy = Path::new(env!("CARGO_MANIFEST_DIR")).join("deploy/observer");
        for file in walk(&deploy) {
            let to = tree
                .join("deploy/observer")
                .join(file.strip_prefix(&deploy).unwrap());
            std::fs::create_dir_all(to.parent().unwrap()).unwrap();
            std::fs::copy(&file, &to).unwrap();
        }
        fake_bin(
            up.dir.path(),
            &[
                ("kubectl", FAKE_KUBECTL),
                ("strata", FAKE_STRATA),
                ("gh", FAKE_GH),
                ("fake-port-forward", FAKE_PORT_FORWARD),
            ],
        );
        for d in ["home", "tmp", "fake", "secrets"] {
            std::fs::create_dir(up.path(d)).unwrap();
        }
        let token = up.path("secrets/ghcr-pull-token");
        std::fs::write(&token, format!("{TOKEN_MARKER}\n")).unwrap();
        std::fs::set_permissions(&token, std::fs::Permissions::from_mode(0o600)).unwrap();
        let listed: Vec<serde_json::Value> = clusters
            .iter()
            .map(|(name, uid, project)| {
                serde_json::json!({"name": name, "uid": uid, "state": "Running", "projectUid": project})
            })
            .collect();
        up.list(&serde_json::json!({"projectUid": "p-doc", "clusters": listed}));
        up
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }

    /// `strata list clusters --json` gives `document`.
    fn list(&self, document: &serde_json::Value) {
        std::fs::write(self.path("fake/list.json"), document.to_string()).unwrap();
    }

    /// Run the script with `args` and the variables `envs`. The shell of the run points `KUBECONFIG`
    /// at another cluster.
    fn run_with(&self, args: &[&str], envs: &[(&str, &str)]) -> Output {
        let bin = self.path("bin");
        let strata = bin.join("strata");
        let mut command = Command::new("bash");
        command
            .arg(self.path("tree/examples/live/observer-up.sh"))
            .args(args)
            .env_clear()
            .env("PATH", path_with(&bin))
            .env("HOME", self.path("home"))
            .env("TMPDIR", self.path("tmp"))
            .env("FAKE", self.path("fake"))
            .env("STRATA_BIN", &strata)
            .env("PALETTE_API_KEY", KEY_MARKER)
            .env("KEY_MARKER", KEY_MARKER)
            .env("TOKEN_MARKER", TOKEN_MARKER)
            .env("GHCR_PULL_TOKEN_FILE", self.path("secrets/ghcr-pull-token"))
            .env("KUBECONFIG", "/elsewhere/another-cluster.yaml");
        for (key, value) in envs {
            command.env(key, value);
        }
        command.output_locked().unwrap()
    }

    fn run(&self, args: &[&str]) -> Output {
        self.run_with(args, &[])
    }

    /// A file of the stand-ins, or an empty text when it is not there.
    fn log(&self, file: &str) -> String {
        std::fs::read_to_string(self.path("fake").join(file)).unwrap_or_default()
    }

    /// The path of the kubeconfig of the cluster `name`.
    fn kubeconfig(&self, name: &str) -> PathBuf {
        self.path(&format!("home/.kube/strata-{name}.yaml"))
    }

    /// The calls of `kubectl` with the `KUBECONFIG` of each, one text for each call.
    fn kubectl_calls(&self) -> Vec<String> {
        self.log("args.log").lines().map(str::to_string).collect()
    }

    /// The overlay that the last `kustomize` call read: its `kustomization.yaml`.
    fn overlay(&self) -> String {
        self.log("overlay/kustomization.yaml")
    }
}

fn succeeded(o: &Output) {
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}{}",
        text(&o.stdout),
        text(&o.stderr)
    );
}

/// The script stopped with a message on standard error, and no call reached a cluster.
fn stopped_before_the_cluster(up: &Up, o: &Output) -> String {
    assert!(
        !o.status.success(),
        "{}{}",
        text(&o.stdout),
        text(&o.stderr)
    );
    assert!(up.kubectl_calls().is_empty(), "{:?}", up.kubectl_calls());
    assert!(
        up.log("applied.yaml").is_empty(),
        "{}",
        up.log("applied.yaml")
    );
    text(&o.stderr)
}

/// No `ConfigMap` that the cluster got has an empty value.
fn assert_no_empty_config(up: &Up) {
    for line in up.log("applied.yaml").lines() {
        for key in ["cluster-uid", "project-uid"] {
            assert!(
                line.trim() != format!("{key}:"),
                "an empty {key} reached the cluster:\n{}",
                up.log("applied.yaml")
            );
        }
    }
}

#[test]
fn a_name_selects_the_cluster_and_the_uids_of_the_cluster_and_its_project_reach_the_config_map() {
    let up = Up::new(&[("alpha", "c-1", "p-1"), ("beta", "c-2", "p-2")]);
    let o = up.run(&["--cluster", "beta", "--tag", "v0.29.0"]);
    succeeded(&o);
    let applied = up.log("applied.yaml");
    assert!(applied.contains("  cluster-uid: c-2\n"), "{applied}");
    assert!(applied.contains("  project-uid: p-2\n"), "{applied}");
    assert!(!applied.contains("c-1"), "{applied}");
    assert_eq!(up.log("api.log"), "get-kubeconfig c-2 project=p-2\n");
    assert!(up.kubeconfig("beta").is_file());
    assert!(!up.kubeconfig("alpha").exists());
}

#[test]
fn with_no_name_the_one_cluster_of_the_project_is_the_cluster() {
    let up = Up::new(&[("alpha", "c-1", "p-1")]);
    succeeded(&up.run(&["--tag", "v0.29.0"]));
    assert!(up.log("applied.yaml").contains("  cluster-uid: c-1\n"));
    assert!(up.kubeconfig("alpha").is_file());
}

#[test]
fn the_project_of_the_listing_serves_a_cluster_that_names_none() {
    let up = Up::new(&[]);
    up.list(&serde_json::json!({"projectUid": "p-doc",
        "clusters": [{"name": "alpha", "uid": "c-1", "state": "Running"}]}));
    succeeded(&up.run(&["--tag", "v0.29.0"]));
    assert!(up.log("applied.yaml").contains("  project-uid: p-doc\n"));
}

#[test]
fn with_no_name_and_several_clusters_the_script_lists_the_names_and_stops() {
    let up = Up::new(&[("alpha", "c-1", "p-1"), ("beta", "c-2", "p-1")]);
    let o = up.run(&[]);
    let error = stopped_before_the_cluster(&up, &o);
    assert!(error.contains("alpha") && error.contains("beta"), "{error}");
    assert!(error.contains("--cluster NAME"), "{error}");
    assert_eq!(o.status.code(), Some(2));
    assert!(!up.path("home/.kube").exists(), "no kubeconfig is written");
    assert!(up.log("api.log").is_empty());
}

#[test]
fn a_name_that_is_not_in_the_project_stops_the_script_and_names_the_clusters() {
    let up = Up::new(&[("alpha", "c-1", "p-1"), ("beta", "c-2", "p-1")]);
    let o = up.run(&["--cluster", "gamma"]);
    let error = stopped_before_the_cluster(&up, &o);
    assert!(error.contains("no cluster named gamma"), "{error}");
    assert!(error.contains("alpha, beta"), "{error}");
    assert!(up.log("api.log").is_empty());
}

#[test]
fn a_project_with_no_cluster_and_a_name_in_two_projects_stop_the_script() {
    let up = Up::new(&[]);
    let error = stopped_before_the_cluster(&up, &up.run(&[]));
    assert!(error.contains("no cluster"), "{error}");

    let up = Up::new(&[("alpha", "c-1", "p-1"), ("alpha", "c-2", "p-2")]);
    let error = stopped_before_the_cluster(&up, &up.run(&["--cluster", "alpha"]));
    assert!(error.contains("2 clusters are named alpha"), "{error}");
    assert!(error.contains("c-1") && error.contains("c-2"), "{error}");
}

#[test]
fn no_path_applies_a_config_map_with_an_empty_uid_for_the_cluster_or_the_project() {
    let cases = [
        // the uid is empty, or the listing gives none
        serde_json::json!({"projectUid": "p-1", "clusters": [{"name": "alpha", "uid": ""}]}),
        serde_json::json!({"projectUid": "p-1", "clusters": [{"name": "alpha"}]}),
        serde_json::json!({"projectUid": "p-1", "clusters": [{"name": "alpha", "uid": null}]}),
        // no project, in the cluster or in the listing
        serde_json::json!({"projectUid": "", "clusters": [{"name": "alpha", "uid": "c-1"}]}),
        serde_json::json!({"clusters": [{"name": "alpha", "uid": "c-1", "projectUid": ""}]}),
        // no list of clusters
        serde_json::json!({"projectUid": "p-1"}),
    ];
    for (n, document) in cases.iter().enumerate() {
        for args in [vec![], vec!["--cluster", "alpha"]] {
            let up = Up::new(&[]);
            up.list(document);
            let o = up.run(&args);
            let error = stopped_before_the_cluster(&up, &o);
            assert!(!error.is_empty(), "case {n}: no message");
            assert_no_empty_config(&up);
        }
    }
}

#[test]
fn the_name_of_a_cluster_cannot_make_the_script_write_outside_the_folder_of_kubeconfigs() {
    let up = Up::new(&[("../escape", "c-1", "p-1")]);
    let o = up.run(&["--cluster", "../escape"]);
    stopped_before_the_cluster(&up, &o);
    assert!(!up.path("home/escape.yaml").exists());
    assert!(up.log("api.log").is_empty());
}

#[test]
fn the_kubeconfig_has_the_mode_0600_and_each_kubectl_call_uses_it_whatever_the_shell_says() {
    let up = Up::new(&[("alpha", "c-1", "p-1")]);
    // an old file that other users can read
    std::fs::create_dir(up.path("home/.kube")).unwrap();
    let file = up.kubeconfig("alpha");
    std::fs::write(&file, "old").unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();

    succeeded(&up.run(&["--tag", "v0.29.0"]));
    let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "the mode of the kubeconfig");
    assert!(
        std::fs::read_to_string(&file)
            .unwrap()
            .contains("the kubeconfig of c-1")
    );
    let folder: Vec<_> = std::fs::read_dir(up.path("home/.kube")).unwrap().collect();
    assert_eq!(folder.len(), 1, "no temporary file stays: {folder:?}");

    // the shell has another KUBECONFIG, and no call reads it, except the render that needs no cluster
    let own = format!("[{}] ", file.display());
    let calls = up.kubectl_calls();
    assert!(calls.len() > 8, "{calls:?}");
    for call in &calls {
        assert!(
            call.starts_with(&own) || call.ends_with("] kustomize ."),
            "a call does not use the kubeconfig of the cluster: {call}"
        );
    }
}

#[test]
fn a_kubeconfig_that_palette_refuses_stops_the_script_and_keeps_the_old_file() {
    let up = Up::new(&[("alpha", "c-1", "p-1")]);
    std::fs::create_dir(up.path("home/.kube")).unwrap();
    std::fs::write(up.kubeconfig("alpha"), "old").unwrap();
    let o = up.run_with(&["--tag", "v0.29.0"], &[("FAKE_NO_KUBECONFIG", "1")]);
    let error = stopped_before_the_cluster(&up, &o);
    assert!(error.contains("no kubeconfig for alpha"), "{error}");
    assert_eq!(
        std::fs::read_to_string(up.kubeconfig("alpha")).unwrap(),
        "old"
    );
    let folder: Vec<_> = std::fs::read_dir(up.path("home/.kube")).unwrap().collect();
    assert_eq!(folder.len(), 1, "no temporary file stays: {folder:?}");
}

#[test]
fn the_key_and_the_token_reach_no_argument_of_kubectl_and_no_file_of_the_run() {
    let up = Up::new(&[("alpha", "c-1", "p-1")]);
    succeeded(&up.run(&["--tag", "v0.29.0", "--read-pods"]));

    // both reached the cluster, through standard input
    let sent = up.log("stdin.log");
    assert!(
        sent.contains(KEY_MARKER),
        "the key did not reach the Secret"
    );
    assert!(
        sent.contains(TOKEN_MARKER),
        "the token did not reach the pull Secret"
    );

    // no argument of kubectl, gh, strata, or api.sh holds one
    for log in ["args.log", "gh.log", "strata.log", "api.log"] {
        let calls = up.log(log);
        for marker in [KEY_MARKER, TOKEN_MARKER] {
            assert!(!calls.contains(marker), "{marker} is in {log}:\n{calls}");
        }
    }
    // no file under the home folder or the temporary folder held one at a call of kubectl
    assert_eq!(up.log("leak.log"), "", "a file of the run held a marker");
    // and none holds one at the end. The files of the stand-ins are the cluster, and the token file
    // is the file of the person.
    for root in ["home", "tmp"] {
        for file in walk(&up.path(root)) {
            let body = std::fs::read(&file).unwrap();
            let body = text(&body);
            assert!(
                !body.contains(KEY_MARKER) && !body.contains(TOKEN_MARKER),
                "{} holds a marker",
                file.display()
            );
        }
    }
    let output = format!("{}{}", up.log("applied.yaml"), up.log("records.yaml"));
    assert!(!output.contains(TOKEN_MARKER));
    // the output of the script shows neither
    let o = up.run(&["--tag", "v0.29.0"]);
    let shown = format!("{}{}", text(&o.stdout), text(&o.stderr));
    assert!(
        !shown.contains(KEY_MARKER) && !shown.contains(TOKEN_MARKER),
        "{shown}"
    );
}

#[test]
fn the_overlay_holds_the_tag_the_pull_secret_and_the_two_periods() {
    let up = Up::new(&[("alpha", "c-1", "p-1")]);
    succeeded(&up.run(&["--tag", "v0.29.0"]));
    let overlay = up.overlay();
    for line in [
        "newName: ghcr.io/como-technologies/strata-observer",
        "newTag: v0.29.0",
        "- name: ghcr-pull",
        "value: --content-every=1m",
        "value: --versions-every=5m",
    ] {
        assert!(overlay.contains(line), "{line}:\n{overlay}");
    }
    assert!(!overlay.contains("components:"), "{overlay}");
    assert!(!up.path("fake/overlay/pods").exists());
    assert!(up.path("fake/overlay/base/deployment.yaml").is_file());
}

#[test]
fn the_overlay_of_read_pods_adds_the_component_as_a_team_lists_it() {
    let up = Up::new(&[("alpha", "c-1", "p-1")]);
    succeeded(&up.run(&["--tag", "v0.29.0", "--read-pods"]));
    let overlay = up.overlay();
    assert!(overlay.contains("newTag: v0.29.0"), "{overlay}");
    assert!(overlay.contains("components:\n  - pods\n"), "{overlay}");
    for file in [
        "kustomization.yaml",
        "clusterrole.yaml",
        "clusterrolebinding.yaml",
    ] {
        assert!(up.path("fake/overlay/pods").join(file).is_file(), "{file}");
    }
    assert!(up.path("fake/clusterrole").exists() && up.path("fake/clusterrolebinding").exists());
}

#[test]
fn kubectl_kustomize_renders_the_overlay_with_and_without_the_component() {
    let Some(kubectl) = on_path("kubectl") else {
        eprintln!("skipped: this machine has no kubectl; CI renders the overlay");
        return;
    };
    for (flags, objects, role) in [(&[][..], 4, false), (&["--read-pods"][..], 6, true)] {
        let up = Up::new(&[("alpha", "c-1", "p-1")]);
        let mut args = vec!["--tag", "v0.29.0"];
        args.extend_from_slice(flags);
        succeeded(&up.run(&args));
        let o = Command::new(&kubectl)
            .arg("kustomize")
            .arg(up.path("fake/overlay"))
            .output_locked()
            .unwrap();
        assert!(o.status.success(), "{}", text(&o.stderr));
        let rendered = text(&o.stdout);
        let kinds = rendered.lines().filter(|l| l.starts_with("kind: ")).count();
        assert_eq!(kinds, objects, "{rendered}");
        for line in [
            "image: ghcr.io/como-technologies/strata-observer:v0.29.0",
            "- name: ghcr-pull",
            "- --content-every=1m",
            "- --versions-every=5m",
        ] {
            assert!(rendered.contains(line), "{line}:\n{rendered}");
        }
        assert_eq!(rendered.contains("- --read-pods"), role, "{rendered}");
        assert_eq!(rendered.contains("kind: ClusterRoleBinding"), role);
    }
}

#[test]
fn the_default_tag_is_the_newest_release_and_tag_takes_any_tag() {
    let up = Up::new(&[("alpha", "c-1", "p-1")]);
    succeeded(&up.run(&[]));
    assert!(up.overlay().contains("newTag: v9.9.9"), "{}", up.overlay());
    assert!(up.log("gh.log").contains("release view"));

    let up = Up::new(&[("alpha", "c-1", "p-1")]);
    succeeded(&up.run(&["--tag", "sha-0a1b2c3"]));
    assert!(
        up.overlay().contains("newTag: sha-0a1b2c3"),
        "{}",
        up.overlay()
    );
    assert!(
        !up.log("gh.log").contains("release view"),
        "{}",
        up.log("gh.log")
    );
}

#[test]
fn a_release_that_gh_cannot_read_and_a_wrong_tag_stop_the_script_before_any_change() {
    let up = Up::new(&[("alpha", "c-1", "p-1")]);
    let o = up.run_with(&[], &[("FAKE_NO_RELEASE", "1")]);
    assert!(!o.status.success());
    assert!(text(&o.stderr).contains("--tag"), "{}", text(&o.stderr));
    assert!(up.log("applied.yaml").is_empty());

    let up = Up::new(&[("alpha", "c-1", "p-1")]);
    for tag in ["a b", "x: y", "-v", "a/b"] {
        let o = up.run(&["--tag", tag]);
        assert_eq!(o.status.code(), Some(2), "{tag}: {}", text(&o.stderr));
    }
    let o = up.run_with(
        &["--tag", "v1"],
        &[("OBSERVER_IMAGE", "registry.example/observer:v2")],
    );
    assert_eq!(o.status.code(), Some(2));
    assert!(up.kubectl_calls().is_empty());
}

#[test]
fn a_second_run_works_and_each_object_is_made_by_create_and_sent_by_apply() {
    let up = Up::new(&[("alpha", "c-1", "p-1")]);
    succeeded(&up.run(&["--tag", "v0.29.0"]));
    succeeded(&up.run(&["--tag", "v0.29.0"]));
    let calls = up.kubectl_calls();
    let creates: Vec<&String> = calls.iter().filter(|c| c.contains(" create ")).collect();
    assert_eq!(creates.len(), 6, "{creates:?}: three objects, two runs");
    for call in &creates {
        assert!(call.ends_with("--dry-run=client -o yaml"), "{call}");
    }
    for object in [
        "create configmap strata-observer",
        "create secret generic strata-observer-key",
        "create secret generic ghcr-pull",
    ] {
        assert_eq!(
            creates.iter().filter(|c| c.contains(object)).count(),
            2,
            "{object}"
        );
    }
    // each create has its apply: the two run in a pipe, so their lines come in any order
    let applies = calls.iter().filter(|c| c.ends_with("] apply -f -")).count();
    assert_eq!(applies, 6, "{calls:?}");
    // the namespace is applied from its file, and the manifests from the render
    assert_eq!(
        calls
            .iter()
            .filter(|c| c.ends_with("/deploy/observer/namespace.yaml"))
            .count(),
        2
    );
    assert_eq!(
        calls
            .iter()
            .filter(|c| c.ends_with("apply -f observer.yaml"))
            .count(),
        2
    );
    assert_no_empty_config(&up);
}

#[test]
fn the_first_seen_records_go_through_a_server_side_apply_that_forces_the_conflicts() {
    let up = Up::new(&[("alpha", "c-1", "p-1")]);
    succeeded(&up.run(&["--tag", "v0.29.0"]));
    assert!(
        up.kubectl_calls()
            .iter()
            .any(|c| c.ends_with("apply --server-side --force-conflicts -f -")),
        "{:?}",
        up.kubectl_calls()
    );
    let records = up.log("records.yaml");
    assert_eq!(records.matches("kind: Secret").count(), 4, "{records}");
    assert!(up.log("strata.log").contains("snapshot cluster:c-1 "));
    // the pod restarts after the objects it reads, and the script waits for it
    let calls = up.kubectl_calls().join("\n");
    let at = |text: &str| {
        calls
            .find(text)
            .unwrap_or_else(|| panic!("{text}:\n{calls}"))
    };
    assert!(at("apply --server-side") < at("apply -f observer.yaml"));
    assert!(at("apply -f observer.yaml") < at("rollout restart deploy/strata-observer"));
    assert!(at("rollout restart") < at("rollout status deploy/strata-observer --timeout=180s"));
}

#[test]
fn the_end_of_up_prints_the_commands_that_open_the_page_on_port_18080() {
    let up = Up::new(&[("alpha", "c-1", "p-1")]);
    let o = up.run(&["--tag", "v0.29.0"]);
    succeeded(&o);
    let shown = text(&o.stdout);
    let file = up.kubeconfig("alpha");
    for line in [
        "just observer-open alpha".to_string(),
        format!(
            "kubectl --kubeconfig {} -n strata-observer port-forward deploy/strata-observer 18080:8080",
            file.display()
        ),
        "http://127.0.0.1:18080".to_string(),
        "just observer-down alpha".to_string(),
    ] {
        assert!(shown.contains(&line), "{line}:\n{shown}");
    }
}

#[test]
fn a_failed_step_stops_up_and_says_which_step_failed() {
    let up = Up::new(&[("alpha", "c-1", "p-1")]);
    // no token file: the first step stops, and nothing changes in the cluster
    std::fs::remove_file(up.path("secrets/ghcr-pull-token")).unwrap();
    let o = up.run(&["--tag", "v0.29.0"]);
    assert_eq!(o.status.code(), Some(2));
    let error = text(&o.stderr);
    assert!(error.contains("no token in"), "{error}");
    assert!(error.contains("the pull Secret"), "{error}");
    assert!(up.log("applied.yaml").is_empty());
    assert!(!up.path("fake/ns").exists(), "the namespace is not made");
}

#[test]
fn down_deletes_the_namespace_the_cluster_role_and_its_binding_and_a_second_run_is_no_error() {
    let up = Up::new(&[("alpha", "c-1", "p-1")]);
    succeeded(&up.run(&["--tag", "v0.29.0", "--read-pods"]));
    for state in ["ns", "deploy", "clusterrole", "clusterrolebinding"] {
        assert!(
            up.path("fake").join(state).exists(),
            "{state} exists after up"
        );
    }

    let o = up.run(&["down"]);
    succeeded(&o);
    for state in ["ns", "deploy", "clusterrole", "clusterrolebinding"] {
        assert!(
            !up.path("fake").join(state).exists(),
            "{state} is gone after down"
        );
    }
    let calls = up.kubectl_calls();
    let deletes: Vec<&String> = calls.iter().filter(|c| c.contains(" delete ")).collect();
    assert_eq!(deletes.len(), 2, "{deletes:?}");
    assert!(
        deletes[0].contains("delete namespace strata-observer --ignore-not-found"),
        "{deletes:?}"
    );
    assert!(
        deletes[1].contains("delete clusterrole,clusterrolebinding -l app.kubernetes.io/name=strata-observer --ignore-not-found"),
        "{deletes:?}"
    );

    // the second run: nothing is there, and the run still succeeds
    let o = up.run(&["down"]);
    succeeded(&o);
}

#[test]
fn down_on_a_cluster_with_no_observer_is_no_error_and_down_uses_the_kubeconfig_of_the_cluster() {
    let up = Up::new(&[("alpha", "c-1", "p-1"), ("beta", "c-2", "p-1")]);
    let o = up.run(&["down", "--cluster", "beta"]);
    succeeded(&o);
    let own = format!("[{}] ", up.kubeconfig("beta").display());
    let calls = up.kubectl_calls();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert!(calls.iter().all(|c| c.starts_with(&own)), "{calls:?}");
    assert!(!up.kubeconfig("alpha").exists());
}

#[test]
fn down_deletes_by_the_label_of_the_observer_and_touches_no_object_of_another_name() {
    // the namespace has no label of the observer, so it goes by its name. The role and the binding
    // carry the label in deploy/observer/pods.
    for file in ["clusterrole.yaml", "clusterrolebinding.yaml"] {
        let manifest = repo_file(&format!("deploy/observer/pods/{file}"));
        assert!(
            manifest.contains("app.kubernetes.io/name: strata-observer"),
            "{file} has the label that down selects"
        );
    }
    assert!(
        !repo_file("deploy/observer/namespace.yaml").contains("app.kubernetes.io/name"),
        "the namespace has no label, so down deletes it by name"
    );
    let script = repo_file("examples/live/observer-up.sh");
    assert!(script.contains("selector=app.kubernetes.io/name=strata-observer"));
}

#[test]
fn down_and_open_refuse_the_flags_of_up_and_the_script_refuses_what_it_does_not_know() {
    let up = Up::new(&[("alpha", "c-1", "p-1")]);
    for args in [
        vec!["down", "--tag", "v1"],
        vec!["open", "--read-pods"],
        vec!["--unknown"],
        vec!["--cluster"],
        vec!["restart"],
    ] {
        let o = up.run(&args);
        assert_eq!(o.status.code(), Some(2), "{args:?}: {}", text(&o.stderr));
    }
    assert!(up.kubectl_calls().is_empty());
}

#[test]
fn open_holds_a_port_forward_to_the_pod_with_the_kubeconfig_of_the_cluster() {
    let up = Up::new(&[("alpha", "c-1", "p-1")]);
    succeeded(&up.run(&["--tag", "v0.29.0"]));
    let o = up.run(&["open"]);
    succeeded(&o);
    let file = up.kubeconfig("alpha");
    let want = format!(
        "--kubeconfig {} -n strata-observer port-forward deploy/strata-observer 18080:8080",
        file.display()
    );
    assert!(
        up.kubectl_calls().iter().any(|c| c.ends_with(&want)),
        "{:?}",
        up.kubectl_calls()
    );
    assert!(
        text(&o.stdout).contains("http://127.0.0.1:18080"),
        "{}",
        text(&o.stdout)
    );

    // PORT gives another port
    let o = up.run_with(&["open"], &[("PORT", "28080")]);
    succeeded(&o);
    assert!(
        up.log("args.log")
            .contains("port-forward deploy/strata-observer 28080:8080")
    );
}

#[test]
fn open_with_no_observer_in_the_cluster_says_so_and_opens_no_port_forward() {
    let up = Up::new(&[("alpha", "c-1", "p-1")]);
    let o = up.run(&["open"]);
    assert_eq!(o.status.code(), Some(2));
    let error = text(&o.stderr);
    assert!(
        error.contains("no observer in the cluster alpha"),
        "{error}"
    );
    assert!(error.contains("just observer-up alpha"), "{error}");
    assert!(!up.log("args.log").contains("port-forward"));
}

#[test]
fn the_help_says_that_the_script_is_for_a_test_cluster_and_what_read_pods_needs() {
    let up = Up::new(&[]);
    let o = up.run(&["--help"]);
    succeeded(&o);
    let help = text(&o.stdout);
    let one = help.split_whitespace().collect::<Vec<_>>().join(" ");
    for needle in [
        "It is for a test.",
        "A team installs the observer with the steps of \"The observer\" in docs/src/automation/observer.md",
        "--read-pods needs an image with that code.",
        "the newest release",
        "A cluster with no observer is no error.",
        "observer-up.sh open [--cluster NAME]",
        "observer-up.sh down [--cluster NAME]",
    ] {
        assert!(one.contains(needle), "{needle}:\n{help}");
    }
    assert!(up.kubectl_calls().is_empty());
}

#[test]
fn the_recipes_pass_the_cluster_and_the_tag_and_the_flags_to_the_script() {
    let Some(just) = on_path("just") else {
        eprintln!("skipped: this machine has no just; CI reads the recipes");
        return;
    };
    let dry = |args: &[&str]| {
        let o = Command::new(&just)
            .arg("--dry-run")
            .args(args)
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output_locked()
            .unwrap();
        assert!(o.status.success(), "{}", text(&o.stderr));
        text(&o.stderr)
            .lines()
            .filter(|l| l.starts_with("examples/live/"))
            .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect::<Vec<_>>()
    };
    assert_eq!(dry(&["observer-up"]), ["examples/live/observer-up.sh up"]);
    assert_eq!(
        dry(&["observer-up", "fred", "v0.29.0", "--read-pods"]),
        ["examples/live/observer-up.sh up --cluster 'fred' --tag 'v0.29.0' --read-pods"]
    );
    assert_eq!(
        dry(&["observer-up", "fred"]),
        ["examples/live/observer-up.sh up --cluster 'fred'"]
    );
    assert_eq!(
        dry(&["observer-open"]),
        ["examples/live/observer-up.sh open"]
    );
    assert_eq!(
        dry(&["observer-open", "fred"]),
        ["examples/live/observer-up.sh open --cluster 'fred'"]
    );
    assert_eq!(
        dry(&["observer-down"]),
        ["examples/live/observer-up.sh down"]
    );
    assert_eq!(
        dry(&["observer-down", "fred"]),
        ["examples/live/observer-up.sh down --cluster 'fred'"]
    );
}

#[test]
fn the_live_example_names_the_script_and_the_three_recipes_with_what_each_needs() {
    let page = repo_file("docs/src/development/live-example.md");
    let one = page.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        page.contains("| `observer-up.sh` |"),
        "the table of scripts lacks observer-up.sh"
    );
    for needle in [
        "just observer-up [CLUSTER] [TAG] [--read-pods]",
        "just observer-open [CLUSTER]",
        "just observer-down [CLUSTER]",
    ] {
        assert!(one.contains(needle), "the page lacks {needle}");
    }
    let section = one
        .split("## A test cluster")
        .nth(1)
        .expect("the page has the section A test cluster");
    for needle in [
        "a token file with only the scope `read:packages`",
        "`kubectl`",
        "`gh`",
        "The key of Palette",
        "A team uses the steps of [Install](../automation/observer.md#install)",
        "`~/.kube/strata-<cluster>.yaml`",
    ] {
        assert!(section.contains(needle), "the section lacks {needle}");
    }
}

// ---- the folder of the observer and its page ------------------------------------------------------

/// The anchor that mdBook makes for a heading: lower case, a hyphen for each space, and no other
/// punctuation.
fn anchor(heading: &str) -> String {
    heading
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-')
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

#[test]
fn the_readme_of_the_observer_holds_only_links_to_the_book_and_each_link_exists() {
    let readme = repo_file("observer/README.md");
    let mut lines = readme.lines().filter(|l| !l.trim().is_empty());
    assert_eq!(lines.next(), Some("# strata-observer"));
    let mut targets = Vec::new();
    for line in lines {
        let (link, _) = line
            .strip_prefix("- [")
            .and_then(|rest| rest.split_once("]("))
            .and_then(|(_, rest)| rest.split_once("): "))
            .unwrap_or_else(|| panic!("the line is no link with its line of text: {line}"));
        let (file, fragment) = link.split_once('#').unwrap_or((link, ""));
        let page = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("observer")
            .join(file);
        assert!(page.is_file(), "{link} does not exist");
        let body = std::fs::read_to_string(&page).unwrap();
        assert!(
            fragment.is_empty()
                || body
                    .lines()
                    .filter_map(|l| l
                        .trim_start_matches('#')
                        .strip_prefix(' ')
                        .filter(|_| l.starts_with('#')))
                    .any(|h| anchor(h) == fragment),
            "{link} names no heading"
        );
        targets.push(file.trim_start_matches("../docs/src/").to_string());
    }
    for page in [
        "automation/observer.md",
        "use-cases/observer.md",
        "reference/observer-api.md",
        "development/observer.md",
        "development/observer-api.md",
        "development/observer-deploy.md",
        "development/live-example.md",
    ] {
        assert!(targets.iter().any(|t| t == page), "the README lacks {page}");
    }
    assert!(readme.contains("`just observer-up`"));
}

#[test]
fn the_build_of_the_observer_in_its_folder_leaves_a_binary_that_git_ignores() {
    let ignore = repo_file(".gitignore");
    assert!(
        ignore.lines().any(|l| l == "/observer/observer"),
        "{ignore}"
    );
}

#[test]
fn the_page_of_the_observer_has_the_five_steps_from_the_baseline_to_the_check() {
    let page = repo_file("docs/src/automation/observer.md");
    let headings: Vec<&str> = page.lines().filter(|l| l.starts_with("## ")).collect();
    let at = |h: &str| {
        headings
            .iter()
            .position(|x| *x == h)
            .unwrap_or_else(|| panic!("no {h}"))
    };
    assert_eq!(
        at("## The baseline and the check"),
        at("## What the service does") + 1
    );
    assert_eq!(
        at("## Run it once"),
        at("## The baseline and the check") + 1
    );

    let section = page
        .split("## The baseline and the check\n")
        .nth(1)
        .and_then(|s| s.split("\n## ").next())
        .unwrap();
    let steps: Vec<&str> = section
        .lines()
        .filter(|l| l.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .collect();
    assert_eq!(steps.len(), 5, "{steps:?}");
    for (step, title) in steps.iter().zip([
        "**Record a version.**",
        "**Merge the record.**",
        "**Write the records.**",
        "**Check.**",
        "**Close the finding.**",
    ]) {
        assert!(step.contains(title), "{step}");
    }
    let one = section.split_whitespace().collect::<Vec<_>>().join(" ");
    for needle in [
        "Only a merge moves the baseline.",
        "The observer never writes a record.",
        "[First-seen records](#first-seen-records)",
        "`--revision`",
    ] {
        assert!(one.contains(needle), "the section lacks {needle}");
    }
}
