//! The manifests and the image of the observer: `deploy/observer/`. No cluster and no Docker: a test
//! reads the files, and a stand-in `kubectl` stands for the render. Each test ties a manifest to the
//! code or to the script that it depends on, so that the two cannot drift.

use std::path::Path;
use std::process::Command;

use serde_yaml::Value;

use crate::common::{
    CommandExt, fake_bin, path_with, recipe, repo_file, script, steps, text, workflow,
    workflow_text,
};

/// A file of `deploy/observer/`, parsed as one YAML document.
fn manifest(file: &str) -> Value {
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

/// The pod template of the Deployment.
fn pod() -> Value {
    manifest("deployment.yaml")["spec"]["template"]["spec"].clone()
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
    let kustomization = manifest("kustomization.yaml");
    assert_eq!(kustomization["namespace"], "strata-observer");
    let resources = strings(&kustomization["resources"]);
    assert_eq!(resources, RENDERED.map(|(file, _)| file));
    for (file, kind) in RENDERED {
        let source = repo_file(&format!("deploy/observer/{file}"));
        assert!(
            !source.lines().any(|line| line == "---"),
            "{file} holds one object: the kustomization renders four"
        );
        assert_eq!(manifest(file)["kind"], kind, "{file}");
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

#[test]
fn the_recipe_observer_manifests_passes_for_four_objects_and_fails_for_any_other_count() {
    let lines = recipe("observer-manifests");
    assert_eq!(lines.len(), 1, "{lines:?}");
    let dir = tempfile::tempdir().unwrap();
    let bin = fake_bin(dir.path(), &[("kubectl", FAKE_KUBECTL_KUSTOMIZE)]);
    let run = |extra: Option<&str>| {
        let mut command = Command::new("bash");
        command
            .args(["--noprofile", "--norc", "-c", &lines[0]])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .env("PATH", path_with(&bin))
            .env_remove("FAKE_EXTRA");
        if let Some(extra) = extra {
            command.env("FAKE_EXTRA", extra);
        }
        command.output_locked().unwrap().status.code()
    };
    assert_eq!(run(None), Some(0), "the four objects of the repository");
    assert_eq!(run(Some("1")), Some(1), "five objects");
}

#[test]
fn the_namespace_enforces_the_restricted_pod_security_standard() {
    let namespace = manifest("namespace.yaml");
    assert_eq!(namespace["metadata"]["name"], "strata-observer");
    assert_eq!(
        namespace["metadata"]["labels"]["pod-security.kubernetes.io/enforce"],
        "restricted"
    );
}

#[test]
fn the_service_account_and_the_pod_get_no_token_and_the_namespace_has_no_role() {
    assert_eq!(
        manifest("serviceaccount.yaml")["automountServiceAccountToken"],
        false
    );
    assert_eq!(pod()["automountServiceAccountToken"], false);
    assert_eq!(pod()["serviceAccountName"], "strata-observer");
    // the observer calls no Kubernetes API, so nothing in the kustomization gives a role
    for (file, _) in RENDERED {
        let kind = manifest(file)["kind"].as_str().unwrap().to_string();
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
    assert_eq!(manifest("deployment.yaml")["spec"]["replicas"], 1);
    assert_eq!(
        manifest("deployment.yaml")["spec"]["strategy"]["type"],
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
        let (flag, value) = arg
            .strip_prefix("--")
            .and_then(|a| a.split_once('='))
            .unwrap_or_else(|| panic!("{arg} is no flag with a value"));
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
    let kustomization = manifest("kustomization.yaml");
    for ((name, _), (written_name, namespace)) in mounted.iter().zip(&written) {
        assert_eq!(name, written_name);
        assert_eq!(*namespace, kustomization["namespace"].as_str().unwrap());
    }
    assert_eq!(mounted.len(), written.len());
}

#[test]
fn the_network_policy_lets_no_pod_connect_in_and_lets_the_observer_connect_to_dns_and_https() {
    let policy = manifest("networkpolicy.yaml");
    let label = &manifest("deployment.yaml")["spec"]["template"]["metadata"]["labels"];
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
    let ports: Vec<(String, u64)> = policy["spec"]["egress"]
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
        .collect();
    assert_eq!(
        ports,
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
    let role = manifest("reader-role.yaml");
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
