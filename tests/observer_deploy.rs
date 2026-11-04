//! The manifests and the image of the observer: `deploy/observer/`. No cluster and no Docker: a test
//! reads the files, and a stand-in `kubectl` stands for the render. Each test ties a manifest to the
//! code or to the script that it depends on, so that the two cannot drift.

use std::path::Path;
use std::process::Command;

use serde_yaml::Value;

use crate::common::{
    CommandExt, block_with, fake_bin, kubectl_args, kubectl_input, page_section, path_with, recipe,
    repo_file, run_with_kubectl, script, steps, text, workflow, workflow_text,
};

/// A file of `deploy/observer/`, parsed as one YAML document.
fn deploy_doc(file: &str) -> Value {
    serde_yaml::from_str(&repo_file(&format!("deploy/observer/{file}")))
        .unwrap_or_else(|e| panic!("{file}: not YAML: {e}"))
}

/// The files that the kustomization renders, with the kind of the one object in each.
const RENDERED: [(&str, &str); 4] = [
    ("namespace.yaml", "Namespace"),
    ("serviceaccount.yaml", "ServiceAccount"),
    ("networkpolicy.yaml", "NetworkPolicy"),
    ("deployment.yaml", "Deployment"),
];

/// The names of the strings in a YAML sequence.
fn strings(value: &Value) -> Vec<&str> {
    value
        .as_sequence()
        .unwrap_or_else(|| panic!("not a sequence: {value:?}"))
        .iter()
        .map(|v| v.as_str().unwrap_or_else(|| panic!("not a string: {v:?}")))
        .collect()
}

/// The name and the value of an argument that is a flag: `--read-pods` has no value, and
/// `--state=/state` has one.
fn flag_of(arg: &str) -> (&str, Option<&str>) {
    let flag = arg
        .strip_prefix("--")
        .unwrap_or_else(|| panic!("{arg} is no flag"));
    match flag.split_once('=') {
        Some((name, value)) => (name, Some(value)),
        None => (flag, None),
    }
}

/// The pod template of the Deployment.
fn pod() -> Value {
    deploy_doc("deployment.yaml")["spec"]["template"]["spec"].clone()
}

/// The one container of the pod.
fn container() -> Value {
    let pod = pod();
    let containers = pod["containers"]
        .as_sequence()
        .expect("the pod has containers");
    assert_eq!(containers.len(), 1, "the pod has one container");
    containers[0].clone()
}

#[test]
fn the_kustomization_renders_four_objects_and_leaves_the_reader_role_to_the_team() {
    let kustomization = deploy_doc("kustomization.yaml");
    assert_eq!(kustomization["namespace"], "strata-observer");
    let resources = strings(&kustomization["resources"]);
    assert_eq!(resources, RENDERED.map(|(file, _)| file));
    for (file, kind) in RENDERED {
        let source = repo_file(&format!("deploy/observer/{file}"));
        assert!(
            !source.lines().any(|line| line == "---"),
            "{file} holds one object: the kustomization renders four"
        );
        assert_eq!(deploy_doc(file)["kind"], kind, "{file}");
    }

    // the role is outside the kustomization: a team binds it to its own group
    let mut files: Vec<String> =
        std::fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("deploy/observer"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".yaml"))
            .collect();
    files.sort();
    assert_eq!(
        files,
        [
            "deployment.yaml",
            "kustomization.yaml",
            "namespace.yaml",
            "networkpolicy.yaml",
            "reader-role.yaml",
            "serviceaccount.yaml"
        ]
    );
    assert!(!resources.contains(&"reader-role.yaml"));

    // the component is a folder that a team lists under `components:`, so the base lists none of it
    assert!(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("deploy/observer/pods")
            .is_dir()
    );
    assert!(resources.iter().all(|r| !r.starts_with("pods")));
}

/// A stand-in `kubectl` for `kubectl kustomize deploy/observer`. It prints the four files with a
/// separator, and one object more when `FAKE_EXTRA` is set.
const FAKE_KUBECTL_KUSTOMIZE: &str = r#"#!/usr/bin/env bash
[ "$1 $2" = "kustomize deploy/observer" ] || exit 2
for f in namespace serviceaccount networkpolicy deployment; do
  cat "deploy/observer/$f.yaml"
  echo "---"
done
[ -z "${FAKE_EXTRA-}" ] || printf 'apiVersion: v1\nkind: ConfigMap\n---\n'
"#;

/// Run the lines of the recipe `name` in the root of the repository, with `kubectl` as the stand-in
/// `kubectl` and `env` as more variables. The `just` program runs each line in turn and stops at the
/// first line that fails. Gives the exit code.
fn run_recipe(name: &str, kubectl: &str, env: &[(&str, &str)]) -> Option<i32> {
    let dir = tempfile::tempdir().unwrap();
    let bin = fake_bin(dir.path(), &[("kubectl", kubectl)]);
    Command::new("bash")
        .args(["--noprofile", "--norc", "-euc", &recipe(name).join("\n")])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("PATH", path_with(&bin))
        .envs(env.iter().copied())
        .output_locked()
        .unwrap()
        .status
        .code()
}

#[test]
fn the_recipe_observer_manifests_passes_for_four_objects_and_fails_for_any_other_count() {
    assert_eq!(recipe("observer-manifests").len(), 1);
    let run = |env: &[(&str, &str)]| run_recipe("observer-manifests", FAKE_KUBECTL_KUSTOMIZE, env);
    assert_eq!(run(&[]), Some(0), "the four objects of the repository");
    assert_eq!(run(&[("FAKE_EXTRA", "1")]), Some(1), "five objects");
}

#[test]
fn the_namespace_enforces_the_restricted_pod_security_standard() {
    let namespace = deploy_doc("namespace.yaml");
    assert_eq!(namespace["metadata"]["name"], "strata-observer");
    assert_eq!(
        namespace["metadata"]["labels"]["pod-security.kubernetes.io/enforce"],
        "restricted"
    );
}

#[test]
fn the_service_account_and_the_pod_get_no_token_and_the_namespace_has_no_role() {
    assert_eq!(
        deploy_doc("serviceaccount.yaml")["automountServiceAccountToken"],
        false
    );
    assert_eq!(pod()["automountServiceAccountToken"], false);
    assert_eq!(pod()["serviceAccountName"], "strata-observer");
    // the base calls no Kubernetes API, so nothing in the kustomization gives a role
    for (file, _) in RENDERED {
        let kind = deploy_doc(file)["kind"].as_str().unwrap().to_string();
        assert!(
            !kind.contains("Role") && !kind.contains("Binding"),
            "{file} is a {kind}: the observer needs no role"
        );
    }
}

#[test]
fn the_pod_runs_as_non_root_with_a_read_only_root_and_no_capability_and_no_privilege() {
    let pod = pod();
    assert_eq!(pod["securityContext"]["runAsNonRoot"], true);
    assert_eq!(pod["securityContext"]["runAsUser"], 65532);
    assert_eq!(pod["securityContext"]["fsGroup"], 65532);
    assert_eq!(
        pod["securityContext"]["seccompProfile"]["type"],
        "RuntimeDefault"
    );
    for host in ["hostNetwork", "hostPID", "hostIPC"] {
        assert!(pod.get(host).is_none(), "{host}");
    }
    let security = &container()["securityContext"];
    assert_eq!(security["allowPrivilegeEscalation"], false);
    assert_eq!(security["readOnlyRootFilesystem"], true);
    assert_eq!(strings(&security["capabilities"]["drop"]), ["ALL"]);
    assert!(security.get("privileged").is_none());
    assert_eq!(deploy_doc("deployment.yaml")["spec"]["replicas"], 1);
    assert_eq!(
        deploy_doc("deployment.yaml")["spec"]["strategy"]["type"],
        "Recreate",
        "two pods would both write the state"
    );
}

/// The paths of the flags of `serve` are the mounts of the volumes, the records are read-only, and
/// the volume of the work is in memory, because a cycle writes the values of a cluster there.
#[test]
fn the_flags_of_the_observer_name_the_mounts_and_the_work_volume_stays_in_memory() {
    let container = container();
    let args = strings(&container["args"]);
    assert_eq!(args[0], "serve");
    let main = repo_file("observer/main.go");
    assert!(
        main.contains(r#"case "serve":"#),
        "the observer has the command serve"
    );
    let mut paths = Vec::new();
    for arg in &args[1..] {
        let (flag, value) = flag_of(arg);
        let value = value.unwrap_or_else(|| panic!("{arg} is no flag with a value"));
        assert!(
            main.contains(&format!("\"{flag}\"")),
            "the observer has no flag --{flag}"
        );
        paths.push((flag, value));
    }
    let mount = |name: &str| {
        container["volumeMounts"]
            .as_sequence()
            .unwrap()
            .iter()
            .find(|m| m["name"] == name)
            .unwrap_or_else(|| panic!("no mount {name}"))
            .clone()
    };
    for (flag, volume) in [("records", "records"), ("work", "work"), ("state", "state")] {
        let value = paths.iter().find(|(f, _)| *f == flag).unwrap().1;
        assert_eq!(mount(volume)["mountPath"], value, "--{flag}");
    }
    assert_eq!(mount("records")["readOnly"], true);
    let volumes = pod()["volumes"].clone();
    let volume = |name: &str| {
        volumes
            .as_sequence()
            .unwrap()
            .iter()
            .find(|v| v["name"] == name)
            .unwrap_or_else(|| panic!("no volume {name}"))
            .clone()
    };
    assert_eq!(volume("work")["emptyDir"]["medium"], "Memory");
    assert!(
        volume("state")["emptyDir"]["medium"].is_null(),
        "the state is on the disk"
    );
    // a team that sets a size writes it once, so the manifest bounds both
    for name in ["work", "state"] {
        assert!(volume(name)["emptyDir"]["sizeLimit"].is_string(), "{name}");
    }
}

/// The process gets the variables of its list and no other. The pod sets exactly those that the
/// observer lets through, and the one that the flag `--cluster-uid` reads.
#[test]
fn the_pod_sets_only_the_variables_that_the_observer_hands_to_strata_and_the_cluster_uid() {
    let runner = repo_file("observer/internal/runner/runner.go");
    let line = runner
        .lines()
        .find(|l| l.starts_with("var allowedEnv = []string{"))
        .expect("the runner lists the variables it hands on");
    let allowed: Vec<&str> = line
        .trim_start_matches("var allowedEnv = []string{")
        .trim_end_matches('}')
        .split(',')
        .map(|s| s.trim().trim_matches('"'))
        .collect();
    let container = container();
    let set: Vec<&str> = container["env"]
        .as_sequence()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    for name in &set {
        assert!(
            *name == "STRATA_CLUSTER_UID" || allowed.contains(name),
            "{name} reaches no process: the runner does not hand it on"
        );
    }
    // the key comes from a Secret and the rest from the ConfigMap, never from a value in the file
    for env in container["env"].as_sequence().unwrap() {
        let name = env["name"].as_str().unwrap();
        if name == "PALETTE_API_KEY" {
            assert_eq!(
                env["valueFrom"]["secretKeyRef"]["name"],
                "strata-observer-key"
            );
            assert_eq!(env["valueFrom"]["secretKeyRef"]["key"], "api-key");
        }
        if ["STRATA_CLUSTER_UID", "PALETTE_API", "PALETTE_PROJECT_UID"].contains(&name) {
            assert_eq!(
                env["valueFrom"]["configMapKeyRef"]["name"], "strata-observer",
                "{name}"
            );
        }
    }
    for name in [
        "PALETTE_API",
        "PALETTE_API_KEY",
        "PALETTE_PROJECT_UID",
        "HOME",
        "TMPDIR",
    ] {
        assert!(set.contains(&name), "the pod sets {name}");
    }
    // a probe runs in the container, because the listener is on 127.0.0.1
    let main = repo_file("observer/main.go");
    assert!(main.contains(r#"case "health":"#));
    for probe in ["startupProbe", "livenessProbe"] {
        assert_eq!(
            strings(&container[probe]["exec"]["command"]),
            ["/usr/local/bin/strata-observer", "health"],
            "{probe}"
        );
    }
    assert!(container.get("readinessProbe").is_none() && container.get("ports").is_none());
}

/// The four Secrets that the Deployment mounts are the four that `observer-records.sh` writes, in
/// the namespace of the kustomization.
#[test]
fn the_deployment_mounts_the_four_secrets_that_the_records_script_writes() {
    let projected = pod()["volumes"]
        .as_sequence()
        .unwrap()
        .iter()
        .find(|v| v["name"] == "records")
        .unwrap()["projected"]
        .clone();
    let mounted: Vec<(String, bool)> = projected["sources"]
        .as_sequence()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["secret"]["name"].as_str().unwrap().to_string(),
                s["secret"]["optional"].as_bool().unwrap(),
            )
        })
        .collect();
    assert!(
        mounted.iter().all(|(_, optional)| *optional),
        "a Secret that the pipeline leaves empty or has not made yet must not stop the pod"
    );
    assert!(repo_file("deploy/observer/deployment.yaml").contains("defaultMode: 0440"));

    let dir = tempfile::tempdir().unwrap();
    let record = dir.path().join("record.json");
    std::fs::write(
        &record,
        r#"{"schema":"strata-snapshot/1","source":{"name":"p","version":"1.0.0"}}"#,
    )
    .unwrap();
    let o = Command::new("bash")
        .arg(script("ci/observer-records.sh"))
        .args(["--revision", "r"])
        .arg(&record)
        .output_locked()
        .unwrap();
    assert!(o.status.success(), "{}", text(&o.stderr));
    use serde::Deserialize;
    let written: Vec<(String, String)> = serde_yaml::Deserializer::from_str(&text(&o.stdout))
        .map(|doc| {
            let secret = Value::deserialize(doc).unwrap();
            (
                secret["metadata"]["name"].as_str().unwrap().to_string(),
                secret["metadata"]["namespace"]
                    .as_str()
                    .unwrap()
                    .to_string(),
            )
        })
        .collect();
    let kustomization = deploy_doc("kustomization.yaml");
    for ((name, _), (written_name, namespace)) in mounted.iter().zip(&written) {
        assert_eq!(name, written_name);
        assert_eq!(*namespace, kustomization["namespace"].as_str().unwrap());
    }
    assert_eq!(mounted.len(), written.len());
}

/// The protocol and the port of each egress rule of a `NetworkPolicy`, in order.
fn egress_ports(policy: &Value) -> Vec<(String, u64)> {
    policy["spec"]["egress"]
        .as_sequence()
        .unwrap()
        .iter()
        .flat_map(|rule| rule["ports"].as_sequence().unwrap())
        .map(|p| {
            (
                p["protocol"].as_str().unwrap().to_string(),
                p["port"].as_u64().unwrap(),
            )
        })
        .collect()
}

#[test]
fn the_network_policy_lets_no_pod_connect_in_and_lets_the_observer_connect_to_dns_and_https() {
    let policy = deploy_doc("networkpolicy.yaml");
    let label = &deploy_doc("deployment.yaml")["spec"]["template"]["metadata"]["labels"];
    assert_eq!(&policy["spec"]["podSelector"]["matchLabels"], label);
    assert_eq!(
        strings(&policy["spec"]["policyTypes"]),
        ["Ingress", "Egress"]
    );
    assert_eq!(
        policy["spec"]["ingress"].as_sequence().map(Vec::len),
        Some(0),
        "an empty list of rules lets no pod connect in"
    );
    assert_eq!(
        egress_ports(&policy),
        [("UDP".into(), 53), ("TCP".into(), 53), ("TCP".into(), 443)]
    );
    assert!(
        policy["spec"]["egress"]
            .as_sequence()
            .unwrap()
            .iter()
            .all(|rule| rule.get("to").is_none()),
        "a team narrows the address in its own copy"
    );
}

/// The role gives the page, and no Secret and no exec. A role with a wildcard, or with the
/// resource `pods/exec` or `secrets`, would give more than a person needs to read the page.
#[test]
fn the_reader_role_gives_the_page_and_no_secret_and_no_exec() {
    let role = deploy_doc("reader-role.yaml");
    assert_eq!(role["kind"], "Role");
    assert_eq!(role["metadata"]["namespace"], "strata-observer");
    let rules: Vec<(Vec<&str>, Vec<&str>, Vec<&str>)> = role["rules"]
        .as_sequence()
        .unwrap()
        .iter()
        .map(|r| {
            (
                strings(&r["apiGroups"]),
                strings(&r["resources"]),
                strings(&r["verbs"]),
            )
        })
        .collect();
    assert_eq!(
        rules,
        [
            (vec!["apps"], vec!["deployments"], vec!["get"]),
            (vec![""], vec!["pods"], vec!["get", "list"]),
            (vec![""], vec!["pods/portforward"], vec!["create"]),
        ]
    );
    assert_eq!(
        strings(&role["rules"][0]["resourceNames"]),
        ["strata-observer"],
        "the role reads the one Deployment"
    );
    let source = repo_file("deploy/observer/reader-role.yaml");
    for forbidden in ["secrets", "pods/exec", "'*'", "\"*\"", "[*]"] {
        let code: String = source
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .collect();
        assert!(!code.contains(forbidden), "the role names {forbidden}");
    }
}

/// The Deployment runs the image that `observer-image.yml` pushes. The workflow lowers the name of
/// the repository, and the tag is a placeholder that a team replaces with a release.
#[test]
fn the_deployment_runs_the_image_that_the_workflow_pushes() {
    let cargo = repo_file("Cargo.toml");
    let repository = cargo
        .lines()
        .find_map(|l| l.strip_prefix("repository = \"https://github.com/"))
        .and_then(|l| l.strip_suffix('"'))
        .expect("Cargo.toml names the repository");
    let image = format!("ghcr.io/{}-observer", repository.to_lowercase());
    assert_eq!(
        container()["image"].as_str(),
        Some(format!("{image}:VERSION").as_str())
    );
    assert!(
        workflow_text("observer-image.yml").contains("ghcr.io/${GITHUB_REPOSITORY,,}-observer"),
        "the workflow pushes {image}"
    );
}

/// The Dockerfile pins each base by its digest and builds the Go of the module. The binary of
/// strata is the one that the workflow and the recipe put in `dist/`, one for each platform.
#[test]
fn the_dockerfile_pins_each_base_and_copies_the_binary_that_the_workflow_puts_in_dist() {
    let dockerfile = repo_file("deploy/observer/Dockerfile");
    let bases: Vec<&str> = dockerfile
        .lines()
        .filter(|l| l.starts_with("FROM "))
        .collect();
    assert_eq!(bases.len(), 2, "a build stage and a final stage");
    for base in &bases {
        let digest = base
            .split("@sha256:")
            .nth(1)
            .unwrap_or_else(|| panic!("{base}: no digest"));
        let digest = digest.split_whitespace().next().unwrap();
        assert!(
            digest.len() == 64 && digest.chars().all(|c| c.is_ascii_hexdigit()),
            "{base}: not a digest"
        );
    }
    let go_mod = repo_file("observer/go.mod");
    let go = go_mod
        .lines()
        .find_map(|l| l.strip_prefix("go "))
        .expect("go.mod names the version of Go");
    assert!(
        bases[0].contains(&format!("golang:{go}@")),
        "the build stage runs the Go of the module: {}",
        bases[0]
    );
    assert!(
        bases[0].contains("--platform=$BUILDPLATFORM"),
        "the build is native"
    );
    assert!(bases[1].contains("distroless/static"), "{}", bases[1]);
    assert!(dockerfile.contains("CGO_ENABLED=0"));
    assert!(dockerfile.contains("COPY dist/strata-$TARGETARCH /usr/local/bin/strata"));
    assert!(dockerfile.contains("ENTRYPOINT [\"/usr/local/bin/strata-observer\"]"));

    // the workflow builds the two platforms that the files in dist/ stand for
    let doc = workflow("observer-image.yml");
    let build = steps(&doc)
        .find(|s| {
            s["uses"]
                .as_str()
                .is_some_and(|u| u.starts_with("docker/build-push-action@"))
        })
        .unwrap();
    assert_eq!(build["with"]["platforms"], "linux/amd64,linux/arm64");
    let install = steps(&doc)
        .find_map(|s| {
            s["run"]
                .as_str()
                .filter(|r| r.contains("install-strata.sh"))
        })
        .unwrap();
    assert!(install.contains(r#""dist/strata-${pair#*:}""#), "{install}");
    assert!(install.contains("x86_64-unknown-linux-musl:amd64"));
    assert!(install.contains("aarch64-unknown-linux-musl:arm64"));

    // a local build and the build of the workflow share the file and the ignore list
    let ignore = repo_file("deploy/observer/Dockerfile.dockerignore");
    let kept: Vec<&str> = ignore
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .collect();
    assert_eq!(kept, ["*", "!observer", "!dist"]);
    assert!(
        repo_file(".gitignore").lines().any(|l| l == "/dist/"),
        "the release script refuses an untracked file"
    );
}

#[test]
fn the_recipe_observer_image_builds_the_static_binary_and_the_image_of_the_dockerfile() {
    let justfile = repo_file("justfile");
    assert!(
        justfile.contains("observer-image: (build-target \"x86_64-unknown-linux-musl\")"),
        "the image is distroless/static, so it needs the musl build"
    );
    assert_eq!(
        recipe("observer-image"),
        [
            "mkdir -p dist && cp target/x86_64-unknown-linux-musl/release/strata dist/strata-amd64",
            "docker build -f deploy/observer/Dockerfile --build-arg VERSION=dev --platform linux/amd64 -t strata-observer:dev ."
        ]
    );
}

// ---- the component deploy/observer/pods ------------------------------------------------------------

/// The files of the component that hold one object each, with the kind of that object.
const COMPONENT: [(&str, &str); 2] = [
    ("clusterrole.yaml", "ClusterRole"),
    ("clusterrolebinding.yaml", "ClusterRoleBinding"),
];

/// The name of the volume of the token and of the CA in the pod that the component makes.
const KUBE_VOLUME: &str = "kube-credentials";

/// A file of `deploy/observer/pods/`, parsed as one YAML document.
fn pods_doc(file: &str) -> Value {
    serde_yaml::from_str(&repo_file(&format!("deploy/observer/pods/{file}")))
        .unwrap_or_else(|e| panic!("pods/{file}: not YAML: {e}"))
}

/// The operations of the one patch that the component makes for the base object of `kind`. The patch
/// names the object of the base by its kind and its name.
fn component_ops(kind: &str) -> Vec<Value> {
    let component = pods_doc("kustomization.yaml");
    let patches: Vec<&Value> = component["patches"]
        .as_sequence()
        .unwrap()
        .iter()
        .filter(|p| p["target"]["kind"] == kind)
        .collect();
    assert_eq!(patches.len(), 1, "one patch for the {kind}");
    let base = deploy_doc(&format!("{}.yaml", kind.to_lowercase()));
    assert_eq!(base["kind"], kind);
    assert_eq!(patches[0]["target"]["name"], base["metadata"]["name"]);
    serde_yaml::from_str(patches[0]["patch"].as_str().unwrap())
        .unwrap_or_else(|e| panic!("the patch of the {kind} is no list of operations: {e}"))
}

/// Add `value` at `path`, a JSON pointer, as the operation `add` of a JSON patch does. The last
/// segment `-` appends to a sequence.
fn add_at(doc: &mut Value, path: &str, value: Value) {
    let (parent, last) = path.rsplit_once('/').expect("a path has a slash");
    let mut node = doc;
    for part in parent.split('/').skip(1) {
        node = match node {
            Value::Sequence(items) => &mut items[part.parse::<usize>().expect("an index")],
            Value::Mapping(map) => map
                .get_mut(part)
                .unwrap_or_else(|| panic!("{path}: no key {part}")),
            _ => panic!("{path}: {part} is in no object"),
        };
    }
    match node {
        Value::Sequence(items) if last == "-" => items.push(value),
        Value::Sequence(items) => items.insert(last.parse().expect("an index"), value),
        Value::Mapping(map) => {
            map.insert(last.into(), value);
        }
        _ => panic!("{path}: the parent is no object"),
    }
}

/// The object of `base` after the patch of the component: what a team renders with the component.
/// The component only adds.
fn with_component(kind: &str, base: &str) -> Value {
    let mut doc = deploy_doc(base);
    for op in component_ops(kind) {
        assert_eq!(op["op"], "add", "the component only adds");
        add_at(&mut doc, op["path"].as_str().unwrap(), op["value"].clone());
    }
    doc
}

/// The pod spec of the Deployment with the component.
fn pod_with_component() -> Value {
    with_component("Deployment", "deployment.yaml")["spec"]["template"]["spec"].clone()
}

/// The string that the Go constant `name` holds in `source`, as in `tokenFile = "token"`.
fn go_const(source: &str, name: &str) -> String {
    source
        .lines()
        .find_map(|line| {
            let (left, right) = line.split_once('=')?;
            (left.trim().trim_start_matches("const ").trim() == name)
                .then(|| right.trim().trim_matches('"').to_string())
        })
        .unwrap_or_else(|| panic!("no constant {name}"))
}

/// The role holds `get` and `list` on `pods`, and nothing else: no other verb, no other resource, no
/// other group, no second rule, no name list, and no URL. A test of the whole `rules` value fails for
/// each addition.
#[test]
fn the_cluster_role_of_the_component_holds_get_and_list_on_pods_and_nothing_else() {
    let role = pods_doc("clusterrole.yaml");
    assert_eq!(role["kind"], "ClusterRole");
    let expected: Value =
        serde_yaml::from_str("[{apiGroups: [''], resources: [pods], verbs: [get, list]}]").unwrap();
    assert_eq!(role["rules"], expected);
    let keys: Vec<&str> = role
        .as_mapping()
        .unwrap()
        .keys()
        .map(|k| k.as_str().unwrap())
        .collect();
    assert_eq!(
        keys,
        ["apiVersion", "kind", "metadata", "rules"],
        "an aggregation rule would add rules"
    );
}

#[test]
fn the_binding_gives_the_role_to_the_service_account_of_the_observer_alone() {
    let role = pods_doc("clusterrole.yaml");
    let binding = pods_doc("clusterrolebinding.yaml");
    assert_eq!(binding["kind"], "ClusterRoleBinding");
    assert_eq!(binding["roleRef"]["apiGroup"], "rbac.authorization.k8s.io");
    assert_eq!(binding["roleRef"]["kind"], "ClusterRole");
    assert_eq!(binding["roleRef"]["name"], role["metadata"]["name"]);
    let subjects = binding["subjects"].as_sequence().unwrap();
    assert_eq!(subjects.len(), 1);
    assert_eq!(subjects[0]["kind"], "ServiceAccount");
    assert_eq!(
        subjects[0]["name"],
        deploy_doc("serviceaccount.yaml")["metadata"]["name"]
    );
    assert_eq!(subjects[0]["name"], pod()["serviceAccountName"]);
    assert_eq!(
        subjects[0]["namespace"],
        deploy_doc("kustomization.yaml")["namespace"],
        "a binding names the namespace of the account"
    );
}

/// The base keeps its promise: no role for the observer, no token, and no flag that reads pods. The
/// flag is off by default in the code. The component is the only source of the three.
#[test]
fn the_base_has_no_role_no_token_and_no_read_pods_flag_and_the_code_keeps_the_flag_off() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("deploy/observer");
    for entry in std::fs::read_dir(dir).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().into_owned();
        if !name.ends_with(".yaml") {
            continue;
        }
        let kind = deploy_doc(&name)["kind"].as_str().unwrap().to_string();
        assert!(
            !["ClusterRole", "ClusterRoleBinding", "RoleBinding"].contains(&kind.as_str()),
            "{name} is a {kind}: the base gives the observer no role"
        );
        assert!(
            kind != "Role" || name == "reader-role.yaml",
            "{name}: the only Role is the one for a person"
        );
    }
    let base_container = container();
    for arg in &strings(&base_container["args"])[1..] {
        let (flag, _) = flag_of(arg);
        assert!(
            !["read-pods", "kube-dir"].contains(&flag),
            "the base has {arg}"
        );
    }
    for volume in pod()["volumes"].as_sequence().unwrap() {
        for source in volume["projected"]["sources"]
            .as_sequence()
            .into_iter()
            .flatten()
        {
            assert!(
                source.get("serviceAccountToken").is_none(),
                "the base has a token in {volume:?}"
            );
        }
    }
    for mount in container()["volumeMounts"].as_sequence().unwrap() {
        assert!(
            !mount["mountPath"]
                .as_str()
                .unwrap()
                .starts_with("/var/run/secrets"),
            "the base mounts a token: {mount:?}"
        );
    }
    let main = repo_file("observer/main.go");
    assert!(
        main.contains(r#""read-pods", false"#),
        "the observer reads no pod unless the flag says so"
    );
}

/// The component adds the two flags to the container. Both are flags of the observer. The directory of
/// `--kube-dir` is the mount of the volume and the default of the code, and the files of the volume
/// are the files that the code reads.
#[test]
fn the_component_adds_the_flags_read_pods_and_kube_dir_and_mounts_the_volume_where_the_code_reads()
{
    let base_container = container();
    let base = strings(&base_container["args"]);
    let pod = pod_with_component();
    let container = &pod["containers"][0];
    let args = strings(&container["args"]);
    assert_eq!(
        args[..base.len()],
        base[..],
        "the base arguments stay first"
    );
    let added: Vec<(&str, Option<&str>)> = args[base.len()..].iter().map(|a| flag_of(a)).collect();
    let kube_dir = "/var/run/secrets/kubernetes.io/serviceaccount";
    assert_eq!(
        added,
        [("read-pods", None), ("kube-dir", Some(kube_dir))],
        "the component adds the two flags"
    );

    let main = repo_file("observer/main.go");
    for (flag, _) in &added {
        assert!(
            main.contains(&format!("\"{flag}\"")),
            "the observer has no flag --{flag}"
        );
    }
    let source = repo_file("observer/internal/kube/kube.go");
    assert_eq!(go_const(&source, "ServiceDir"), kube_dir);
    assert!(
        main.contains(r#""kube-dir", kube.ServiceDir"#),
        "the flag defaults to the directory of the code"
    );

    let mount = container["volumeMounts"]
        .as_sequence()
        .unwrap()
        .iter()
        .find(|m| m["name"] == KUBE_VOLUME)
        .expect("the container mounts the volume");
    assert_eq!(mount["mountPath"], kube_dir);
    assert_eq!(mount["readOnly"], true);
}

/// The token expires in one hour and has the audience of the API server. The CA comes from the
/// `ConfigMap` that each namespace holds. The base pod keeps `automountServiceAccountToken: false`:
/// the volume of the component is the only token. Its name starts with no `kube-api-access`, the
/// prefix of the volume that the cluster adds, so that a check of the base still means something.
#[test]
fn the_projected_volume_gives_a_token_that_expires_in_one_hour_and_the_ca_of_the_cluster() {
    let pod = pod_with_component();
    assert_eq!(pod["automountServiceAccountToken"], false);
    let volume = pod["volumes"]
        .as_sequence()
        .unwrap()
        .iter()
        .find(|v| v["name"] == KUBE_VOLUME)
        .expect("the pod has the volume");
    assert!(!KUBE_VOLUME.starts_with("kube-api-access"));
    let sources = volume["projected"]["sources"].as_sequence().unwrap();
    assert_eq!(sources.len(), 2, "the token and the CA");

    let kube = repo_file("observer/internal/kube/kube.go");
    let token = &sources[0]["serviceAccountToken"];
    assert_eq!(token["expirationSeconds"], 3600);
    assert_eq!(token["path"], go_const(&kube, "tokenFile").as_str());
    assert!(
        token.get("audience").is_none(),
        "the default audience is the API server"
    );

    let ca = &sources[1]["configMap"];
    assert_eq!(ca["name"], "kube-root-ca.crt");
    let item = &ca["items"][0];
    assert_eq!(ca["items"].as_sequence().unwrap().len(), 1);
    assert_eq!(item["key"], "ca.crt");
    assert_eq!(item["path"], go_const(&kube, "caFile").as_str());
}

/// The component opens TCP 6443 beside 443, and nothing else. The policy keeps DNS and 443, and it
/// still lets no pod connect in.
#[test]
fn the_component_opens_tcp_6443_beside_443_and_changes_nothing_else_in_the_policy() {
    let base = deploy_doc("networkpolicy.yaml");
    let policy = with_component("NetworkPolicy", "networkpolicy.yaml");
    let mut expected = egress_ports(&base);
    assert!(
        expected.contains(&("TCP".into(), 443)),
        "the base keeps 443"
    );
    expected.push(("TCP".into(), 6443));
    assert_eq!(egress_ports(&policy), expected);
    assert!(
        policy["spec"]["egress"]
            .as_sequence()
            .unwrap()
            .iter()
            .all(|rule| rule.get("to").is_none()),
        "a team narrows the address in its own copy"
    );
    assert_eq!(policy["spec"]["ingress"], base["spec"]["ingress"]);
    assert_eq!(policy["spec"]["policyTypes"], base["spec"]["policyTypes"]);
    assert_eq!(policy["spec"]["podSelector"], base["spec"]["podSelector"]);
}

/// The kustomization of the component lists each file of its folder, and each file holds one object.
/// The folder `example/` is a kustomization of a team that lists the base and the component.
#[test]
fn the_component_lists_its_files_and_each_holds_one_object() {
    let component = pods_doc("kustomization.yaml");
    assert_eq!(component["apiVersion"], "kustomize.config.k8s.io/v1alpha1");
    assert_eq!(component["kind"], "Component");
    let listed = strings(&component["resources"]);
    assert_eq!(listed, COMPONENT.map(|(file, _)| file));
    let mut on_disk: Vec<String> =
        std::fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("deploy/observer/pods"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".yaml") && name != "kustomization.yaml")
            .collect();
    on_disk.sort();
    assert_eq!(on_disk, listed, "each file of the folder is in the list");
    for (file, kind) in COMPONENT {
        let source = repo_file(&format!("deploy/observer/pods/{file}"));
        assert!(
            !source.lines().any(|line| line == "---"),
            "{file} holds one object"
        );
        assert_eq!(pods_doc(file)["kind"], kind, "{file}");
    }
    let example: Value = serde_yaml::from_str(&repo_file(
        "deploy/observer/pods/example/kustomization.yaml",
    ))
    .unwrap();
    assert_eq!(strings(&example["resources"]), ["../.."]);
    assert_eq!(strings(&example["components"]), [".."]);
}

/// A stand-in `kubectl` for `kubectl kustomize deploy/observer/pods/example`. It prints the four files
/// of the base and the two of the component, with a separator. A real `kubectl` applies the patch of
/// the component. The stand-in prints the line that the patch adds, unless `FAKE_NO_FLAG` is set.
/// `FAKE_EXTRA` adds one object.
const FAKE_KUBECTL_KUSTOMIZE_PODS: &str = r#"#!/usr/bin/env bash
[ "$1 $2" = "kustomize deploy/observer/pods/example" ] || exit 2
for f in namespace serviceaccount networkpolicy deployment pods/clusterrole pods/clusterrolebinding; do
  cat "deploy/observer/$f.yaml"
  echo "---"
done
[ -n "${FAKE_NO_FLAG-}" ] || echo "        - --read-pods"
[ -z "${FAKE_EXTRA-}" ] || printf 'apiVersion: v1\nkind: ConfigMap\n---\n'
"#;

#[test]
fn the_recipe_observer_manifests_pods_passes_for_six_objects_and_the_flag_and_fails_otherwise() {
    let run = |env: &[(&str, &str)]| {
        run_recipe("observer-manifests-pods", FAKE_KUBECTL_KUSTOMIZE_PODS, env)
    };
    assert_eq!(run(&[]), Some(0), "the six objects and the flag");
    assert_eq!(run(&[("FAKE_EXTRA", "1")]), Some(1), "seven objects");
    assert_eq!(
        run(&[("FAKE_NO_FLAG", "1")]),
        Some(1),
        "no flag --read-pods"
    );
}

// ---- the steps of the page "The observer" ------------------------------------------------------------

/// The section "Install" of the page of the service, up to the next section.
fn install_steps() -> String {
    page_section("docs/src/automation/observer.md", "## Install")
}

/// The word of `block` after the word `before`.
fn word_after<'a>(block: &'a str, before: &str) -> &'a str {
    let mut words = block.split_whitespace();
    words
        .by_ref()
        .find(|word| *word == before)
        .unwrap_or_else(|| panic!("no word {before:?}"));
    words.next().expect("a word follows")
}

/// Step 2 of the install makes the Secret that the image pull needs. The token goes on standard
/// input, as the key does, and no argument holds it.
#[test]
fn step_2_of_the_install_makes_the_pull_secret_from_a_token_on_standard_input() {
    let block = block_with(&install_steps(), "bash", "create secret generic ghcr-pull");
    let dir = tempfile::tempdir().unwrap();
    let token = "s3cr3t-token";
    let o = run_with_kubectl(
        dir.path(),
        &block.replace("<github user>", "octo"),
        &[("GHCR_PULL_TOKEN", token)],
    );
    assert!(o.status.success(), "{}", text(&o.stderr));

    let namespace = deploy_doc("kustomization.yaml")["namespace"]
        .as_str()
        .unwrap()
        .to_string();
    let args = kubectl_args(dir.path());
    assert_eq!(
        args.trim(),
        format!(
            "-n {namespace} create secret generic ghcr-pull --type=kubernetes.io/dockerconfigjson --from-file=.dockerconfigjson=/dev/stdin"
        )
    );
    let config: serde_json::Value = serde_json::from_str(&kubectl_input(dir.path())).unwrap();
    let entry = &config["auths"]["ghcr.io"];
    assert_eq!(entry["username"], "octo");
    assert_eq!(entry["password"], token);
    // base64 of "octo:s3cr3t-token"
    assert_eq!(entry["auth"], "b2N0bzpzM2NyM3QtdG9rZW4=");
}

/// Step 6 names the folder, the image, and a patch. The patch adds the pull Secret of step 2 to the
/// Deployment of the folder, which has none.
#[test]
fn step_6_of_the_install_patches_the_deployment_of_the_folder_with_the_pull_secret_of_step_2() {
    let steps = install_steps();
    let secret = word_after(
        &block_with(&steps, "bash", "create secret generic ghcr-pull"),
        "generic",
    )
    .to_string();
    let yaml = block_with(&steps, "yaml", "kind: Kustomization");
    let kustomization: Value = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(strings(&kustomization["resources"]), ["deploy/observer"]);

    let image = container()["image"].as_str().unwrap().to_string();
    let (name, _) = image.rsplit_once(':').expect("the image names a tag");
    assert_eq!(kustomization["images"][0]["name"], name);

    let deployment = deploy_doc("deployment.yaml");
    let patch = &kustomization["patches"][0];
    assert_eq!(patch["target"]["kind"], deployment["kind"]);
    assert_eq!(patch["target"]["name"], deployment["metadata"]["name"]);
    let ops: Value = serde_yaml::from_str(patch["patch"].as_str().unwrap()).unwrap();
    let ops = ops.as_sequence().unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0]["op"], "add");
    assert_eq!(ops[0]["path"], "/spec/template/spec/imagePullSecrets");
    assert_eq!(ops[0]["value"][0]["name"], secret.as_str());
    assert!(
        pod().get("imagePullSecrets").is_none(),
        "the manifest has no pull Secret, so the patch is the only source"
    );
}

/// Step 7 applies the reader role, which the kustomization omits, and binds it in its namespace.
#[test]
fn step_7_of_the_install_binds_the_reader_role_of_the_folder_in_its_namespace() {
    let block = block_with(&install_steps(), "bash", "create rolebinding");
    let role = deploy_doc("reader-role.yaml");
    assert!(block.contains("kubectl apply -f deploy/observer/reader-role.yaml"));
    assert_eq!(
        word_after(&block, "-n"),
        role["metadata"]["namespace"].as_str().unwrap()
    );
    let role_flag = format!("--role={}", role["metadata"]["name"].as_str().unwrap());
    let words: Vec<&str> = block.split_whitespace().collect();
    assert!(words.contains(&role_flag.as_str()), "{block}");
    assert!(
        words
            .iter()
            .any(|w| w.starts_with("--group=") || w.starts_with("--user=")),
        "{block}"
    );
}

/// The live run makes the same Secret, patch, and binding. So a person who follows the steps
/// meets what the run checked. The Secret and the patch are in `lib.sh`, which `observer.sh` sources.
#[test]
fn the_live_run_makes_the_pull_secret_the_patch_and_the_binding_of_the_install_steps() {
    let live = repo_file("examples/live/observer.sh") + &repo_file("examples/live/lib.sh");
    for needle in [
        "create secret generic ghcr-pull --type=kubernetes.io/dockerconfigjson --from-file=.dockerconfigjson=/dev/stdin",
        "path: /spec/template/spec/imagePullSecrets",
        "- name: ghcr-pull",
        "create rolebinding reader --role=strata-observer-reader",
    ] {
        assert!(
            live.contains(needle),
            "observer.sh and lib.sh lack {needle:?}"
        );
    }
}
