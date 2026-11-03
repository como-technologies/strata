//! The scripts under scripts/ci/, which ship in a release and run in a pipeline.
//! No network: every host and every command outside this repository is a recording stand-in.

use crate::common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::*;
use strata::api::codes::UPGRADE_NOT_SUPPORTED_CODE;

/// The scripts a release carries. The release workflow installs the published tag and runs the
/// help of each one; this is the same check, before a tag exists.
const SHIPPED: [&str; 6] = [
    "pr-comment.sh",
    "merge-gate.sh",
    "verify-report.sh",
    "upstream-report.sh",
    "preflight.sh",
    "install-strata.sh",
];

#[test]
fn every_script_a_release_carries_prints_its_help_and_exits_0() {
    for name in SHIPPED {
        let o = Command::new("bash")
            .arg(script(&format!("ci/{name}")))
            .arg("--help")
            .output_locked()
            .unwrap();
        assert_eq!(
            o.status.code(),
            Some(0),
            "{name} --help: {}",
            text(&o.stderr)
        );
        let help = text(&o.stdout);
        assert!(
            help.contains(name),
            "{name} --help names the script:\n{help}"
        );
        assert!(
            help.lines().count() >= 5,
            "{name} --help says what it does:\n{help}"
        );
        assert!(
            !help.contains("scripts/pr-comment.sh") && !help.contains("scripts/verify-report.sh"),
            "{name} --help names the path the script lives at now:\n{help}"
        );
    }
}

/// The line of each script that prints its help: the comment block from line 2 to the first line
/// of `set -euo`. A script ships alone and cannot source a function, so each one has this line.
const HELP_LINE: &str = r#"sed -n '2,/^set -euo/{/^#/p}' "$0" | sed 's/^# \{0,1\}//'"#;

#[test]
fn each_script_prints_its_whole_header_as_its_help_from_one_line() {
    for name in SHIPPED {
        let source = repo_file(&format!("scripts/ci/{name}"));
        assert!(
            source.contains(HELP_LINE),
            "{name} prints its help from a line range, or in another form than the others"
        );
        // the header ends at the line before the first `set -euo`, and each line of it is a comment
        let expected: Vec<&str> = source
            .lines()
            .skip(1)
            .take_while(|l| !l.starts_with("set -euo"))
            .map(|l| {
                l.strip_prefix("# ")
                    .or_else(|| l.strip_prefix('#'))
                    .unwrap_or_else(|| panic!("{name}: a line of the header is not a comment: {l}"))
            })
            .collect();
        assert_eq!(
            help_of(name).lines().collect::<Vec<_>>(),
            expected,
            "{name} --help is not its header, line for line"
        );
    }
}

/// The files a team copies, and the reusable workflows they call. They name scripts, options and a
/// release version, and nothing else reads them: a renamed option or a moved script breaks a
/// drop-in silently.
const DROP_INS: [&str; 8] = [
    "examples/ci/github/strata.yml",
    "examples/ci/gitlab/strata.yml",
    ".github/workflows/comment.yml",
    ".github/workflows/verify.yml",
    ".github/workflows/upstream.yml",
    ".github/workflows/merge-gate.yml",
    ".github/workflows/preflight.yml",
    ".github/workflows/preflight-comment.yml",
];

/// The `--help` of one script that ships.
fn help_of(name: &str) -> String {
    let o = Command::new("bash")
        .arg(script(&format!("ci/{name}")))
        .arg("--help")
        .output_locked()
        .unwrap();
    assert!(o.status.success(), "{name} --help failed");
    text(&o.stdout)
}

#[test]
fn the_ci_drop_ins_name_only_scripts_that_ship() {
    for file in DROP_INS {
        for token in repo_file(file).split(|c: char| c.is_whitespace() || c == '"' || c == '\'') {
            let token = token.trim_end_matches(&[',', ';', ')'][..]);
            if !token.ends_with(".sh") {
                continue;
            }
            let name = token.rsplit('/').next().unwrap();
            assert!(
                SHIPPED.contains(&name),
                "{file} names {token}, which no release carries"
            );
            // a path under scripts/ is this repository's copy, and must be where it says
            if let Some(rest) = token.split_once("scripts/").map(|(_, r)| r) {
                let at = script(rest);
                assert!(at.exists(), "{file} names {token}, which is not at {at:?}");
            }
        }
    }
}

#[test]
fn the_ci_drop_ins_pass_only_options_the_scripts_have() {
    let helps: Vec<(&str, String)> = SHIPPED.iter().map(|n| (*n, help_of(n))).collect();
    let known = |flag: &str| helps.iter().any(|(_, h)| whole_word(h, flag, &['-']));
    for file in DROP_INS {
        let body = repo_file(file);
        let mut lines = body.lines().peekable();
        while let Some(line) = lines.next() {
            let Some(name) = SHIPPED.iter().find(|n| line.contains(**n)) else {
                continue;
            };
            // the call, and the lines that continue it: a YAML scalar wraps without a backslash
            let mut block = line.to_string();
            let indent = line.len() - line.trim_start().len();
            while let Some(next) = lines.peek() {
                let deeper = next.len() - next.trim_start().len() > indent;
                // "- " starts a list item; "--label" continues the command above it
                let new_key = next.trim_start().starts_with("- ") || next.trim_end().ends_with(':');
                if !deeper || new_key || next.trim().is_empty() {
                    break;
                }
                block.push(' ');
                block.push_str(lines.next().unwrap());
            }
            for word in block.split(|c: char| c.is_whitespace() || c == '"' || c == '\'') {
                if !word.starts_with("--") || word.len() < 4 {
                    continue;
                }
                let flag = word.trim_end_matches(&[',', ';', ')'][..]);
                assert!(
                    known(flag),
                    "{file} passes {flag} to {name}, and no script that ships has it"
                );
            }
        }
        // a flag the file carries another way: quoted in an environment value, such as
        // '--no-verify', or assigned to a shell variable, such as rollback=--allow-rollback.
        // Line by line, and never a comment, because prose holds apostrophes.
        for line in body.lines() {
            if line.trim_start().starts_with('#') {
                continue;
            }
            let mut carried: Vec<&str> = line
                .split('\'')
                .skip(1)
                .step_by(2)
                .filter(|q| q.starts_with("--") && !q.contains(' '))
                .collect();
            carried.extend(
                line.split(|c: char| c.is_whitespace() || c == '"' || c == '\'')
                    .filter_map(|w| w.split_once("=--").map(|(_, f)| f))
                    .filter(|f| !f.is_empty())
                    .map(|f| {
                        let end = f
                            .find(|c: char| !c.is_ascii_alphanumeric() && c != '-')
                            .unwrap_or(f.len());
                        &f[..end]
                    })
                    .map(|f| Box::leak(format!("--{f}").into_boxed_str()) as &str),
            );
            for flag in carried {
                let flag = flag.trim_end_matches(&[';', ',', ')'][..]);
                if flag.len() < 4 {
                    continue;
                }
                assert!(
                    known(flag),
                    "{file} carries {flag}, and no script that ships has it"
                );
            }
        }
    }
}

#[test]
fn the_ci_drop_ins_name_the_version_this_repository_is_at() {
    let version = format!("v{}", version_of("Cargo.toml"));
    for file in DROP_INS {
        for token in repo_file(file).split(|c: char| c.is_whitespace() || c == '"' || c == '\'') {
            let token = token.trim_end_matches(&[',', ';', ')', '`'][..]);
            // the pin of an action of another repository, such as sigstore/cosign-installer@v4.1.2
            if token.contains('@') && !token.starts_with("como-technologies/") {
                continue;
            }
            let named = token.rsplit('@').next().unwrap_or(token);
            let looks_like_a_tag = named.starts_with('v')
                && named.len() > 3
                && named[1..].split('.').count() == 3
                && named[1..]
                    .split('.')
                    .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
            if looks_like_a_tag {
                assert_eq!(
                    named, version,
                    "{file} names {named}, and this repository is at {version}: a release bump \
                     must move every drop-in with it"
                );
            }
        }
    }
}

/// The release workflow installs what it published and runs each script the installer installed.
/// The two lists are written in two files, so they can disagree. The check named
/// `install-strata.sh`, which the installer never installs, shows this: the first release to run
/// the check failed on it. This test holds the two lists together.
#[test]
fn the_release_check_runs_the_scripts_the_installer_installs() {
    let installer = repo_file("scripts/ci/install-strata.sh");
    let release = workflow_text("release.yml");

    // `scripts=(pr-comment.sh merge-gate.sh verify-report.sh)` in the installer. --bin-only empties
    // that list at run time; this is the declaration, which names every script a full install gets.
    let installs: Vec<&str> = installer
        .lines()
        .find(|l| l.trim_start().starts_with("scripts=(") && l.contains(".sh"))
        .expect("the installer names the scripts it installs")
        .trim()
        .trim_start_matches("scripts=(")
        .trim_end_matches(')')
        .split_whitespace()
        .collect();

    // `for s in pr-comment.sh merge-gate.sh verify-report.sh; do` in the release check
    let checks: Vec<&str> = release
        .lines()
        .find(|l| l.trim_start().starts_with("for s in ") && l.contains(".sh"))
        .expect("the release workflow names the scripts it runs")
        .trim()
        .trim_start_matches("for s in ")
        .trim_end_matches("; do")
        .split_whitespace()
        .collect();

    assert!(
        !installs.is_empty() && !checks.is_empty(),
        "the parsing broke"
    );
    assert_eq!(
        checks, installs,
        "the release workflow runs scripts the installer does not install, or misses one it does"
    );
}

// ---- the options of the tools that a pipeline wraps -------------------------------------------

/// Where a tool lists its options.
enum Listed {
    /// A script of `scripts/ci/`: the usage lines of its help.
    Script(&'static str),
    /// A command of strata: the `Options:` of the help that the book prints.
    Command(&'static str),
}

impl Listed {
    /// Each option of the tool, except `--help` and `--version`.
    fn options(&self) -> BTreeSet<String> {
        let mut found = BTreeSet::new();
        let flag_of = |token: &str| {
            let flag = token.trim_matches(['[', ']', ',', ';']);
            flag.starts_with("--").then(|| flag.to_string())
        };
        match self {
            Listed::Script(name) => {
                // `  scripts/ci/NAME <ref>... [--a] [--b X]`, and the indented lines that go on
                let mut in_usage = false;
                for line in help_of(name).lines() {
                    let begins =
                        line.starts_with(' ') && line.trim_start().starts_with("scripts/ci/");
                    in_usage = begins || (in_usage && line.starts_with(' '));
                    if in_usage {
                        found.extend(line.split_whitespace().filter_map(flag_of));
                    }
                }
            }
            Listed::Command(name) => {
                let help = repo_file(&format!("docs/src/output/help-{name}.txt"));
                let mut in_options = false;
                for line in help.lines() {
                    if line.trim_start().starts_with('-') && in_options {
                        // `  -q, --quiet` and `      --format <FORMAT>`: the first long name
                        found.extend(line.split_whitespace().find_map(flag_of));
                    }
                    in_options = line == "Options:" || (in_options && !line.is_empty());
                }
            }
        }
        found.remove("--help");
        found.remove("--version");
        found
    }
}

/// One option that a pipeline sets: the reusable workflow takes it from an input, and the GitLab
/// job from a variable. An empty `input` or `variable` means the wrapper sets the option itself,
/// from the event or from the key, and offers no setting for it.
struct Reach {
    flag: &'static str,
    input: &'static str,
    variable: &'static str,
}

const fn reach(flag: &'static str, input: &'static str, variable: &'static str) -> Reach {
    Reach {
        flag,
        input,
        variable,
    }
}

/// Options that a wrapper does not pass on purpose, with the reason.
struct Left {
    flags: &'static [&'static str],
    why: &'static str,
}

/// The options that choose how a command prints. A gate prints to the log of its job, and its
/// exit status is the verdict.
const PRINTING: Left = Left {
    flags: &[
        "--format",
        "--json",
        "--color",
        "--no-color",
        "--compact",
        "--quiet",
    ],
    why: "the gate prints to the log of the job, and its exit status is the verdict",
};

/// A reusable workflow and the GitLab job that wrap the same tool.
struct Wrapper {
    workflow: &'static str,
    job: &'static str,
    tool: Listed,
    reaches: &'static [Reach],
    left: &'static [Left],
}

const HOST: Left = Left {
    flags: &["--host"],
    why: "the script picks the host from the CI variables",
};

const DRY_RUN: Left = Left {
    flags: &["--dry-run"],
    why: "it touches no host, so a person tries it at a terminal",
};

const WRAPPERS: [Wrapper; 7] = [
    Wrapper {
        workflow: "comment.yml",
        job: "strata-comment",
        tool: Listed::Script("pr-comment.sh"),
        reaches: &[
            reach("--pr", "", ""),
            reach("--repo", "", ""),
            reach("--label", "label", "STRATA_LABEL"),
            reach("--threshold", "threshold", "STRATA_THRESHOLD"),
            reach("--repave", "repave", "STRATA_REPAVE"),
        ],
        left: &[
            HOST,
            DRY_RUN,
            Left {
                flags: &["--preflight"],
                why: "preflight-comment.yml posts the report of a preflight",
            },
        ],
    },
    Wrapper {
        workflow: "preflight-comment.yml",
        // GitLab has no workflow of its own for it: preflight.sh posts the note in the job of the
        // preflight, when STRATA_PREFLIGHT_COMMENT is true
        job: "strata-preflight",
        tool: Listed::Script("pr-comment.sh"),
        reaches: &[
            reach("--pr", "", ""),
            reach("--repo", "", ""),
            reach("--preflight", "", ""),
        ],
        left: &[
            HOST,
            DRY_RUN,
            Left {
                flags: &["--label", "--threshold", "--repave"],
                why: "the preflight comment compares nothing and sets no label",
            },
        ],
    },
    Wrapper {
        workflow: "merge-gate.yml",
        job: "strata-merge-gate",
        tool: Listed::Script("merge-gate.sh"),
        reaches: &[
            reach("--no-verify", "verify", ""),
            reach(
                "--allow-rollback",
                "rollback-label",
                "STRATA_ROLLBACK_LABEL",
            ),
            reach(
                "--allow-no-digest",
                "allow-no-digest",
                "STRATA_ALLOW_NO_DIGEST",
            ),
        ],
        left: &[
            Left {
                flags: &["--base"],
                why: "the workflow sets GITHUB_BASE_REF from its input base",
            },
            Left {
                flags: &["--remote"],
                why: "the checkout of the job names its remote origin",
            },
        ],
    },
    Wrapper {
        workflow: "verify.yml",
        job: "strata-verify",
        tool: Listed::Script("verify-report.sh"),
        reaches: &[
            reach("--repo", "", ""),
            reach("--label", "label", "STRATA_DRIFT_LABEL"),
            reach(
                "--allow-no-digest",
                "allow-no-digest",
                "STRATA_ALLOW_NO_DIGEST",
            ),
            reach("--who", "who", "STRATA_WHO"),
        ],
        left: &[HOST, DRY_RUN],
    },
    Wrapper {
        workflow: "upstream.yml",
        job: "strata-upstream",
        tool: Listed::Script("upstream-report.sh"),
        reaches: &[
            reach("--repo", "", ""),
            reach("--label", "label", "STRATA_BEHIND_LABEL"),
            reach("--no-impact", "no-impact", "STRATA_NO_IMPACT"),
            reach("--save", "save", "STRATA_SAVE"),
        ],
        left: &[HOST, DRY_RUN],
    },
    Wrapper {
        workflow: "promote-gate.yml",
        job: "strata-promote-gate",
        tool: Listed::Command("same-release"),
        reaches: &[reach("--allow", "allow", "STRATA_PROMOTE_ALLOW")],
        left: &[PRINTING],
    },
    Wrapper {
        workflow: "retire-gate.yml",
        job: "strata-retire-gate",
        tool: Listed::Command("consumers"),
        reaches: &[],
        left: &[
            PRINTING,
            Left {
                flags: &["--list-only"],
                why: "the gate reads each cluster, because the list of Palette can omit a consumer",
            },
        ],
    },
];

/// The options of a tool that a wrapper neither sets nor omits on purpose. Also the options that
/// the wrapper names and the tool does not have.
fn unaccounted(options: &BTreeSet<String>, reaches: &[Reach], left: &[Left]) -> Vec<String> {
    let named: BTreeSet<&str> = reaches
        .iter()
        .map(|r| r.flag)
        .chain(left.iter().flat_map(|l| l.flags.iter().copied()))
        .collect();
    let mut wrong: Vec<String> = options
        .iter()
        .filter(|o| !named.contains(o.as_str()))
        .map(|o| format!("{o} is an option of the tool, and nothing says what the wrapper does"))
        .collect();
    wrong.extend(
        named
            .iter()
            .filter(|n| !options.contains(**n))
            .map(|n| format!("{n} is not an option of the tool")),
    );
    wrong.extend(
        left.iter()
            .filter(|l| l.why.is_empty())
            .map(|l| format!("{:?} is omitted with no reason", l.flags)),
    );
    wrong
}

/// What a workflow runs: the commands and the values of the environment of each step, without
/// the comments.
fn executed_by(workflow: &serde_yaml::Value) -> String {
    let mut text = String::new();
    let mut keep = |value: &str| {
        for line in value.lines().filter(|l| !l.trim_start().starts_with('#')) {
            text.push_str(line);
            text.push('\n');
        }
    };
    for step in steps(workflow) {
        step["run"].as_str().into_iter().for_each(&mut keep);
        for value in step["env"]
            .as_mapping()
            .into_iter()
            .flat_map(|e| e.values())
        {
            value.as_str().into_iter().for_each(&mut keep);
        }
    }
    text
}

/// The lines of one job of the GitLab drop-in, without the comments.
fn gitlab_job_text(gitlab: &str, job: &str) -> String {
    let head = format!("{job}:");
    gitlab
        .lines()
        .skip_while(|l| *l != head)
        .skip(1)
        .take_while(|l| l.is_empty() || l.starts_with(' '))
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether `text` reads the variable, as `$NAME` or `${NAME`, and not a longer name.
fn reads_variable(text: &str, name: &str) -> bool {
    [format!("${name}"), format!("${{{name}")]
        .iter()
        .any(|read| whole_word(text, read, &['_']))
}

#[test]
fn a_flag_or_a_variable_is_not_found_in_a_longer_word_that_starts_with_it() {
    assert!(whole_word("run --tier 3", "--tier", &['-']));
    assert!(whole_word("run --tier", "--tier", &['-']));
    assert!(!whole_word("run --tier-high 3", "--tier", &['-']));
    assert!(!whole_word("run --tiers 3", "--tier", &['-']));
    // the first place is a longer word, and the second place is the word
    assert!(whole_word("--tier-high --tier=3", "--tier", &['-']));

    assert!(reads_variable("echo $NAME-x", "NAME"));
    assert!(reads_variable("echo ${NAME:-}", "NAME"));
    assert!(!reads_variable("echo $NAME_2", "NAME"));
    assert!(!reads_variable("echo ${NAME_2:-}", "NAME"));
    assert!(!reads_variable("echo $NAMES", "NAME"));
}

/// Each reusable workflow takes each option of its script, or of its command, from an input, sets
/// it itself, or omits it with a reason. A new option of the script fails this test until a
/// person decides which one it is.
#[test]
fn each_reusable_workflow_names_each_option_of_its_script() {
    for w in &WRAPPERS {
        let name = w.workflow;
        let wrong = unaccounted(&w.tool.options(), w.reaches, w.left);
        assert!(wrong.is_empty(), "{name}: {wrong:#?}");

        let workflow = workflow(name);
        let inputs = workflow["on"]["workflow_call"]["inputs"]
            .as_mapping()
            .unwrap_or_else(|| panic!("{name} has no inputs"));
        let executed = executed_by(&workflow);
        for r in w.reaches {
            assert!(
                whole_word(&executed, r.flag, &['-']),
                "{name} never passes {} to the tool",
                r.flag
            );
            assert!(
                r.input.is_empty() || inputs.contains_key(r.input),
                "{name} has no input {} for {}",
                r.input,
                r.flag
            );
        }
    }
}

/// The GitLab job reads the variable that takes the place of each input, and passes the option.
#[test]
fn each_gitlab_job_sets_the_options_that_its_reusable_workflow_sets() {
    let gitlab = repo_file(GITLAB_DROP_IN);
    for w in &WRAPPERS {
        let job = gitlab_job_text(&gitlab, w.job);
        assert!(!job.is_empty(), "the drop-in has no job {}", w.job);
        for r in w.reaches.iter().filter(|r| !r.variable.is_empty()) {
            assert!(
                reads_variable(&job, r.variable),
                "{} never reads {} for {}",
                w.job,
                r.variable,
                r.flag
            );
            assert!(
                whole_word(&job, r.flag, &['-']),
                "{} never passes {}",
                w.job,
                r.flag
            );
            assert!(
                gitlab.contains(&format!("\n  {}:", r.variable)),
                "the drop-in gives {} no default",
                r.variable
            );
        }
    }
}

/// The promotion gate reads a ref such as `name@version` in a project, as the retire gate does.
#[test]
fn the_promote_gate_takes_the_project_of_a_ref_as_the_retire_gate_does() {
    let promote = workflow_text("promote-gate.yml");
    let retire = workflow_text("retire-gate.yml");
    assert!(promote.contains("PALETTE_PROJECT: ${{ inputs.project }}"));
    assert!(retire.contains("PALETTE_PROJECT: ${{ inputs.project }}"));
    assert!(promote.contains("PALETTE_PROJECT_UID: ${{ secrets.palette-project-uid }}"));
    let call = &workflow("promote-gate.yml")["on"]["workflow_call"];
    assert!(call["inputs"]["project"].is_mapping());
    assert!(call["secrets"]["palette-project-uid"].is_mapping());
}

/// A script passes to its command the options that a pipeline sets. Only the script does this:
/// the workflow and the job call the script.
struct Passes {
    script: &'static str,
    command: &'static str,
    passed: &'static [Reach],
    left: &'static [Left],
}

const PROJECT: Left = Left {
    flags: &["--project", "--project-name"],
    why: "the project comes from PALETTE_PROJECT_UID or PALETTE_PROJECT",
};

const FORMATS: Left = Left {
    flags: &["--json", "--color", "--no-color", "--compact"],
    why: "the script asks for the format that it reads",
};

/// The options of `strata preflight` that a pipeline sets. `preflight.sh` reads the variable of
/// each one and passes the option. The reusable workflow sets the variable from the input, and the
/// GitLab drop-in gives the variable a default. An empty input or variable means that the script
/// sets the option itself.
const PREFLIGHT: [Reach; 17] = [
    reach("--cloud", "cloud", "STRATA_PREFLIGHT_CLOUD"),
    reach("--release", "release", "STRATA_PREFLIGHT_RELEASE"),
    reach("--records", "records", "STRATA_PREFLIGHT_RECORDS"),
    reach("--probes", "probes", "STRATA_PREFLIGHT_PROBES"),
    reach("--tier", "tier", "STRATA_PREFLIGHT_TIER"),
    reach("--rule", "rule", "STRATA_PREFLIGHT_RULE"),
    reach("--strict", "strict", "STRATA_PREFLIGHT_STRICT"),
    reach("--ready", "ready", "STRATA_PREFLIGHT_READY"),
    reach("--no-exec", "no-exec", "STRATA_PREFLIGHT_NO_EXEC"),
    reach("--verbose", "verbose", "STRATA_PREFLIGHT_VERBOSE"),
    reach("--watch", "watch", "STRATA_PREFLIGHT_WATCH"),
    reach("--every", "every", "STRATA_PREFLIGHT_EVERY"),
    reach("--limit", "limit", "STRATA_PREFLIGHT_LIMIT"),
    reach("--out", "out", "STRATA_PREFLIGHT_OUT"),
    // set by the script: the markdown goes to standard output and to the directory of --out.
    // A run with no key reads nothing from Palette
    reach("--format", "", ""),
    reach("--quiet", "", ""),
    reach("--offline", "", ""),
];

/// The two arguments of `strata preflight`, as the input of the workflow and the variable that
/// `preflight.sh` reads.
const PREFLIGHT_ARGUMENTS: [(&str, &str); 2] = [
    ("source", "STRATA_PREFLIGHT_SOURCE"),
    ("change", "STRATA_PREFLIGHT_CHANGE"),
];

const PASSES: [Passes; 4] = [
    Passes {
        script: "verify-report.sh",
        command: "verify",
        passed: &[
            reach("--allow-no-digest", "", ""),
            reach("--who", "", ""),
            reach("--summary", "", ""),
            reach("--format", "", ""),
            reach("--quiet", "", ""),
        ],
        left: &[PROJECT, FORMATS],
    },
    Passes {
        script: "merge-gate.sh",
        command: "verify",
        passed: &[
            reach("--allow-no-digest", "", ""),
            reach("--summary", "", ""),
            reach("--json", "", ""),
            reach("--quiet", "", ""),
        ],
        left: &[
            PROJECT,
            Left {
                flags: &["--who"],
                why: "the gate stops the merge, and the drift issue names who",
            },
            Left {
                flags: &["--format", "--color", "--no-color", "--compact"],
                why: "the gate reads the status and the summary",
            },
        ],
    },
    Passes {
        script: "upstream-report.sh",
        command: "behind",
        passed: &[
            reach("--no-impact", "", ""),
            reach("--since", "", ""),
            reach("--format", "", ""),
            reach("--quiet", "", ""),
        ],
        left: &[
            FORMATS,
            Left {
                flags: &["--pre-release"],
                why: "an issue opens for a final version, and a pipeline would open one for each build",
            },
        ],
    },
    Passes {
        script: "preflight.sh",
        command: "preflight",
        passed: &PREFLIGHT,
        left: &[
            Left {
                flags: &["--rules", "--promotion", "--ledger", "--schema"],
                why: "a mode that checks no cluster",
            },
            Left {
                flags: &["--json", "--compact", "--color", "--no-color"],
                why: "the script fixes the markdown report",
            },
        ],
    },
];

/// The lines of a file of the repository that are not comments.
fn code_of(path: &str) -> String {
    repo_file(path)
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Each option of `strata verify`, `strata behind`, and `strata preflight` reaches the script that
/// wraps it, or the script omits it with a reason.
#[test]
fn each_script_passes_the_options_of_its_command_or_omits_them_on_purpose() {
    for p in &PASSES {
        let options = Listed::Command(p.command).options();
        let wrong = unaccounted(&options, p.passed, p.left);
        assert!(
            wrong.is_empty(),
            "{} for {}: {wrong:#?}",
            p.script,
            p.command
        );
        let code = code_of(&format!("scripts/ci/{}", p.script));
        for r in p.passed {
            assert!(
                whole_word(&code, r.flag, &['-']),
                "{} never passes {} to {}",
                p.script,
                r.flag,
                p.command
            );
        }
    }
}

/// The reusable workflow and the GitLab job build no option of `strata preflight`. They give each
/// input to `preflight.sh` as a variable. The script builds the list in one place.
#[test]
fn the_preflight_workflow_and_the_gitlab_job_give_each_input_to_the_script_as_a_variable() {
    let script = code_of("scripts/ci/preflight.sh");
    let gitlab = repo_file(GITLAB_DROP_IN);
    let job = gitlab_job_text(&gitlab, "strata-preflight");
    let workflow = workflow("preflight.yml");
    let inputs = workflow["on"]["workflow_call"]["inputs"]
        .as_mapping()
        .unwrap();
    let executed = executed_by(&workflow);
    let step = workflow["jobs"]["preflight"]["steps"]
        .as_sequence()
        .unwrap()
        .iter()
        .find(|s| {
            s["run"]
                .as_str()
                .is_some_and(|r| r.contains("scripts/ci/preflight.sh"))
        })
        .expect("the workflow runs preflight.sh");
    assert!(
        job.contains("preflight.sh"),
        "the job never runs the script"
    );

    let variables = PREFLIGHT
        .iter()
        .filter(|r| !r.variable.is_empty())
        .map(|r| (r.input, r.variable))
        .chain(PREFLIGHT_ARGUMENTS);
    for (input, variable) in variables {
        assert!(
            reads_variable(&script, variable),
            "preflight.sh never reads {variable}"
        );
        assert!(
            inputs.contains_key(input),
            "preflight.yml has no input {input} for {variable}"
        );
        assert_eq!(
            step["env"][variable].as_str(),
            Some(format!("${{{{ inputs.{input} }}}}").as_str()),
            "the step does not set {variable} from the input {input}"
        );
        assert!(
            gitlab.contains(&format!("\n  {variable}:")),
            "the drop-in gives {variable} no default"
        );
    }

    // the comment is for GitLab: a caller of preflight.yml calls preflight-comment.yml
    assert!(reads_variable(&script, "STRATA_PREFLIGHT_COMMENT"));
    assert!(whole_word(&script, "--preflight", &['-']));
    assert!(gitlab.contains("\n  STRATA_PREFLIGHT_COMMENT:"));

    // neither of the two names an option of the command, except the one that the job chooses
    for r in &PREFLIGHT {
        assert!(
            !whole_word(&executed, r.flag, &['-']),
            "preflight.yml passes {} itself",
            r.flag
        );
        assert!(
            r.flag == "--offline" || !whole_word(&job, r.flag, &['-']),
            "the GitLab job passes {} itself",
            r.flag
        );
    }
}

/// The key of the project is set for the other jobs, so the static tier reads nothing from
/// Palette unless the job watches a build. The job runs its script lines with the variables that
/// the drop-in sets, and `preflight.sh` is a stand-in that logs its arguments.
#[test]
fn the_gitlab_preflight_job_gives_the_script_offline_for_the_static_tier_without_a_watch() {
    let gitlab = gitlab_drop_in();
    let lines: Vec<&str> = gitlab["strata-preflight"]["script"]
        .as_sequence()
        .expect("the job has script lines")
        .iter()
        .map(|l| l.as_str().unwrap())
        .collect();
    let run = |tier: &str, watch: &str| -> String {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".strata")).unwrap();
        executable(
            &dir.path().join(".strata/preflight.sh"),
            "#!/usr/bin/env bash\necho \"preflight.sh $*\" >> calls.log\n",
        );
        let o = Command::new("bash")
            .args(["--noprofile", "--norc", "-eo", "pipefail", "-c"])
            .arg(lines.join("\n"))
            .current_dir(dir.path())
            .env("STRATA_PREFLIGHT_TIER", tier)
            .env("STRATA_PREFLIGHT_WATCH", watch)
            .output_locked()
            .unwrap();
        assert!(o.status.success(), "{}", text(&o.stderr));
        std::fs::read_to_string(dir.path().join("calls.log"))
            .unwrap()
            .trim()
            .to_string()
    };
    assert_eq!(run("static", "false"), "preflight.sh --offline");
    assert_eq!(run("static", "true"), "preflight.sh");
    assert_eq!(run("", "false"), "preflight.sh");
    assert_eq!(run("palette", "false"), "preflight.sh");
}

/// The names of the variables that a path of the GitLab file reads, as `$NAME` or `${NAME}`.
fn variables_in(path: &str) -> Vec<&str> {
    path.split('$')
        .skip(1)
        .map(|rest| rest.trim_start_matches('{'))
        .map(|rest| rest.split(|c: char| !c.is_ascii_alphanumeric() && c != '_'))
        .filter_map(|mut name| name.next())
        .filter(|name| !name.is_empty())
        .collect()
}

#[test]
fn a_variable_in_a_path_is_found_with_or_without_braces() {
    assert_eq!(variables_in("$STRATA_SAVE"), ["STRATA_SAVE"]);
    assert_eq!(variables_in("${OUT}/$NAME.json"), ["OUT", "NAME"]);
    assert!(variables_in("preflight/").is_empty());
}

/// A path of `artifacts` that a variable names is empty when the variable is empty. A GitLab
/// runner then gets an empty path on each run, and it is not sure that this is safe. So a job
/// with such a path has one of two things. In the first, the drop-in gives the variable a value
/// that is not empty. In the second, each rule of the job selects the job only when the variable
/// is set.
#[test]
fn no_gitlab_job_keeps_an_artifact_at_the_path_that_an_empty_variable_names() {
    let gitlab = gitlab_drop_in();
    let mut paths = 0;
    for (job, body) in gitlab.as_mapping().unwrap() {
        let job = job.as_str().unwrap();
        for path in body["artifacts"]["paths"]
            .as_sequence()
            .into_iter()
            .flatten()
            .map(|p| p.as_str().unwrap())
        {
            paths += 1;
            assert!(
                !path.is_empty(),
                "{job} keeps an artifact with an empty path"
            );
            for name in variables_in(path) {
                let default = gitlab["variables"][name].as_str().unwrap_or("");
                let selected_by_it = body["rules"].as_sequence().is_some_and(|rules| {
                    !rules.is_empty()
                        && rules.iter().all(|rule| {
                            rule["when"].as_str() != Some("never")
                                && reads_variable(rule["if"].as_str().unwrap_or(""), name)
                        })
                });
                assert!(
                    !default.is_empty() || selected_by_it,
                    "{job} keeps the path {path}, and {name} is empty by default"
                );
            }
        }
    }
    assert!(
        paths >= 2,
        "the drop-in keeps the artifact of two jobs at least"
    );
}

/// The upstream job has two forms. The job without an artifact runs when `STRATA_SAVE` is empty,
/// which is the default. The job with the artifact runs when `STRATA_SAVE` names a file, and it
/// is the same job otherwise.
#[test]
fn the_gitlab_job_of_the_upstream_report_keeps_an_artifact_only_when_it_saves_a_file() {
    let gitlab = gitlab_drop_in();
    assert_eq!(
        gitlab["variables"]["STRATA_SAVE"].as_str(),
        Some(""),
        "the default saves no file"
    );
    let plain = &gitlab["strata-upstream"];
    let saving = &gitlab["strata-upstream-save"];
    assert!(
        plain["artifacts"].is_null(),
        "the job that saves nothing keeps no artifact"
    );
    assert_eq!(
        saving["artifacts"]["paths"][0].as_str(),
        Some("$STRATA_SAVE")
    );
    assert_eq!(saving["extends"].as_str(), Some("strata-upstream"));
    assert!(
        saving["script"].is_null(),
        "the job that saves has the script of the other job, and no copy of it"
    );

    // the job that saves nothing does not run when the variable has a value, and runs otherwise
    let first = &plain["rules"][0];
    assert_eq!(first["if"].as_str(), Some("$STRATA_SAVE"));
    assert_eq!(first["when"].as_str(), Some("never"));
    assert!(
        plain["rules"][1]["if"]
            .as_str()
            .is_some_and(|rule| rule.contains(r#"$CI_PIPELINE_SOURCE == "schedule""#))
    );
    let rule = saving["rules"][0]["if"].as_str().unwrap();
    assert!(
        rule.contains(r#"$CI_PIPELINE_SOURCE == "schedule""#)
            && reads_variable(rule, "STRATA_SAVE"),
        "{rule}"
    );
    assert_eq!(saving["rules"].as_sequence().unwrap().len(), 1);
}

// ---- pr-comment.sh ---------------------------------------------------------------------------

/// A `gh` that records every invocation and saves any comment body it is handed. It answers the
/// two listings that the script reads from files that the test controls.
const FAKE_GH: &str = r#"#!/usr/bin/env bash
set -euo pipefail
args="$*"
echo "$args" >> "$FAKE_GH_LOG"
prev=""
for a in "$@"; do
  case "$a" in body=@*) cp "${a#body=@}" "$FAKE_GH_SAVED_BODY" ;; esac
  [ "$prev" = "--body-file" ] && cp "$a" "$FAKE_GH_SAVED_BODY"
  prev="$a"
done
case "$args" in
  *"-X PATCH"*) ;;
  "label create"*) ;;
  "label list"*) if [ -n "${FAKE_GH_REPO_LABELS:-}" ]; then cat "$FAKE_GH_REPO_LABELS"; else echo "[]"; fi ;;
  *"/comments"*) if [ -n "${FAKE_GH_COMMENTS:-}" ]; then cat "$FAKE_GH_COMMENTS"; else echo "[]"; fi ;;
  "pr view"*"--json labels"*) if [ -n "${FAKE_GH_LABELS:-}" ]; then cat "$FAKE_GH_LABELS"; else echo '{"labels":[]}'; fi ;;
  "pr view"*"--json number"*) echo '{"number":7}' ;;
  "repo view"*) echo '{"nameWithOwner":"o/r"}' ;;
esac
"#;

/// A `strata` that logs every call and answers `snapshot <ref> -o <file>` with a canned snapshot.
/// A live ref needs no Palette. Everything else goes to the real binary.
///
/// `compare --repave` is the one question that needs Palette. The real binary decides which pair
/// the question applies to, and it does so before any request. The stand-in asks it with a Palette
/// that nothing listens at, and passes on its refusal of the pair as strata prints it. For any
/// other pair, the stand-in runs the real comparison without the option, and adds the answer.
/// `FAKE_REPAVE_JSON` gives the `paletteRepave` of the report, and `FAKE_REPAVE_TEXT` gives the
/// `Palette:` line of the markdown. When `FAKE_REPAVE_FAIL` is set, it exits 2 as strata does when
/// Palette gives no answer.
const FAKE_STRATA: &str = r#"#!/usr/bin/env bash
set -euo pipefail
echo "$*" >> "$FAKE_STRATA_LOG"
if [ "$1" = snapshot ]; then
  cp "$FAKE_STRATA_SNAPSHOT" "$4"
  exit 0
fi
if [ "$1" = compare ] && [[ " $* " == *" --repave "* ]]; then
  if [ -n "${FAKE_REPAVE_FAIL:-}" ]; then echo "strata: Palette gave no answer" >&2; exit 2; fi
  refusal=$(PALETTE_API_KEY=k PALETTE_API=http://127.0.0.1:9 STRATA_RETRIES=0 \
    "$REAL_STRATA" compare "$2" "$3" --repave 2>&1 >/dev/null | grep '^strata: --repave needs' || true)
  if [ -n "$refusal" ]; then echo "$refusal" >&2; exit 2; fi
  args=()
  for a in "$@"; do [ "$a" = --repave ] || args+=("$a"); done
  code=0
  out=$("$REAL_STRATA" "${args[@]}") || code=$?
  if [[ " $* " == *" --format json "* ]]; then
    jq -c --argjson r "$FAKE_REPAVE_JSON" '. + {paletteRepave: $r}' <<<"$out"
  else
    printf '%s\n\n**Palette:** %s\n' "$out" "$FAKE_REPAVE_TEXT"
  fi
  exit "$code"
fi
exec "$REAL_STRATA" "$@"
"#;

/// A `curl` for the GitLab REST API. It logs each call as "METHOD /path field field". It keeps
/// each body file that it receives as body-<n>.md, in call order. It answers a GET from a
/// gl-*.json file under `FAKE_GL` when the test wrote one. It numbers a new issue from 41 up. It
/// prints the status code `FAKE_GL_STATUS` (200 by default) the way `-w '%{http_code}'` does.
const FAKE_GL_CURL: &str = r#"#!/usr/bin/env bash
set -euo pipefail
out=/dev/null; method=GET; url=""; data=()
while [ $# -gt 0 ]; do
  case "$1" in
    -o) out=$2; shift 2 ;;
    -X) method=$2; shift 2 ;;
    -H|-w) shift 2 ;;
    --data-urlencode) data+=("$2"); shift 2 ;;
    -*) shift ;;
    *) url=$1; shift ;;
  esac
done
path=${url#*/api/v4}
echo "$method $path ${data[*]-}" >> "$FAKE_GL/curl.log"
n=$(wc -l < "$FAKE_GL/curl.log")
for d in "${data[@]-}"; do
  f=${d#*@}
  case "$d" in *@*) [ -f "$f" ] && cp "$f" "$FAKE_GL/body-$n.md" ;; esac
done
answer='{}'
from() { [ -f "$FAKE_GL/$1" ] && answer=$(cat "$FAKE_GL/$1") || answer=$2; }
case "$method $path" in
  "GET "*"/notes"*) from gl-notes.json "[]" ;;
  "GET "*"/merge_requests/"*) from gl-mr.json '{"labels":[]}' ;;
  "GET "*"/labels"*) from gl-labels.json "[]" ;;
  "GET "*"/issues"*) from gl-issues.json "[]" ;;
  "POST /projects/"*"/issues") c=$(cat "$FAKE_GL/created" 2>/dev/null || echo 40); c=$((c + 1))
                                echo "$c" > "$FAKE_GL/created"; answer="{\"iid\":$c}" ;;
esac
printf '%s' "$answer" > "$out"
printf '%s' "${FAKE_GL_STATUS:-200}"
"#;

/// The variables of a GitLab job, as the scripts read them.
///
/// The stand-in curl answers from the directory `gl`; the project, the API, and the token are
/// fixed.
fn gitlab_job(gl: &str) -> Vec<(&str, &str)> {
    vec![
        ("FAKE_GL", gl),
        ("GITLAB_CI", "true"),
        ("CI_PROJECT_ID", "42"),
        ("CI_API_V4_URL", "https://gitlab.example/api/v4"),
        ("GITLAB_TOKEN", "a-token"),
    ]
}

struct Host {
    _dir: tempfile::TempDir,
    bin: PathBuf,
    log: PathBuf,
    body: PathBuf,
    comments: PathBuf,
    labels: PathBuf,
    repo_labels: PathBuf,
    gl: PathBuf,
    home: PathBuf,
    a: String,
    a2: String,
    b: String,
    not_a_record: String,
}

/// Snapshot files for fixtures a, a2 and b, plus the fake `gh` on its own bin directory.
fn host() -> Host {
    let dir = tempfile::tempdir().unwrap();
    let raw = dir.path().join("raw");
    common::make_fixtures(&raw);
    let snap = |name: &str| -> String {
        let out = dir.path().join(format!("{name}.json"));
        snapshot_from_raw(&raw, name, &out);
        out.to_str().unwrap().to_string()
    };
    let bin = fake_bin(
        dir.path(),
        &[
            ("gh", FAKE_GH),
            ("fake-strata", FAKE_STRATA),
            ("curl", FAKE_GL_CURL),
        ],
    );
    Host {
        a: snap("a"),
        a2: snap("a2"),
        b: snap("b"),
        not_a_record: raw.join("a/profile.json").display().to_string(),
        bin,
        log: dir.path().join("gh.log"),
        body: dir.path().join("saved-body.md"),
        comments: dir.path().join("comments.json"),
        labels: dir.path().join("labels.json"),
        repo_labels: dir.path().join("repo-labels.json"),
        gl: {
            let g = dir.path().join("gitlab");
            std::fs::create_dir(&g).unwrap();
            g
        },
        home: {
            let h = dir.path().join("home");
            std::fs::create_dir(&h).unwrap();
            h
        },
        _dir: dir,
    }
}

impl Host {
    fn run(&self, args: &[&str]) -> Output {
        self.run_with(args, &[])
    }

    fn run_with(&self, args: &[&str], envs: &[(&str, &str)]) -> Output {
        // an empty log means no call, and `lines_of` stops the test when the log is gone
        std::fs::write(&self.log, "").unwrap();
        let _ = std::fs::remove_file(&self.body);
        let mut c = Command::new("bash");
        c.arg(script("ci/pr-comment.sh"))
            .args(args)
            .env("PATH", path_with(&self.bin))
            .env("STRATA_QUIET", "1")
            .env("FAKE_GH_LOG", &self.log)
            .env("FAKE_GH_SAVED_BODY", &self.body)
            .env("HOME", &self.home)
            .env_remove("GITHUB_REPOSITORY")
            .env_remove("PALETTE_API_KEY")
            .env_remove("SPECTROCLOUD_APIKEY");
        // the script runs the binary of the tests, or a stand-in that runs it as REAL_STRATA
        give_strata(&mut c, "STRATA_BIN");
        give_strata(&mut c, "REAL_STRATA");
        if self.repo_labels.exists() {
            c.env("FAKE_GH_REPO_LABELS", &self.repo_labels);
        }
        for (k, v) in envs {
            c.env(k, v);
        }
        if self.comments.exists() {
            c.env("FAKE_GH_COMMENTS", &self.comments);
        }
        if self.labels.exists() {
            c.env("FAKE_GH_LABELS", &self.labels);
        }
        c.output_locked().unwrap()
    }

    fn calls(&self) -> Vec<String> {
        lines_of(&self.log)
    }

    fn saved_body(&self) -> String {
        std::fs::read_to_string(&self.body).expect("gh was handed a comment body")
    }

    /// Run the script the way a GitLab job does: the CI variables, and no --host.
    fn run_gitlab(&self, args: &[&str], envs: &[(&str, &str)]) -> Output {
        for f in ["body-1.md", "body-2.md", "body-3.md"] {
            let _ = std::fs::remove_file(self.gl.join(f));
        }
        std::fs::write(self.gl.join("curl.log"), "").unwrap();
        let gl = self.gl.display().to_string();
        let mut all = gitlab_job(&gl);
        all.push(("CI_MERGE_REQUEST_IID", "7"));
        all.extend_from_slice(envs);
        self.run_with(args, &all)
    }

    fn put_gl(&self, name: &str, body: &str) {
        std::fs::write(self.gl.join(name), body).unwrap();
    }

    fn gl_calls(&self) -> Vec<String> {
        lines_of(&self.gl.join("curl.log"))
    }

    /// The body file handed to curl on the call with this 1-based number.
    fn gl_handed(&self, call: usize) -> String {
        handed_in(&self.gl, "body", call)
    }
}

#[test]
fn pr_comment_posts_one_marked_comment_and_adds_the_label_at_the_threshold() {
    let h = host();
    let o = h.run(&[&h.a, &h.b, "--pr", "7", "--repo", "o/r"]);
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    let calls = h.calls();
    assert!(
        calls
            .iter()
            .any(|c| c.starts_with("pr comment 7 --repo o/r --body-file ")),
        "{calls:?}"
    );
    assert!(!calls.iter().any(|c| c.contains("PATCH")), "{calls:?}");
    let body = h.saved_body();
    assert!(
        body.starts_with("<!-- strata -->\n"),
        "marker first so the comment can be found again:\n{body}"
    );
    assert!(
        body.contains("### lab-profile@1.2.0 → lab-profile@1.3.0"),
        "{body}"
    );
    assert!(body.contains("| `~` | k8s | `kubernetes` |"), "{body}");
    let create = calls
        .iter()
        .position(|c| c.starts_with("label create repave --repo o/r"))
        .expect("a label the repository lacks is created, since gh cannot add an unknown label");
    let add = calls
        .iter()
        .position(|c| c == "pr edit 7 --repo o/r --add-label repave")
        .expect("repave reaches the default threshold");
    assert!(create < add, "{calls:?}");
    assert!(
        text(&o.stderr).contains("max impact: repave"),
        "{}",
        text(&o.stderr)
    );

    // a repository that already has the label gets no create
    std::fs::write(&h.repo_labels, r#"[{"name": "bug"}, {"name": "repave"}]"#).unwrap();
    let o = h.run(&[&h.a, &h.b, "--pr", "7", "--repo", "o/r"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(
        !h.calls().iter().any(|c| c.starts_with("label create")),
        "{:?}",
        h.calls()
    );
    assert!(
        h.calls()
            .contains(&"pr edit 7 --repo o/r --add-label repave".to_string())
    );
}

#[test]
fn pr_comment_updates_the_existing_marked_comment_instead_of_adding_one() {
    let h = host();
    std::fs::write(
        &h.comments,
        r#"[{"id": 9, "body": "unrelated"}, {"id": 123, "body": "<!-- strata -->\nold report"}]"#,
    )
    .unwrap();
    let o = h.run(&[&h.a, &h.b, "--pr", "7", "--repo", "o/r"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let calls = h.calls();
    assert!(
        calls
            .iter()
            .any(|c| c.starts_with("api -X PATCH repos/o/r/issues/comments/123 -F body=@")),
        "{calls:?}"
    );
    assert!(
        !calls.iter().any(|c| c.starts_with("pr comment")),
        "{calls:?}"
    );
    assert!(h.saved_body().starts_with("<!-- strata -->\n"));
}

#[test]
fn pr_comment_removes_the_label_below_the_threshold_and_touches_nothing_when_already_right() {
    let h = host();
    std::fs::write(
        &h.labels,
        r#"{"labels": [{"name": "repave"}, {"name": "other"}]}"#,
    )
    .unwrap();
    let o = h.run(&[&h.a, &h.a2, "--pr", "7", "--repo", "o/r"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let calls = h.calls();
    assert!(
        calls.contains(&"pr edit 7 --repo o/r --remove-label repave".to_string()),
        "identical profiles are below the threshold: {calls:?}"
    );
    assert!(h.saved_body().contains("IDENTICAL"));

    std::fs::remove_file(&h.labels).unwrap();
    let o = h.run(&[&h.a, &h.a2, "--pr", "7", "--repo", "o/r"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(
        !h.calls().iter().any(|c| c.starts_with("pr edit")),
        "no label change when the label is already absent: {:?}",
        h.calls()
    );

    // a lower threshold and another label name
    let o = h.run(&[
        &h.a,
        &h.b,
        "--pr",
        "7",
        "--repo",
        "o/r",
        "--label",
        "needs-window",
        "--threshold",
        "in-place",
    ]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(
        h.calls()
            .contains(&"pr edit 7 --repo o/r --add-label needs-window".to_string())
    );
}

#[test]
fn pr_comment_dry_run_prints_the_comment_and_calls_no_host() {
    let h = host();
    let o = h.run(&[&h.a, &h.b, "--dry-run"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(
        text(&o.stdout).starts_with("<!-- strata -->\n"),
        "{}",
        text(&o.stdout)
    );
    assert!(
        text(&o.stderr).contains("would add label repave"),
        "{}",
        text(&o.stderr)
    );
    assert!(h.calls().is_empty(), "{:?}", h.calls());
}

#[test]
fn pr_comment_fails_before_posting_when_strata_cannot_compare() {
    let h = host();
    let o = h.run(&[&h.not_a_record, &h.b, "--pr", "7", "--repo", "o/r"]);
    assert_eq!(o.status.code(), Some(2), "{}", text(&o.stderr));
    assert!(
        text(&o.stderr).contains("not a strata snapshot"),
        "{}",
        text(&o.stderr)
    );
    assert!(
        h.calls().is_empty(),
        "nothing is posted on a failed comparison: {:?}",
        h.calls()
    );

    // a live ref is snapshotted once, before any comparison; without credentials that step fails
    let o = h.run(&["lab-profile@1.3.0", &h.b, "--pr", "7", "--repo", "o/r"]);
    assert_eq!(o.status.code(), Some(2), "{}", text(&o.stderr));
    assert!(
        text(&o.stderr).contains("PALETTE_API_KEY is required"),
        "{}",
        text(&o.stderr)
    );
    assert!(h.calls().is_empty(), "{:?}", h.calls());
}

#[test]
fn pr_comment_snapshots_a_live_ref_once_and_compares_files_offline() {
    let h = host();
    let log = h.bin.parent().unwrap().join("strata.log");
    let o = h.run_with(
        &["lab-profile@1.3.0", &h.a, "--pr", "7", "--repo", "o/r"],
        &[
            ("STRATA_BIN", h.bin.join("fake-strata").to_str().unwrap()),
            ("FAKE_STRATA_SNAPSHOT", &h.b),
            ("FAKE_STRATA_LOG", log.to_str().unwrap()),
        ],
    );
    assert!(o.status.success(), "{}", text(&o.stderr));
    let calls = lines_of(&log);
    let snapshots: Vec<&String> = calls
        .iter()
        .filter(|c| c.starts_with("snapshot "))
        .collect();
    assert_eq!(snapshots.len(), 1, "{calls:?}");
    assert!(
        snapshots[0].starts_with("snapshot lab-profile@1.3.0 -o "),
        "{calls:?}"
    );
    let compares: Vec<&String> = calls.iter().filter(|c| c.starts_with("compare ")).collect();
    assert_eq!(compares.len(), 2, "one per format: {calls:?}");
    assert!(
        compares.iter().all(|c| !c.contains("lab-profile@1.3.0")),
        "the comparisons read the snapshot file, not Palette: {calls:?}"
    );
    assert!(
        h.saved_body()
            .contains("### lab-profile@1.3.0 → lab-profile@1.2.0")
    );
}

const PALETTE_REPAVES: &str = r#"{"required": true, "agrees": true}"#;
const PALETTE_REPAVES_TEXT: &str =
    "repave: PackValuesUpdated kubernetes (kubeadmconfig.postKubeadmCommands)";
const PALETTE_KEEPS: &str = r#"{"required": false, "agrees": false}"#;
const PALETTE_KEEPS_TEXT: &str = "no repave";

impl Host {
    /// Run the script with the stand-in `strata` of [`FAKE_STRATA`], which answers `compare
    /// --repave` with `json` (the `paletteRepave` of the report) and `text` (the `Palette:` line).
    /// `more` is any other variable of the run.
    fn run_repave(&self, args: &[&str], json: &str, text: &str, more: &[(&str, &str)]) -> Output {
        let fake = self.bin.join("fake-strata");
        let log = self.strata_log();
        std::fs::write(&log, "").unwrap();
        let mut envs = vec![
            ("STRATA_BIN", fake.to_str().unwrap()),
            ("FAKE_STRATA_LOG", log.to_str().unwrap()),
            ("FAKE_REPAVE_JSON", json),
            ("FAKE_REPAVE_TEXT", text),
        ];
        envs.extend_from_slice(more);
        self.run_with(args, &envs)
    }

    fn strata_log(&self) -> PathBuf {
        self.bin.parent().unwrap().join("strata.log")
    }

    /// The calls of the stand-in `strata` in the last run of [`Host::run_repave`].
    fn strata_calls(&self) -> Vec<String> {
        lines_of(&self.strata_log())
    }

    /// A copy of the snapshot `file` that records a cluster, as `strata snapshot cluster:payments`
    /// does, and else the same.
    fn cluster_of(&self, file: &str) -> String {
        self.edited("cluster.json", file, |source| {
            source["cluster"] =
                serde_json::json!({"uid": "c-1", "name": "payments", "profiles": []});
        })
    }

    /// A copy of the snapshot `file` that Palette does not hold, as a candidate: no uid.
    fn candidate_of(&self, file: &str) -> String {
        self.edited("candidate.json", file, |source| {
            source["uid"] = serde_json::json!("");
            source["template"] = serde_json::json!("candidate");
        })
    }

    fn edited(
        &self,
        name: &str,
        file: &str,
        change: impl FnOnce(&mut serde_json::Value),
    ) -> String {
        let mut doc: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
        change(&mut doc["source"]);
        let out = self._dir.path().join(name);
        std::fs::write(&out, doc.to_string()).unwrap();
        out.display().to_string()
    }
}

#[test]
fn pr_comment_repave_sends_the_question_for_a_cluster_and_a_version_and_the_label_follows_the_answer()
 {
    let h = host();
    let cluster = h.cluster_of(&h.a);

    // the table says repave for this pair, and Palette says no: the label is not added
    let o = h.run_repave(
        &[&cluster, &h.b, "--repave", "--dry-run"],
        PALETTE_KEEPS,
        PALETTE_KEEPS_TEXT,
        &[],
    );
    assert!(o.status.success(), "{}", text(&o.stderr));
    let calls = h.strata_calls();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert!(
        calls.iter().all(|c| c.contains(" --repave")),
        "both renderings carry the question: {calls:?}"
    );
    assert!(
        text(&o.stdout).contains("**Palette:** no repave"),
        "{}",
        text(&o.stdout)
    );
    let said = text(&o.stderr);
    assert!(
        said.contains("max impact: repave;"),
        "the table is read: {said}"
    );
    assert!(
        said.contains("label repave: Palette says no repave; would remove label repave if present"),
        "{said}"
    );

    // the table says nothing for this pair, and Palette says repave: the label is added
    let o = h.run_repave(
        &[&cluster, &h.a2, "--repave", "--dry-run"],
        PALETTE_REPAVES,
        PALETTE_REPAVES_TEXT,
        &[],
    );
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(
        text(&o.stdout).contains(&format!("**Palette:** {PALETTE_REPAVES_TEXT}")),
        "{}",
        text(&o.stdout)
    );
    let said = text(&o.stderr);
    assert!(said.contains("max impact: none;"), "{said}");
    assert!(
        said.contains("label repave: Palette says repave; would add label repave"),
        "{said}"
    );
}

#[test]
fn pr_comment_repave_changes_the_label_on_the_host_by_the_answer_of_palette() {
    let h = host();
    let cluster = h.cluster_of(&h.a);
    std::fs::write(&h.labels, r#"{"labels": [{"name": "repave"}]}"#).unwrap();
    let args = [&cluster[..], &h.b, "--pr", "7", "--repo", "o/r", "--repave"];

    let o = h.run_repave(&args, PALETTE_KEEPS, PALETTE_KEEPS_TEXT, &[]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(
        h.calls()
            .contains(&"pr edit 7 --repo o/r --remove-label repave".to_string()),
        "the label goes where the table says repave: {:?}",
        h.calls()
    );
    assert!(h.saved_body().contains("**Palette:** no repave"));
    assert!(
        text(&o.stderr)
            .contains("label repave: Palette says no repave; comment posted; label repave removed"),
        "{}",
        text(&o.stderr)
    );

    // Palette says repave and the label is there: nothing to change
    let o = h.run_repave(&args, PALETTE_REPAVES, PALETTE_REPAVES_TEXT, &[]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(
        !h.calls().iter().any(|c| c.starts_with("pr edit")),
        "{:?}",
        h.calls()
    );
    assert!(
        text(&o.stderr)
            .contains("label repave: Palette says repave; comment posted; label repave unchanged"),
        "{}",
        text(&o.stderr)
    );
}

#[test]
fn pr_comment_repave_reads_the_table_below_may_repave_when_palette_says_no_repave() {
    let h = host();
    let cluster = h.cluster_of(&h.a);
    // "no repave" does not answer a label for the level cluster-wide
    let o = h.run_repave(
        &[
            &cluster,
            &h.b,
            "--repave",
            "--dry-run",
            "--threshold",
            "cluster-wide",
        ],
        PALETTE_KEEPS,
        PALETTE_KEEPS_TEXT,
        &[],
    );
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(
        text(&o.stderr).contains(
            "label repave: Palette says no repave, and the table decides below may-repave; would add label repave"
        ),
        "{}",
        text(&o.stderr)
    );
    // at may-repave, the answer of Palette settles the label
    let o = h.run_repave(
        &[
            &cluster,
            &h.b,
            "--repave",
            "--dry-run",
            "--threshold",
            "may-repave",
        ],
        PALETTE_KEEPS,
        PALETTE_KEEPS_TEXT,
        &[],
    );
    assert!(
        text(&o.stderr)
            .contains("label repave: Palette says no repave; would remove label repave if present"),
        "{}",
        text(&o.stderr)
    );
}

#[test]
fn pr_comment_repave_leaves_to_strata_which_pair_it_applies_to_and_the_table_decides_for_the_rest()
{
    let h = host();
    let cluster = h.cluster_of(&h.a);
    let candidate = h.candidate_of(&h.b);
    let billing = h.edited("billing.json", &h.b, |source| {
        source["cluster"] = serde_json::json!({"uid": "c-2", "name": "billing", "profiles": []});
    });
    let pairs = [
        // two profile versions: no cluster
        (
            [h.a.clone(), h.b.clone()],
            "strata: --repave needs one side that is a cluster and one that is a profile version\n",
        ),
        // a candidate is not in Palette
        (
            [cluster.clone(), candidate],
            "strata: --repave needs the profile side to be a profile version that Palette holds, with its uid",
        ),
        // two clusters: a pair that the script once passed on to strata
        (
            [cluster, billing],
            "strata: --repave needs one side that is a cluster and one that is a profile version, or",
        ),
    ];
    for ([a, b], refusal) in &pairs {
        let o = h.run_repave(
            &[a, b, "--repave", "--dry-run"],
            PALETTE_KEEPS,
            PALETTE_KEEPS_TEXT,
            &[],
        );
        assert!(o.status.success(), "{}", text(&o.stderr));
        let said = text(&o.stderr);
        assert!(said.contains(refusal), "strata says why:\n{said}");
        assert!(
            said.contains("pr-comment: --repave got no answer, so the table decides"),
            "{said}"
        );
        assert!(
            said.contains("label repave: the table decides; would add label repave"),
            "{said}"
        );
        assert!(!text(&o.stdout).contains("Palette:"), "{}", text(&o.stdout));
        assert!(
            h.strata_calls()
                .iter()
                .any(|c| c.starts_with("compare ") && c.contains(" --repave")),
            "the script asks strata, which decides: {:?}",
            h.strata_calls()
        );
    }
    // the script reads no record to decide
    assert!(
        !repo_file("scripts/ci/pr-comment.sh").contains(".source"),
        "pr-comment.sh looks in a record: strata decides which pair --repave applies to"
    );
}

#[test]
fn pr_comment_repave_goes_on_with_the_table_when_palette_gives_no_answer() {
    let h = host();
    let cluster = h.cluster_of(&h.a);
    let o = h.run_repave(
        &[&cluster, &h.b, "--repave", "--pr", "7", "--repo", "o/r"],
        PALETTE_KEEPS,
        PALETTE_KEEPS_TEXT,
        &[("FAKE_REPAVE_FAIL", "1")],
    );
    assert!(o.status.success(), "{}", text(&o.stderr));
    let said = text(&o.stderr);
    assert!(
        said.contains("pr-comment: --repave got no answer, so the table decides"),
        "{said}"
    );
    assert!(
        said.contains("max impact: repave; label repave: the table decides;"),
        "{said}"
    );
    assert!(
        h.calls()
            .contains(&"pr edit 7 --repo o/r --add-label repave".to_string()),
        "the table adds the label: {:?}",
        h.calls()
    );
    assert!(!h.saved_body().contains("Palette:"));
}

#[test]
fn pr_comment_without_repave_asks_palette_nothing_and_adds_no_clause_to_the_note() {
    let h = host();
    let cluster = h.cluster_of(&h.a);
    let o = h.run_repave(
        &[&cluster, &h.b, "--dry-run"],
        PALETTE_KEEPS,
        PALETTE_KEEPS_TEXT,
        &[],
    );
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(
        h.strata_calls().iter().all(|c| !c.contains("--repave")),
        "{:?}",
        h.strata_calls()
    );
    assert!(
        text(&o.stderr)
            .trim_end()
            .ends_with("max impact: repave; would add label repave"),
        "{}",
        text(&o.stderr)
    );
}

#[test]
fn pr_comment_posts_one_marked_note_and_labels_the_merge_request_on_gitlab() {
    let h = host();
    // no --host and no --pr: the CI variables of a GitLab job say both
    let o = h.run_gitlab(&[&h.a, &h.b], &[]);
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    let calls = h.gl_calls();
    assert!(
        calls[0].starts_with("GET /projects/42/merge_requests/7/notes"),
        "{calls:?}"
    );
    assert!(
        calls[1].starts_with("POST /projects/42/merge_requests/7/notes body@"),
        "{calls:?}"
    );
    let body = h.gl_handed(2);
    assert!(
        body.starts_with("<!-- strata -->\n"),
        "marker first so the note can be found again:\n{body}"
    );
    assert!(
        body.contains("### lab-profile@1.2.0 → lab-profile@1.3.0"),
        "{body}"
    );
    let create = calls
        .iter()
        .position(|c| c.starts_with("POST /projects/42/labels name=repave"))
        .expect("a label the project lacks is created first");
    let add = calls
        .iter()
        .position(|c| c == "PUT /projects/42/merge_requests/7 add_labels=repave")
        .expect("repave reaches the default threshold");
    assert!(create < add, "{calls:?}");
    assert!(
        text(&o.stderr).contains("max impact: repave"),
        "{}",
        text(&o.stderr)
    );
}

#[test]
fn pr_comment_updates_the_marked_note_on_gitlab_and_leaves_a_label_it_has() {
    let h = host();
    h.put_gl(
        "gl-notes.json",
        r#"[{"id": 11, "body": "a review note"}, {"id": 55, "body": "<!-- strata -->\nan older report"}]"#,
    );
    h.put_gl("gl-mr.json", r#"{"labels": ["repave", "review"]}"#);
    let o = h.run_gitlab(&[&h.a, &h.b], &[]);
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    let calls = h.gl_calls();
    assert!(
        calls
            .iter()
            .any(|c| c.starts_with("PUT /projects/42/merge_requests/7/notes/55 body@")),
        "the marked note is updated, not doubled: {calls:?}"
    );
    assert!(
        !calls
            .iter()
            .any(|c| c.starts_with("POST /projects/42/merge_requests/7/notes")),
        "{calls:?}"
    );
    assert!(
        !calls.iter().any(|c| c.contains("add_labels")),
        "the merge request already has the label: {calls:?}"
    );
    assert!(
        text(&o.stderr).contains("comment 55 updated"),
        "{}",
        text(&o.stderr)
    );
}

#[test]
fn pr_comment_stops_on_what_gitlab_answers_and_asks_for_a_token_that_can_write() {
    let h = host();
    let o = h.run_gitlab(&[&h.a, &h.b], &[("FAKE_GL_STATUS", "403")]);
    assert!(!o.status.success());
    assert!(
        text(&o.stderr).contains("GitLab answered HTTP 403"),
        "{}",
        text(&o.stderr)
    );

    let o = h.run_gitlab(&[&h.a, &h.b], &[("GITLAB_TOKEN", "")]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("set GITLAB_TOKEN"),
        "{}",
        text(&o.stderr)
    );
    assert!(h.gl_calls().is_empty(), "no call before the token is there");
}

/// A markdown report of `strata preflight`.
const PREFLIGHT_REPORT: &str =
    "## Preflight: payments\n\n| check | verdict |\n|---|---|\n| `eks-subnets` | FAIL |\n";

/// The report, in a file of the stand-in home.
fn preflight_report(h: &Host) -> String {
    let report = h.home.join("report.md");
    std::fs::write(&report, PREFLIGHT_REPORT).unwrap();
    report.display().to_string()
}

#[test]
fn pr_comment_preflight_posts_the_report_under_its_own_marker_and_touches_no_label() {
    let h = host();
    let report = preflight_report(&h);
    // no strata is needed: the report is a file
    let o = h.run_with(
        &["--preflight", &report, "--pr", "7", "--repo", "o/r"],
        &[("STRATA_BIN", "/nonexistent/strata")],
    );
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    let calls = h.calls();
    assert!(
        calls
            .iter()
            .any(|c| c.starts_with("pr comment 7 --repo o/r --body-file ")),
        "{calls:?}"
    );
    assert!(
        !calls
            .iter()
            .any(|c| c.contains("label") || c.starts_with("pr edit")),
        "a preflight report sets no label: {calls:?}"
    );
    let body = h.saved_body();
    assert!(
        body.starts_with("<!-- strata-preflight -->\n## Preflight: payments"),
        "{body}"
    );
    assert!(body.contains("| `eks-subnets` | FAIL |"), "{body}");
    assert!(
        text(&o.stderr).contains("comment posted"),
        "{}",
        text(&o.stderr)
    );
}

#[test]
fn pr_comment_preflight_updates_its_own_comment_and_leaves_the_comparison_comment() {
    let h = host();
    let report = preflight_report(&h);
    std::fs::write(
        &h.comments,
        r#"[{"id": 9, "body": "<!-- strata -->\nthe comparison"}, {"id": 123, "body": "<!-- strata-preflight -->\nthe last push"}]"#,
    )
    .unwrap();
    let o = h.run(&["--preflight", &report, "--pr", "7", "--repo", "o/r"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let calls = h.calls();
    assert!(
        calls
            .iter()
            .any(|c| c.starts_with("api -X PATCH repos/o/r/issues/comments/123 -F body=@")),
        "{calls:?}"
    );
    assert!(
        !calls
            .iter()
            .any(|c| c.contains("comments/9") || c.starts_with("pr comment")),
        "{calls:?}"
    );

    // a pull request with a comparison comment and no preflight comment gets a second comment.
    // The comparison run does not change the preflight comment
    std::fs::write(
        &h.comments,
        r#"[{"id": 9, "body": "<!-- strata -->\nthe comparison"}]"#,
    )
    .unwrap();
    let o = h.run(&["--preflight", &report, "--pr", "7", "--repo", "o/r"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(
        h.calls().iter().any(|c| c.starts_with("pr comment 7")),
        "{:?}",
        h.calls()
    );
    std::fs::write(
        &h.comments,
        r#"[{"id": 5, "body": "<!-- strata-preflight -->\nthe report"}]"#,
    )
    .unwrap();
    let o = h.run(&[&h.a, &h.b, "--pr", "7", "--repo", "o/r"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(
        h.calls().iter().any(|c| c.starts_with("pr comment 7")),
        "the comparison does not take the preflight comment: {:?}",
        h.calls()
    );
}

#[test]
fn pr_comment_preflight_dry_run_prints_the_comment_and_calls_no_host() {
    let h = host();
    let report = preflight_report(&h);
    let o = h.run(&["--preflight", &report, "--dry-run"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(
        text(&o.stdout).starts_with("<!-- strata-preflight -->\n## Preflight: payments"),
        "{}",
        text(&o.stdout)
    );
    assert!(h.calls().is_empty(), "{:?}", h.calls());
}

#[test]
fn pr_comment_preflight_stops_on_a_missing_report_and_on_a_comparison_option() {
    let h = host();
    let report = preflight_report(&h);
    let missing = h.home.join("none.md").display().to_string();
    let o = h.run(&["--preflight", &missing, "--pr", "7", "--repo", "o/r"]);
    assert_eq!(o.status.code(), Some(2), "{}", text(&o.stderr));
    assert!(
        text(&o.stderr).contains("missing or empty"),
        "{}",
        text(&o.stderr)
    );
    let empty = h.home.join("empty.md");
    std::fs::write(&empty, "").unwrap();
    let o = h.run(&[
        "--preflight",
        empty.to_str().unwrap(),
        "--pr",
        "7",
        "--repo",
        "o/r",
    ]);
    assert_eq!(o.status.code(), Some(2), "{}", text(&o.stderr));

    for extra in [
        vec![h.a.as_str()],
        vec!["--label", "x"],
        vec!["--threshold", "sync"],
        vec!["--repave"],
    ] {
        let mut args = vec!["--preflight", report.as_str(), "--pr", "7", "--repo", "o/r"];
        args.extend(extra.iter().copied());
        let o = h.run(&args);
        assert_eq!(o.status.code(), Some(2), "{extra:?}: {}", text(&o.stderr));
        assert!(
            text(&o.stderr).contains("--preflight takes no ref"),
            "{extra:?}: {}",
            text(&o.stderr)
        );
    }
    assert!(
        h.calls().is_empty(),
        "no host call on a wrong use: {:?}",
        h.calls()
    );
}

#[test]
fn pr_comment_preflight_posts_and_updates_one_marked_note_on_gitlab() {
    let h = host();
    let report = preflight_report(&h);
    let o = h.run_gitlab(&["--preflight", &report], &[]);
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    let calls = h.gl_calls();
    assert!(
        calls[0].starts_with("GET /projects/42/merge_requests/7/notes"),
        "{calls:?}"
    );
    assert!(
        calls[1].starts_with("POST /projects/42/merge_requests/7/notes body@"),
        "{calls:?}"
    );
    assert_eq!(calls.len(), 2, "a note, and no label: {calls:?}");
    assert!(
        h.gl_handed(2)
            .starts_with("<!-- strata-preflight -->\n## Preflight: payments"),
        "{}",
        h.gl_handed(2)
    );

    h.put_gl(
        "gl-notes.json",
        r#"[{"id": 11, "body": "<!-- strata -->\nthe comparison"}, {"id": 55, "body": "<!-- strata-preflight -->\nthe last push"}]"#,
    );
    let o = h.run_gitlab(&["--preflight", &report], &[]);
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    assert!(
        h.gl_calls()
            .iter()
            .any(|c| c.starts_with("PUT /projects/42/merge_requests/7/notes/55 body@")),
        "{:?}",
        h.gl_calls()
    );
    assert!(
        text(&o.stderr).contains("comment 55 updated"),
        "{}",
        text(&o.stderr)
    );
}

// ---- install-strata.sh -------------------------------------------------------------------------

/// A `curl` for the release API of a code host: the release document, and each asset by its id.
const FAKE_RELEASE_CURL: &str = r#"#!/usr/bin/env bash
set -euo pipefail
out=""; url=""
while [ $# -gt 0 ]; do
  case "$1" in
    -o) out=$2; shift 2 ;;
    -H) shift 2 ;;
    -*) shift ;;
    *) url=$1; shift ;;
  esac
done
echo "$url" >> "$FAKE_REL/curl.log"
case "$url" in
  */releases/tags/*|*/releases/latest) cp "$FAKE_REL/release.json" "$out" ;;
  */releases/assets/*) cp "$FAKE_REL/assets/${url##*/}" "$out" ;;
  *) echo "no such URL: $url" >&2; exit 22 ;;
esac
"#;

/// The SHA-256 digest of a file, as `sha256sum` prints it.
fn sha256_of(path: &Path) -> String {
    let sum = Command::new("sha256sum").arg(path).output_locked().unwrap();
    text(&sum.stdout)
        .split_whitespace()
        .next()
        .unwrap()
        .to_string()
}

/// The tools that `install-strata.sh` and the stand-in `curl` call, found on the `PATH` of the test.
const INSTALLER_TOOLS: [&str; 13] = [
    "bash",
    "chmod",
    "cp",
    "head",
    "jq",
    "mkdir",
    "mktemp",
    "mv",
    "rm",
    "sed",
    "sha256sum",
    "tail",
    "uname",
];

/// A directory with a link to each tool of the installer, and to no `cosign`. A machine of a
/// developer can have a real `cosign` on its `PATH`, which the tests that name its absence must
/// not find.
fn tools_without_cosign(d: &Path) -> PathBuf {
    let tools = d.join("tools");
    std::fs::create_dir_all(&tools).unwrap();
    let path = std::env::var("PATH").unwrap();
    for name in INSTALLER_TOOLS {
        let found = path
            .split(':')
            .map(|dir| Path::new(dir).join(name))
            .find(|p| p.is_file())
            .unwrap_or_else(|| panic!("{name} is not on the PATH"));
        std::os::unix::fs::symlink(found, tools.join(name)).unwrap();
    }
    tools
}

/// A release with the four files a job downloads, the checksums over them, and the bundle that
/// signs the checksums. The bundle of the stand-in `cosign` is the digest of the file it signs.
fn release() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let assets = d.join("assets");
    std::fs::create_dir(&assets).unwrap();
    let files = [
        (
            "1",
            "strata-x86_64-unknown-linux-musl",
            "#!/bin/sh\necho strata 9.9.9\n",
        ),
        ("2", "pr-comment.sh", "#!/bin/sh\necho comment\n"),
        ("3", "verify-report.sh", "#!/bin/sh\necho report\n"),
        ("4", "install-strata.sh", "#!/bin/sh\necho installer\n"),
        ("6", "merge-gate.sh", "#!/bin/sh\necho gate\n"),
        ("7", "upstream-report.sh", "#!/bin/sh\necho upstream\n"),
        (
            "8",
            "strata-preflight-x86_64-unknown-linux-musl",
            "#!/bin/sh\necho strata-preflight 9.9.9\n",
        ),
        ("10", "preflight.sh", "#!/bin/sh\necho preflight\n"),
    ];
    let mut sums = String::new();
    let mut listed = Vec::new();
    for (id, name, body) in files {
        std::fs::write(assets.join(id), body).unwrap();
        sums.push_str(&format!("{}  {}\n", sha256_of(&assets.join(id)), name));
        listed.push(serde_json::json!({"name": name, "id": id.parse::<u32>().unwrap()}));
    }
    std::fs::write(assets.join("5"), &sums).unwrap();
    listed.push(serde_json::json!({"name": "SHA256SUMS", "id": 5}));
    std::fs::write(
        assets.join("9"),
        format!("signed {}", sha256_of(&assets.join("5"))),
    )
    .unwrap();
    listed.push(serde_json::json!({"name": "SHA256SUMS.sigstore.json", "id": 9}));
    std::fs::write(
        d.join("release.json"),
        serde_json::json!({"tag_name": "v9.9.9", "assets": listed}).to_string(),
    )
    .unwrap();
    fake_bin(d, &[("curl", FAKE_RELEASE_CURL)]);
    tools_without_cosign(d);
    dir
}

fn install(d: &Path, args: &[&str]) -> Output {
    Command::new("bash")
        .arg(script("ci/install-strata.sh"))
        .args(args)
        .current_dir(d)
        .env(
            "PATH",
            format!("{}:{}", d.join("bin").display(), d.join("tools").display()),
        )
        .env("FAKE_REL", d)
        .env("STRATA_API", "https://host.example/api")
        .env("STRATA_REPO", "o/r")
        .env_remove("STRATA_TOKEN")
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .output_locked()
        .unwrap()
}

#[test]
fn install_strata_takes_the_binary_and_the_scripts_of_one_release_and_checks_them() {
    let r = release();
    let d = r.path();
    let o = install(
        d,
        &[
            "--version",
            "v9.9.9",
            "--dir",
            "here",
            "--target",
            "x86_64-unknown-linux-musl",
        ],
    );
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    assert_eq!(
        text(&o.stdout).trim(),
        "here/strata",
        "the path of the binary, for STRATA_BIN"
    );
    for name in [
        "strata",
        "strata-preflight",
        "pr-comment.sh",
        "merge-gate.sh",
        "verify-report.sh",
        "upstream-report.sh",
        "SHA256SUMS",
    ] {
        assert!(d.join("here").join(name).exists(), "{name} is installed");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for name in [
            "strata",
            "pr-comment.sh",
            "merge-gate.sh",
            "verify-report.sh",
            "upstream-report.sh",
        ] {
            let mode = std::fs::metadata(d.join("here").join(name))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o111, 0o111, "{name} is executable");
        }
    }
    assert!(
        text(&o.stderr).contains("v9.9.9 installed in here, checksums verified"),
        "{}",
        text(&o.stderr)
    );
}

/// `--bin-only` is how a person installs onto their PATH. The binary arrives, and nothing beside
/// it: no CI scripts, and no SHA256SUMS.
#[test]
fn install_strata_bin_only_installs_the_binary_and_nothing_beside_it() {
    let r = release();
    let d = r.path();
    let o = install(
        d,
        &[
            "--version",
            "v9.9.9",
            "--dir",
            "here",
            "--target",
            "x86_64-unknown-linux-musl",
            "--bin-only",
        ],
    );
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    assert_eq!(
        text(&o.stdout).trim(),
        "here/strata",
        "the path of the binary, for STRATA_BIN"
    );
    assert!(
        d.join("here").join("strata").exists(),
        "strata is installed"
    );
    assert!(
        d.join("here").join("strata-preflight").exists(),
        "strata-preflight is installed beside it, so `strata preflight` runs"
    );
    for name in [
        "pr-comment.sh",
        "merge-gate.sh",
        "verify-report.sh",
        "upstream-report.sh",
        "SHA256SUMS",
    ] {
        assert!(
            !d.join("here").join(name).exists(),
            "{name} is not installed with --bin-only"
        );
    }
}

#[test]
fn install_strata_installs_nothing_when_a_file_does_not_match_the_checksums() {
    let r = release();
    let d = r.path();
    // the release serves another binary than the one SHA256SUMS signed
    std::fs::write(d.join("assets/1"), "#!/bin/sh\necho another build\n").unwrap();
    let o = install(
        d,
        &[
            "--version",
            "v9.9.9",
            "--dir",
            "here",
            "--target",
            "x86_64-unknown-linux-musl",
        ],
    );
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("do not match SHA256SUMS"),
        "{}",
        text(&o.stderr)
    );
    assert!(
        !d.join("here/strata").exists(),
        "the check comes before anything is installed"
    );
}

#[test]
fn install_strata_says_which_file_a_release_does_not_have() {
    let r = release();
    let d = r.path();
    let doc = std::fs::read_to_string(d.join("release.json")).unwrap();
    let doc = doc.replace("\"pr-comment.sh\"", "\"something-else.sh\"");
    std::fs::write(d.join("release.json"), doc).unwrap();
    let o = install(
        d,
        &[
            "--version",
            "v9.9.9",
            "--dir",
            "here",
            "--target",
            "x86_64-unknown-linux-musl",
        ],
    );
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("the release v9.9.9 has no pr-comment.sh"),
        "{}",
        text(&o.stderr)
    );
}

/// A `cosign` that checks a bundle the way a real one checks a signature: the bundle must name the
/// digest of the file. It records its arguments, so a test sees the identity and the issuer. Its
/// `version` prints the line `GitVersion:` as a real `cosign version` does, with the text
/// `@VERSION@` replaced.
const FAKE_COSIGN: &str = r#"#!/usr/bin/env bash
set -euo pipefail
echo "$*" >> "$FAKE_REL/cosign.log"
if [ "$1" = version ]; then
  printf 'GitVersion:    @VERSION@\nGitCommit:     0000\n'
  exit 0
fi
[ "$1" = verify-blob ] || { echo "no such command: $1" >&2; exit 64; }
blob=$2
shift 2
bundle=""
while [ $# -gt 0 ]; do
  case "$1" in
    --bundle) bundle=$2; shift 2 ;;
    *) shift ;;
  esac
done
digest=$(sha256sum "$blob")
[ "$(< "$bundle")" = "signed ${digest%% *}" ] || { echo "Error: invalid signature when validating the bundle" >&2; exit 1; }
echo "Verified OK" >&2
"#;

/// The installer, with a stand-in `cosign` of version 3 in front of the tools, and the arguments
/// of the release `v9.9.9` of `o/r`.
fn install_signed(d: &Path, extra: &[&str]) -> Output {
    install_signed_by(d, "v3.1.4", extra)
}

/// The same, with a stand-in `cosign` that prints `version` as its version.
fn install_signed_by(d: &Path, version: &str, extra: &[&str]) -> Output {
    fake_bin(d, &[("cosign", &FAKE_COSIGN.replace("@VERSION@", version))]);
    let mut args = vec![
        "--version",
        "v9.9.9",
        "--dir",
        "here",
        "--target",
        "x86_64-unknown-linux-musl",
    ];
    args.extend_from_slice(extra);
    install(d, &args)
}

#[test]
fn install_strata_verifies_the_signature_of_the_sums_and_says_so() {
    let r = release();
    let d = r.path();
    let o = install_signed(d, &["--require-signature"]);
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    assert!(
        text(&o.stderr).contains("signature verified"),
        "{}",
        text(&o.stderr)
    );
    assert!(d.join("here/strata").exists(), "the binary is installed");
    let log = std::fs::read_to_string(d.join("cosign.log")).unwrap();
    assert!(
        log.contains(
            "--certificate-identity \
             https://github.com/o/r/.github/workflows/release.yml@refs/tags/v9.9.9"
        ),
        "the identity is the release workflow of the repository, at the tag: {log}"
    );
    assert!(
        log.contains("--certificate-oidc-issuer https://token.actions.githubusercontent.com"),
        "{log}"
    );
}

#[test]
fn install_strata_installs_nothing_when_the_bundle_is_the_signature_of_another_file() {
    let r = release();
    let d = r.path();
    // a valid signature, but of a file that is not SHA256SUMS
    std::fs::write(d.join("assets/9"), "signed 0000").unwrap();
    let o = install_signed(d, &["--require-signature"]);
    assert_eq!(o.status.code(), Some(2));
    let err = text(&o.stderr);
    assert!(
        err.contains("the signature of SHA256SUMS in v9.9.9 does not verify")
            && err.contains("invalid signature"),
        "{err}"
    );
    assert!(
        !d.join("here/strata").exists(),
        "the check comes before anything is installed"
    );

    // the same stop without the option: a signature that fails is never skipped
    let o = install_signed(d, &[]);
    assert_eq!(o.status.code(), Some(2), "{}", text(&o.stderr));
}

#[test]
fn install_strata_without_cosign_installs_and_says_that_it_did_not_verify() {
    let r = release();
    let d = r.path();
    let o = install(
        d,
        &[
            "--version",
            "v9.9.9",
            "--dir",
            "here",
            "--target",
            "x86_64-unknown-linux-musl",
        ],
    );
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    assert!(
        text(&o.stderr).contains("signature not verified: cosign is not on the PATH"),
        "{}",
        text(&o.stderr)
    );
    assert!(d.join("here/strata").exists());
    let calls = std::fs::read_to_string(d.join("curl.log")).unwrap();
    assert!(
        !calls.contains("/releases/assets/9"),
        "no bundle comes down when nothing reads it: {calls}"
    );
}

#[test]
fn install_strata_requires_cosign_before_any_download_when_asked_to() {
    let r = release();
    let d = r.path();
    let o = install(
        d,
        &[
            "--version",
            "v9.9.9",
            "--dir",
            "here",
            "--target",
            "x86_64-unknown-linux-musl",
            "--require-signature",
        ],
    );
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("--require-signature needs cosign"),
        "{}",
        text(&o.stderr)
    );
    assert!(!d.join("curl.log").exists(), "no request went out");
    assert!(!d.join("here").exists(), "nothing is installed");
}

/// The release writes the sums of its scripts with `./*.sh`, so `SHA256SUMS` names a script as
/// `./name`. `sha256sum --check` opens that path, and finds the file in the directory of the
/// installer.
#[test]
fn install_strata_checks_the_sums_that_name_a_script_with_a_leading_dot_and_slash() {
    let r = release();
    let d = r.path();
    let sums = std::fs::read_to_string(d.join("assets/5")).unwrap();
    let dotted: String = sums
        .lines()
        .map(|l| match l.split_once("  ") {
            Some((sum, name)) if name.ends_with(".sh") => format!("{sum}  ./{name}\n"),
            _ => format!("{l}\n"),
        })
        .collect();
    assert!(dotted.contains("  ./pr-comment.sh\n"), "{dotted}");
    std::fs::write(d.join("assets/5"), dotted).unwrap();
    // the signature covers the new sums
    std::fs::write(
        d.join("assets/9"),
        format!("signed {}", sha256_of(&d.join("assets/5"))),
    )
    .unwrap();
    let o = install_signed(d, &["--require-signature"]);
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    assert!(
        d.join("here/pr-comment.sh").exists(),
        "the script is installed"
    );

    // a script that does not match its line still stops the install
    std::fs::write(d.join("assets/2"), "#!/bin/sh\necho another comment\n").unwrap();
    let o = install(
        d,
        &[
            "--version",
            "v9.9.9",
            "--dir",
            "again",
            "--target",
            "x86_64-unknown-linux-musl",
        ],
    );
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("do not match SHA256SUMS"),
        "{}",
        text(&o.stderr)
    );
}

/// The bundles of a release are in the format of cosign 3. A cosign 2 probably cannot read them,
/// so the installer does not call it, and installs as it does without cosign.
#[test]
fn install_strata_with_a_cosign_older_than_3_installs_and_says_that_it_did_not_check() {
    let r = release();
    let d = r.path();
    let o = install_signed_by(d, "v2.4.1", &[]);
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    assert!(
        text(&o.stderr).contains("signature not verified: cosign older than 3"),
        "{}",
        text(&o.stderr)
    );
    assert!(d.join("here/strata").exists(), "the binary is installed");
    let log = std::fs::read_to_string(d.join("cosign.log")).unwrap();
    assert!(
        !log.contains("verify-blob"),
        "a cosign older than 3 checks nothing: {log}"
    );
    let calls = std::fs::read_to_string(d.join("curl.log")).unwrap();
    assert!(
        !calls.contains("/releases/assets/9"),
        "no bundle comes down when nothing reads it: {calls}"
    );
}

#[test]
fn install_strata_requires_cosign_3_before_any_download_when_asked_to() {
    let r = release();
    let d = r.path();
    let o = install_signed_by(d, "v2.4.1", &["--require-signature"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("--require-signature needs cosign 3 or later"),
        "{}",
        text(&o.stderr)
    );
    assert!(!d.join("curl.log").exists(), "no request went out");
    assert!(!d.join("here").exists(), "nothing is installed");
}

/// A version that the installer cannot read does not stop a cosign that works. It runs the check,
/// and the check decides.
#[test]
fn install_strata_tries_a_cosign_that_prints_no_version_number() {
    let r = release();
    let d = r.path();
    let o = install_signed_by(d, "devel", &["--require-signature"]);
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    assert!(
        text(&o.stderr).contains("signature verified"),
        "{}",
        text(&o.stderr)
    );
    // a cosign with two digits in its major version is newer than 3
    let o = install_signed_by(d, "v10.0.0", &["--require-signature"]);
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
}

#[test]
fn install_strata_requires_a_bundle_in_the_release_when_asked_to() {
    let r = release();
    let d = r.path();
    let doc = std::fs::read_to_string(d.join("release.json")).unwrap();
    std::fs::write(
        d.join("release.json"),
        doc.replace("SHA256SUMS.sigstore.json", "something-else.json"),
    )
    .unwrap();
    let o = install_signed(d, &["--require-signature"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("the release v9.9.9 has no SHA256SUMS.sigstore.json"),
        "{}",
        text(&o.stderr)
    );
    assert!(!d.join("here/strata").exists());

    // without the option, the install goes on and says what it did not check
    let o = install_signed(d, &[]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(
        text(&o.stderr).contains("signature not verified: the release v9.9.9 has no"),
        "{}",
        text(&o.stderr)
    );
}

// ---- merge-gate.sh ----------------------------------------------------------------------------

/// A repository with a main branch that holds `releases/dev.json`, and a branch `work` that
/// changes it. `main_side` is what another merge request put on main in the meantime.
struct Gate {
    dir: tempfile::TempDir,
}

impl Gate {
    fn new(base: &str, branch: &str, main_side: &str) -> Gate {
        Gate::build(base, branch, main_side, false)
    }

    /// The same, and the branch then merged main and kept its own record, which is what a hand
    /// merge of a conflict does. The branch carries main, and still pins its own version.
    fn merged_keeping_its_own(base: &str, branch: &str, main_side: &str) -> Gate {
        Gate::build(base, branch, main_side, true)
    }

    fn build(base: &str, branch: &str, main_side: &str, merge_ours: bool) -> Gate {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        git(d, &["init", "-q", "-b", "main"]);
        std::fs::create_dir(d.join("releases")).unwrap();
        std::fs::write(d.join("releases/dev.json"), base).unwrap();
        git(d, &["add", "."]);
        git(d, &["commit", "-q", "-m", "the record"]);
        git(d, &["checkout", "-q", "-b", "work"]);
        std::fs::write(d.join("releases/dev.json"), branch).unwrap();
        git(d, &["commit", "-qam", "the branch moves dev"]);
        git(d, &["checkout", "-q", "main"]);
        if main_side != base {
            std::fs::write(d.join("releases/dev.json"), main_side).unwrap();
            git(d, &["commit", "-qam", "another merge request moved dev"]);
        }
        // a remote named origin is this repository itself, which is what a fetch in CI reads
        git(d, &["remote", "add", "origin", d.to_str().unwrap()]);
        git(d, &["checkout", "-q", "work"]);
        if merge_ours {
            git(d, &["fetch", "-q", "origin", "main"]);
            git(
                d,
                &["merge", "-q", "-X", "ours", "--no-edit", "origin/main"],
            );
        }
        Gate { dir }
    }

    /// Write the record and commit it on the branch that is checked out.
    fn pin(&self, content: &str, message: &str) {
        std::fs::write(self.dir.path().join("releases/dev.json"), content).unwrap();
        git(self.dir.path(), &["commit", "-qam", message]);
    }

    /// What the gate reads to find a rollback a commit declares: the messages of the commits the
    /// branch carries and the target branch does not. A failure of a rollback test says nothing
    /// without this, because the gate only reports that it found no declaration.
    fn branch_messages(&self) -> String {
        let d = self.dir.path();
        let refs = git(d, &["log", "--oneline", "--all", "--decorate"]);
        let range = git(d, &["log", "--format=%B", "HEAD", "--not", "origin/main"]);
        format!(
            "\n-- git log --format=%B HEAD --not origin/main --\n{}\n-- every commit --\n{}",
            text(&range.stdout),
            text(&refs.stdout)
        )
    }

    fn run(&self, args: &[&str]) -> Output {
        self.run_env(args, &[])
    }

    /// The same run, with `env` after the other variables of the gate.
    ///
    /// The git isolation comes last.
    fn run_env(&self, args: &[&str], env: &[(&str, &std::path::Path)]) -> Output {
        let mut c = Command::new("bash");
        c.arg(script("ci/merge-gate.sh"))
            .args(args)
            .current_dir(self.dir.path())
            // a key in the ~/.palette.env of the machine must not reach the gate
            .env("HOME", self.dir.path())
            .env("STRATA_QUIET", "1")
            .env_remove("PALETTE_API_KEY")
            .env_remove("SPECTROCLOUD_APIKEY")
            // The script reads its target branch and its remote from the CI of the caller. A pull
            // request run of this repository sets GITHUB_BASE_REF. Without these lines, a test
            // gets the base of that run and not the base that it gives.
            .env_remove("GITHUB_BASE_REF")
            .env_remove("CI_MERGE_REQUEST_TARGET_BRANCH_NAME")
            .env_remove("STRATA_REMOTE");
        give_strata(&mut c, "STRATA_BIN").envs(env.iter().copied());
        isolate_git(&mut c).output_locked().unwrap()
    }
}

/// The record of fixture a with the pack kubernetes at each of `versions`, in the order given. All
/// carry one header, the way records written by one job do: the ref and the time are what the job
/// wrote, not the change.
fn gate_records_at(versions: &[&str]) -> Vec<String> {
    let dir = tempfile::tempdir().unwrap();
    let raw = dir.path().join("raw");
    common::make_fixtures(&raw);
    versions
        .iter()
        .enumerate()
        .map(|(i, version)| {
            let name = format!("pin-{i}");
            common::rewrite(&raw.join("a"), &raw.join(&name), &[("1.29.4", version)]);
            let out = dir.path().join(format!("{name}.json"));
            snapshot_from_raw(&raw, &name, &out);
            let doc = std::fs::read_to_string(&out).unwrap();
            let doc = field(&doc, "\"ref\": \"", "releases/dev.json");
            field(&doc, "\"fetchedAt\": \"", "2026-09-17T00:00:00Z")
        })
        .collect()
}

/// The record of fixture a, the record the branch pins, and the later record another merge
/// request put on main. All three carry one header, the way records written by one job do.
fn gate_records() -> (String, String, String) {
    let mut records = gate_records_at(&["1.29.4", "1.30.2", "1.31.0"]).into_iter();
    let (base, branch, main_side) = (
        records.next().unwrap(),
        records.next().unwrap(),
        records.next().unwrap(),
    );
    assert_ne!(base, branch);
    assert_ne!(branch, main_side);
    (base, branch, main_side)
}

/// Replace the value that follows the first `key` in a JSON document.
fn field(doc: &str, key: &str, value: &str) -> String {
    let i = doc
        .find(key)
        .unwrap_or_else(|| panic!("no {key} in the document"))
        + key.len();
    let j = doc[i..].find('"').unwrap() + i;
    format!("{}{}{}", &doc[..i], value, &doc[j..])
}

#[test]
fn merge_gate_stops_a_branch_that_would_replace_a_newer_pin() {
    let (base, branch, main_side) = gate_records();
    // the branch settled the conflict by keeping its own record, so git merges and says nothing
    let g = Gate::merged_keeping_its_own(&base, &branch, &main_side);
    let o = g.run(&["releases/*.json", "--base", "main", "--no-verify"]);
    assert_eq!(
        o.status.code(),
        Some(1),
        "the gate is red:\n{}{}",
        text(&o.stdout),
        text(&o.stderr)
    );
    let out = text(&o.stdout);
    assert!(
        out.contains("BEHIND: releases/dev.json replaces what main pinned with an older version"),
        "{out}"
    );
    assert!(
        out.contains("k8s/kubernetes: main has 1.31.0, the merge result has 1.30.2"),
        "the line names the pack and both versions:\n{out}"
    );
    assert!(
        out.contains("main never pinned this release in the last 50 commits of releases/dev.json"),
        "the evidence says this is not a return to anything main ran:\n{out}"
    );
    assert!(
        out.contains("the branch wrote this record before main moved to"),
        "the evidence says the author never saw the later release:\n{out}"
    );
    assert!(
        out.contains("Rollback: releases/dev.json to lab-profile@1.2.0"),
        "the report gives the line that declares a rollback:\n{out}"
    );
    assert!(
        text(&o.stderr).contains("1 record: 1 behind main"),
        "{}",
        text(&o.stderr)
    );
}

/// The red or green gate for a branch that moves the pin of kubernetes from `on_main` to
/// `on_branch`, with the output of the gate.
fn gate_for_pin_move(on_main: &str, on_branch: &str) -> (Output, String) {
    let records = gate_records_at(&[on_main, on_branch]);
    let g = Gate::new(&records[0], &records[1], &records[0]);
    let o = g.run(&["releases/*.json", "--base", "main", "--no-verify"]);
    let out = format!("{}{}", text(&o.stdout), text(&o.stderr));
    (o, out)
}

#[test]
fn merge_gate_reads_the_move_of_a_pin_from_a_pre_release_to_its_final_as_a_move_forward() {
    let (o, out) = gate_for_pin_move("1.30.2-g0123456789ab", "1.30.2");
    assert!(o.status.success(), "the gate is green:\n{out}");
    assert!(out.contains("1 record: 0 behind main"), "{out}");
}

#[test]
fn merge_gate_reads_the_move_of_a_pin_from_a_final_to_its_pre_release_as_a_move_back() {
    let (o, out) = gate_for_pin_move("1.30.2", "1.30.2-g0123456789ab");
    assert_eq!(o.status.code(), Some(1), "the gate is red:\n{out}");
    assert!(
        out.contains("k8s/kubernetes: main has 1.30.2, the merge result has 1.30.2-g0123456789ab"),
        "the line names the pack and both versions:\n{out}"
    );
}

#[test]
fn merge_gate_reads_the_order_of_two_pre_releases_and_of_a_move_to_a_later_version() {
    // the order is that of Semantic Versioning: 11 is above 2, and a later version is above a pre-release
    let (o, out) = gate_for_pin_move("1.30.2-rc.2", "1.30.2-rc.11");
    assert!(o.status.success(), "rc.11 is above rc.2:\n{out}");
    let (o, out) = gate_for_pin_move("1.30.2-rc.11", "1.30.2-rc.2");
    assert_eq!(o.status.code(), Some(1), "rc.2 is below rc.11:\n{out}");
    let (o, out) = gate_for_pin_move("1.30.2-rc.2", "1.31.0");
    assert!(o.status.success(), "1.31.0 is above 1.30.2-rc.2:\n{out}");
    let (o, out) = gate_for_pin_move("1.30.2+5", "1.30.2+4");
    assert!(o.status.success(), "build data has no precedence:\n{out}");
}

#[test]
fn merge_gate_reads_a_move_from_a_version_that_does_not_parse_to_one_that_parses_as_a_move_back() {
    // a version that does not parse is above each version that parses
    let (o, out) = gate_for_pin_move("1.2.3.4", "1.3.0");
    assert_eq!(o.status.code(), Some(1), "the gate is red:\n{out}");
    assert!(
        out.contains("k8s/kubernetes: main has 1.2.3.4, the merge result has 1.3.0"),
        "the line names the pack and both versions:\n{out}"
    );
    let (o, out) = gate_for_pin_move("1.3.0", "1.2.3.4");
    assert!(
        o.status.success(),
        "the move to such a version is forward:\n{out}"
    );
}

/// A branch that deliberately returns the record to the release that main pinned before.
fn rollback_gate() -> (Gate, String) {
    let (base, branch, main_side) = gate_records();
    let g = Gate::merged_keeping_its_own(&base, &branch, &main_side);
    (g, base)
}

#[test]
fn merge_gate_passes_a_rollback_that_a_commit_declares() {
    let (g, base) = rollback_gate();
    g.pin(
        &base,
        "pin the release of last week\n\nRollback: releases/dev.json to lab-profile@1.2.0\n",
    );
    let o = g.run(&["releases/*.json", "--base", "main", "--no-verify"]);
    assert!(
        o.status.success(),
        "{}{}{}",
        text(&o.stdout),
        text(&o.stderr),
        g.branch_messages()
    );
    let out = text(&o.stdout);
    assert!(
        out.contains(
            "ROLLBACK: releases/dev.json returns to lab-profile@1.2.0, declared in a commit of the branch"
        ),
        "{out}{}",
        g.branch_messages()
    );
    assert!(
        out.contains("main pinned this release before, on "),
        "the evidence confirms that main ran this release:\n{out}"
    );
    assert!(
        text(&o.stderr).contains("1 record: 0 behind main, 1 declared rollback"),
        "{}",
        text(&o.stderr)
    );
}

/// A declaration the gate must find however long the branch log is. `git log | grep -q` answers
/// 141 under `set -o pipefail` when grep leaves at the first match and git log still has more to
/// write. A long log then turned a declared rollback into BEHIND. The long message here fills the
/// pipe, which makes that race certain instead of occasional.
#[test]
fn merge_gate_finds_a_declaration_behind_a_log_too_long_for_one_pipe() {
    let (g, base) = rollback_gate();
    // a log larger than one pipe buffer, so git log still has bytes to write when grep leaves.
    // One message must stay under the limit on a single argument, so this is several commits.
    // What they write does not matter: the gate reads the record of HEAD, which is written last.
    for filler in ["{}\n", "[]\n", "{ }\n", "[ ]\n"] {
        g.pin(
            filler,
            &format!(
                "a commit of the branch\n\n{}",
                "filler line\n".repeat(5_000)
            ),
        );
    }
    g.pin(
        &base,
        "pin the release of last week\n\nRollback: releases/dev.json to lab-profile@1.2.0\n",
    );
    let o = g.run(&["releases/*.json", "--base", "main", "--no-verify"]);
    assert!(
        o.status.success(),
        "a long log must not hide the declaration{}{}{}",
        text(&o.stdout),
        text(&o.stderr),
        g.branch_messages()
    );
    assert!(
        text(&o.stdout).contains("ROLLBACK: releases/dev.json returns to lab-profile@1.2.0"),
        "{}",
        text(&o.stdout)
    );
}

#[test]
fn merge_gate_passes_a_rollback_that_the_label_allows() {
    let (g, base) = rollback_gate();
    g.pin(&base, "pin the release of last week");
    let o = g.run(&[
        "releases/*.json",
        "--base",
        "main",
        "--no-verify",
        "--allow-rollback",
    ]);
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    assert!(
        text(&o.stdout).contains(
            "ROLLBACK: releases/dev.json returns to lab-profile@1.2.0, by the label on the merge request"
        ),
        "{}",
        text(&o.stdout)
    );
}

#[test]
fn merge_gate_stays_red_when_the_declaration_names_another_release() {
    let (g, base) = rollback_gate();
    g.pin(
        &base,
        "pin the release of last week\n\nRollback: releases/dev.json to lab-profile@9.9.9\n",
    );
    let o = g.run(&["releases/*.json", "--base", "main", "--no-verify"]);
    assert_eq!(
        o.status.code(),
        Some(1),
        "a declaration counts only for the release it names"
    );
    let out = text(&o.stdout);
    assert!(out.contains("BEHIND: releases/dev.json"), "{out}");
    // and the evidence still tells the reviewer that main ran this release before
    assert!(
        out.contains("main pinned this release before, on "),
        "{out}"
    );
}

#[test]
fn merge_gate_passes_a_branch_that_carries_the_target_branch_and_moves_forward() {
    let (base, branch, main_side) = gate_records();
    // the target branch did not move, and the branch moves the pin forward
    let g = Gate::new(&base, &branch, &base);
    let o = g.run(&["releases/*.json", "--base", "main", "--no-verify"]);
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    assert!(
        text(&o.stderr).contains("the merge result is clear against main"),
        "{}",
        text(&o.stderr)
    );

    // and a branch that settled the conflict by taking the later record is clear too
    let g = Gate::merged_keeping_its_own(&base, &main_side, &main_side);
    let o = g.run(&["releases/*.json", "--base", "main", "--no-verify"]);
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
}

#[test]
fn merge_gate_reads_a_conflict_as_a_release_and_leaves_the_checkout_alone() {
    let (base, branch, main_side) = gate_records();
    let g = Gate::new(&base, &branch, &main_side);
    let o = g.run(&["releases/*.json", "--base", "main", "--no-verify"]);
    assert_eq!(o.status.code(), Some(1));
    let out = text(&o.stdout);
    assert!(
        out.contains("CONFLICT: this branch does not merge origin/main cleanly"),
        "{out}"
    );
    assert!(out.contains("releases/dev.json"), "{out}");
    assert!(
        out.contains("1.31.0") && out.contains("1.30.2"),
        "the conflict is explained as the two releases:\n{out}"
    );
    let status = git(g.dir.path(), &["status", "--porcelain"]);
    assert!(
        text(&status.stdout).trim().is_empty(),
        "the merge is aborted, so the job leaves a clean checkout: {}",
        text(&status.stdout)
    );
}

#[test]
fn merge_gate_needs_a_target_branch_and_a_record() {
    let (base, branch, main_side) = gate_records();
    let g = Gate::new(&base, &branch, &main_side);
    let o = g.run(&["releases/*.json", "--no-verify"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("pass --base"),
        "{}",
        text(&o.stderr)
    );

    // a branch that merges cleanly, so the patterns are what is left to fail on
    let clean = Gate::new(&base, &branch, &base);
    let o = clean.run(&["releases/none/*.json", "--base", "main", "--no-verify"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("no record in the merge result matches"),
        "{}",
        text(&o.stderr)
    );
}

#[test]
fn merge_gate_says_when_it_does_not_verify_and_finds_the_key_where_strata_does() {
    let (base, branch, _) = gate_records();
    // a branch that merges cleanly and moves no pin back, so verify is the only check left
    let g = Gate::new(&base, &branch, &base);

    let o = g.run(&["releases/*.json", "--base", "main", "--no-verify"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    assert!(
        text(&o.stderr).contains("not verified: --no-verify"),
        "{}",
        text(&o.stderr)
    );

    // no key anywhere: the gate omits verify, and the last line says so
    let o = g.run(&["releases/*.json", "--base", "main"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    assert!(
        text(&o.stderr).contains(
            "not verified: no Palette key in PALETTE_API_KEY, SPECTROCLOUD_APIKEY, or ~/.palette.env"
        ),
        "{}",
        text(&o.stderr)
    );

    // a key in ~/.palette.env alone is a key, as strata reads it: the gate verifies. This address
    // answers nothing, so the record cannot be checked, and the gate fails closed.
    let home = tempfile::tempdir().unwrap();
    std::fs::write(
        home.path().join(".palette.env"),
        format!("PALETTE_API_KEY=k\nPALETTE_API={REFUSED}\nSTRATA_RETRIES=0\n"),
    )
    .unwrap();
    let o = g.run_env(
        &["releases/*.json", "--base", "main"],
        &[("HOME", home.path())],
    );
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    assert!(
        text(&o.stdout).contains("ERROR: releases/dev.json could not be checked"),
        "{}",
        text(&o.stdout)
    );
    assert!(
        text(&o.stderr).contains("1 did not verify"),
        "{}",
        text(&o.stderr)
    );
}

/// A `strata` for the gate. `verify` answers as for a record with a pack that has no upstream
/// digest, unless `--allow-no-digest` accepts it. Every other command is the real strata. Each
/// call of verify is logged.
const FAKE_UNVERIFIED_STRATA: &str = r#"#!/usr/bin/env bash
if [ "$1" != verify ]; then exec "$REAL_STRATA" "$@"; fi
echo "$*" >> "$FAKE_LOG"
case " $* " in *" --allow-no-digest "*) echo "OK: unchanged"; exit 0 ;; esac
case " $* " in
  *" --summary "*) echo '{"records":[{"path":"releases/dev.json","status":"unverified"}]}' ;;
  *) echo "UNVERIFIED: csi/csi-aws-ebs has no upstream digest" ;;
esac
exit 1
"#;

/// A `strata` of release 0.25.0 for the gate. It writes no `versionMove`: its schema of the
/// comparison lacks the field, and a comparison in JSON carries none. Every other answer is the
/// real strata.
const FAKE_OLD_STRATA: &str = r#"#!/usr/bin/env bash
case "$1" in
  schema) "$REAL_STRATA" "$@" | jq 'del(.. | .versionMove?)' ;;
  compare)
    case " $* " in
      *" json "*) "$REAL_STRATA" "$@" | jq 'del(.versionMove) | del(.packs[]?.versionMove)' ;;
      *) exec "$REAL_STRATA" "$@" ;;
    esac ;;
  *) exec "$REAL_STRATA" "$@" ;;
esac
"#;

#[test]
fn merge_gate_stops_with_exit_2_when_strata_is_older_than_the_script() {
    // a pin that moves back from 1.31.0 to 1.30.2: with the strata of 0.25.0 no move is backward,
    // so the gate was green
    let records = gate_records_at(&["1.31.0", "1.30.2"]);
    let g = Gate::new(&records[0], &records[1], &records[0]);
    let bin = tempfile::tempdir().unwrap();
    let old = bin.path().join("strata");
    executable(&old, FAKE_OLD_STRATA);
    let args = ["releases/*.json", "--base", "main", "--no-verify"];
    let o = g.run_env(&args, &[("STRATA_BIN", old.as_path())]);
    let err = text(&o.stderr);
    assert_eq!(o.status.code(), Some(2), "{err}{}", text(&o.stdout));
    assert!(
        err.contains("merge-gate: strata is older than this script"),
        "{err}"
    );
    assert!(!text(&o.stdout).contains("BEHIND"), "{}", text(&o.stdout));

    // the strata of this release finds the backward pin
    let o = g.run(&args);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stdout));
    assert!(text(&o.stdout).contains("BEHIND"), "{}", text(&o.stdout));
}

#[test]
fn merge_gate_names_an_unverified_record_and_passes_allow_no_digest_on() {
    let (base, branch, _) = gate_records();
    let g = Gate::new(&base, &branch, &base);
    let bin = tempfile::tempdir().unwrap();
    let fake = bin.path().join("strata");
    executable(&fake, FAKE_UNVERIFIED_STRATA);
    let log = bin.path().join("verify.log");
    let run = |extra: &[&str]| {
        let mut c = Command::new("bash");
        c.arg(script("ci/merge-gate.sh"))
            .args(["releases/*.json", "--base", "main"])
            .args(extra)
            .current_dir(g.dir.path())
            .env("HOME", g.dir.path())
            .env("STRATA_BIN", &fake)
            .env("FAKE_LOG", &log)
            .env("STRATA_QUIET", "1")
            .env("PALETTE_API_KEY", "k")
            .env_remove("GITHUB_BASE_REF")
            .env_remove("CI_MERGE_REQUEST_TARGET_BRANCH_NAME")
            .env_remove("STRATA_REMOTE");
        give_strata(&mut c, "REAL_STRATA");
        isolate_git(&mut c).output_locked().unwrap()
    };

    // red, and the report says why: a pack with no digest, not a drift
    let o = run(&[]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    let out = text(&o.stdout);
    assert!(
        out.contains("UNVERIFIED: releases/dev.json holds a pack with no upstream digest"),
        "{out}"
    );
    assert!(out.contains("--allow-no-digest accepts it"), "{out}");
    assert!(!out.contains("DRIFTED"), "{out}");

    // the option reaches verify, and the gate is clear
    std::fs::remove_file(&log).unwrap();
    let o = run(&["--allow-no-digest"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let calls = lines_of(&log);
    assert!(
        !calls.is_empty() && calls.iter().all(|l| l.contains("--allow-no-digest")),
        "{calls:?}"
    );
}

// ---- verify-report.sh ------------------------------------------------------------------------

/// A `strata` for verify-report.sh. The call `verify --summary ...` answers from summary.json and
/// summary.exit. The call `verify <path> --format markdown` answers from <basename>.md, .err, and
/// .exit. All these files are under `FAKE_VERIFY`. This `strata` logs every call.
const FAKE_VERIFY_STRATA: &str = r#"#!/usr/bin/env bash
echo "$*" >> "$FAKE_VERIFY/strata.log"
[ "$1" = verify ] || exit 9
if [ "$2" = --summary ]; then
  cat "$FAKE_VERIFY/summary.json" 2>/dev/null
  cat "$FAKE_VERIFY/summary.err" >&2 2>/dev/null
  exit "$(cat "$FAKE_VERIFY/summary.exit")"
fi
key=$(basename "$2" .json)
cat "$FAKE_VERIFY/$key.md" 2>/dev/null
[ -f "$FAKE_VERIFY/$key.err" ] && cat "$FAKE_VERIFY/$key.err" >&2
exit "$(cat "$FAKE_VERIFY/$key.exit")"
"#;

/// The stand-ins of one script that opens issues: verify-report.sh or upstream-report.sh.
///
/// The directory holds a `strata` that answers from the files that a test puts there. Its `gh`
/// and `curl` log each call and keep each body there.
struct Report {
    script: PathBuf,
    dir: tempfile::TempDir,
}

impl Report {
    /// The stand-ins of the script `name` under scripts/, with `strata_stand_in` as its strata.
    fn new(name: &str, strata_stand_in: &str) -> Report {
        let dir = tempfile::tempdir().unwrap();
        fake_bin(
            dir.path(),
            &[
                ("strata", strata_stand_in),
                ("gh", FAKE_ISSUES_GH),
                ("curl", FAKE_GL_CURL),
            ],
        );
        Report {
            script: script(name),
            dir,
        }
    }

    /// verify-report.sh, with the strata that answers `verify`.
    fn verify() -> Report {
        Report::new("ci/verify-report.sh", FAKE_VERIFY_STRATA)
    }

    /// Write the file `name` where the stand-ins read it.
    fn put(&self, name: &str, body: &str) {
        std::fs::write(self.dir.path().join(name), body).unwrap();
    }

    /// Run the script with the stand-in `jq` `stand_in`: see [`JQ_1_6`] and [`JQ_STRICT_TRIM`].
    fn use_jq(&self, stand_in: &str) {
        executable(&self.dir.path().join("bin/jq"), stand_in);
    }

    /// The summary that strata prints for verify-report.sh, from (path, status, name, version)
    /// per record.
    fn summary(&self, exit: i32, records: &[(&str, &str, &str, &str)]) {
        let records: Vec<serde_json::Value> = records
            .iter()
            .map(|(path, status, name, version)| {
                let mut r = serde_json::json!({"path": path, "status": status, "name": name,
                    "version": version, "uid": UID_A, "recordedAt": "2026-09-16T01:56:23Z"});
                if *status == "error" {
                    r["error"] = serde_json::json!({"kind": "api", "message": "GET /v1/clusterprofiles/64f0 failed with HTTP 404"});
                }
                r
            })
            .collect();
        self.put(
            "summary.json",
            &serde_json::json!({"schema": "strata-verify-summary/1", "records": records})
                .to_string(),
        );
        self.put("summary.exit", &exit.to_string());
    }

    /// The report and the exit code of `strata verify <path> --format markdown` for one record,
    /// for verify-report.sh.
    fn record(&self, key: &str, exit: i32, report: &str, err: &str) {
        self.put(&format!("{key}.md"), report);
        self.put(&format!("{key}.exit"), &exit.to_string());
        let e = self.dir.path().join(format!("{key}.err"));
        if err.is_empty() {
            let _ = std::fs::remove_file(e);
        } else {
            std::fs::write(e, err).unwrap();
        }
    }

    /// Run the script as a GitHub job does, with no variable of a GitLab job.
    fn run(&self, args: &[&str]) -> Output {
        self.run_with(args, &[])
    }

    /// Run the script with `envs` after the other variables.
    fn run_with(&self, args: &[&str], envs: &[(&str, &str)]) -> Output {
        // a test reads the calls of one run, not of the runs before it. An empty log means no call,
        // and `lines_of` stops the test when the log is gone.
        let _ = std::fs::remove_file(self.dir.path().join("since-used.json"));
        for f in ["gh.log", "strata.log", "curl.log"] {
            std::fs::write(self.dir.path().join(f), "").unwrap();
        }
        let mut c = Command::new("bash");
        c.arg(&self.script)
            .args(args)
            .env("PATH", path_with(&self.dir.path().join("bin")))
            .env("FAKE_VERIFY", self.dir.path())
            .env_remove("STRATA_BIN")
            .env_remove("GITHUB_REPOSITORY")
            .envs(envs.iter().copied());
        c.output_locked().unwrap()
    }

    /// Run the script as a GitLab job does: the stand-in curl answers from the same directory,
    /// so its log is there too.
    fn run_gitlab(&self, args: &[&str]) -> Output {
        let dir = self.dir.path().display().to_string();
        self.run_with(args, &gitlab_job(&dir))
    }

    /// The lines of the log `name` of a stand-in, or none when it made no call.
    fn log(&self, name: &str) -> Vec<String> {
        lines_of(&self.dir.path().join(name))
    }

    /// The body or comment handed to gh on the call with this 1-based number.
    fn handed(&self, kind: &str, call: usize) -> String {
        handed_in(self.dir.path(), kind, call)
    }
}

#[test]
fn verify_report_opens_one_issue_per_record_that_did_not_verify_and_closes_it_once_it_does() {
    let d = Report::verify();
    d.summary(
        2,
        &[
            ("releases/a.json", "unchanged", "lab-profile", "1.2.0"),
            ("releases/b.json", "drifted", "lab-profile", "1.3.0"),
            ("releases/c.json", "error", "lab-profile", "1.0.0"),
        ],
    );
    d.record("b", 1, "## lab-profile@1.3.0\n\n**Result:** CHANGED\n", "");
    d.record(
        "c",
        2,
        "",
        "strata: GET /v1/clusterprofiles/64f0 failed with HTTP 404\n",
    );

    // first run: no issues yet, and no label
    let o = d.run(&["releases/*.json", "--repo", "o/r"]);
    assert_eq!(
        o.status.code(),
        Some(2),
        "a record that could not be checked: {}",
        text(&o.stderr)
    );
    assert_eq!(
        d.log("strata.log"),
        vec![
            "verify --summary --quiet --format json releases/*.json",
            "verify releases/b.json --quiet --format markdown",
            "verify releases/c.json --quiet --format markdown",
        ],
        "one summary, then each record that did not verify on its own for its report"
    );
    let calls = d.log("gh.log");
    assert_eq!(calls.len(), 5, "{calls:#?}");
    assert!(
        calls[0].starts_with("issue list --repo o/r --label strata-drift --state open"),
        "{}",
        calls[0]
    );
    assert!(
        calls[1].starts_with("label list --repo o/r"),
        "{}",
        calls[1]
    );
    assert!(
        calls[2].starts_with("label create strata-drift --repo o/r"),
        "{}",
        calls[2]
    );
    assert!(
        calls[3].starts_with("issue create --repo o/r --title strata verify: lab-profile@1.3.0 drifted (releases/b.json)"),
        "{}",
        calls[3]
    );
    assert!(calls[3].contains("--label strata-drift"), "{}", calls[3]);
    assert!(
        !calls[3].contains("--milestone"),
        "the shipped script sets no milestone: {}",
        calls[3]
    );
    let b = d.handed("body", 4);
    assert!(
        b.starts_with("<!-- strata-verify: releases/b.json -->\n"),
        "{b}"
    );
    assert!(
        b.contains("`strata verify releases/b.json` reports drift"),
        "{b}"
    );
    assert!(
        b.contains("**Result:** CHANGED"),
        "the report is in the issue: {b}"
    );
    assert!(
        calls[4].starts_with(
            "issue create --repo o/r --title strata verify: releases/c.json could not be checked"
        ),
        "{}",
        calls[4]
    );
    let c = d.handed("body", 5);
    assert!(
        c.starts_with("<!-- strata-verify: releases/c.json -->\n"),
        "{c}"
    );
    assert!(
        c.contains("api: GET /v1/clusterprofiles/64f0 failed with HTTP 404"),
        "{c}"
    );
    let err = text(&o.stderr);
    assert!(
        err.contains("3 records: 1 unchanged, 1 drifted, 1 could not be checked; opened #41 for releases/b.json; opened #42 for releases/c.json"),
        "{err}"
    );
    assert!(
        !err.contains('✓'),
        "gh's success messages stay out of the report: {err}"
    );

    // second run: b still drifts, c verifies again, a keeps verifying and has an old issue; an
    // issue for a record not given stays open
    d.summary(
        1,
        &[
            ("releases/a.json", "unchanged", "lab-profile", "1.2.0"),
            ("releases/b.json", "drifted", "lab-profile", "1.3.0"),
            ("releases/c.json", "error", "lab-profile", "1.0.0"),
        ],
    );
    d.record(
        "b",
        1,
        "## lab-profile@1.3.0\n\n**Result:** CHANGED, again\n",
        "",
    );
    d.record("c", 0, "", "");
    d.put(
        "issues.json",
        r#"[{"number":7,"title":"old","body":"<!-- strata-verify: releases/a.json -->\nold"},
            {"number":41,"title":"t","body":"<!-- strata-verify: releases/b.json -->\r\nedited on the web"},
            {"number":42,"title":"t","body":"<!-- strata-verify: releases/c.json -->\nold"},
            {"number":50,"title":"t","body":"<!-- strata-verify: releases/other.json -->\nold"}]"#,
    );
    let o = d.run(&["releases/*.json", "--repo", "o/r"]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    let calls = d.log("gh.log");
    assert_eq!(calls.len(), 4, "{calls:#?}");
    assert!(
        calls[1].starts_with("issue close 7 --repo o/r --comment"),
        "{}",
        calls[1]
    );
    assert!(
        d.handed("comment", 2).contains("`releases/a.json` verifies again: lab-profile@1.2.0 is unchanged since 2026-09-16T01:56:23Z"),
        "{}",
        d.handed("comment", 2)
    );
    assert!(
        calls[2].starts_with("issue edit 41 --repo o/r --title strata verify: lab-profile@1.3.0 drifted (releases/b.json)"),
        "{}",
        calls[2]
    );
    assert!(d.handed("body", 3).contains("CHANGED, again"));
    assert!(
        calls[3].starts_with("issue close 42 --repo o/r --comment"),
        "{}",
        calls[3]
    );
    assert!(
        calls
            .iter()
            .all(|c| !c.contains(" 50 ") && !c.starts_with("issue create")),
        "exactly one issue per record, and none touched for a record not given: {calls:#?}"
    );
    assert!(
        text(&o.stderr).contains("3 records: 2 unchanged, 1 drifted, 0 could not be checked; closed #7 for releases/a.json; updated #41 for releases/b.json; closed #42 for releases/c.json"),
        "{}",
        text(&o.stderr)
    );
    assert!(!text(&o.stderr).contains('✓'), "{}", text(&o.stderr));

    // a host call that fails stops the script and shows what the host said
    d.put(
        "issues.json",
        r#"[{"number":99,"title":"t","body":"<!-- strata-verify: releases/a.json -->\nold"}]"#,
    );
    let o = d.run(&["releases/*.json", "--repo", "o/r"]);
    assert_ne!(o.status.code(), Some(0));
    assert!(
        text(&o.stderr).contains("Could not resolve to an issue with the number of 99"),
        "{}",
        text(&o.stderr)
    );

    // a clean run with no open issues only lists them
    d.summary(
        0,
        &[("releases/a.json", "unchanged", "lab-profile", "1.2.0")],
    );
    d.put("issues.json", "[]");
    let o = d.run(&["releases/a.json", "--repo", "o/r"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    assert_eq!(d.log("gh.log").len(), 1);
}

#[test]
fn verify_report_opens_updates_and_closes_gitlab_issues_the_same_way() {
    let d = Report::verify();
    d.summary(
        2,
        &[
            ("releases/a.json", "unchanged", "lab-profile", "1.2.0"),
            ("releases/b.json", "drifted", "lab-profile", "1.3.0"),
            ("releases/c.json", "error", "lab-profile", "1.0.0"),
        ],
    );
    d.record("a", 0, "", "");
    d.record("b", 1, "| `~` | k8s | `kubernetes` |\n", "");
    d.record(
        "c",
        2,
        "",
        "api: GET /v1/clusterprofiles/64f0 failed with HTTP 404\n",
    );
    // one issue is already open for the record that verifies again, and one for the record that
    // still cannot be checked
    d.put(
        "gl-issues.json",
        r#"[{"iid": 7, "title": "strata verify: lab-profile@1.2.0 drifted (releases/a.json)",
             "description": "<!-- strata-verify: releases/a.json -->\nan older report"},
            {"iid": 9, "title": "strata verify: releases/c.json could not be checked",
             "description": "<!-- strata-verify: releases/c.json -->\nan older report"}]"#,
    );

    let o = d.run_gitlab(&["releases/*.json"]);
    assert_eq!(o.status.code(), Some(2), "{}", text(&o.stderr));
    let calls = d.log("curl.log");
    assert!(
        calls[0].starts_with("GET /projects/42/issues?labels=strata-drift&state=opened"),
        "{calls:?}"
    );
    // the record that verifies again is answered with a note and then closed
    let note = calls
        .iter()
        .position(|c| {
            c.starts_with("POST /projects/42/issues/7/notes body=`releases/a.json` verifies again")
        })
        .expect("the closing comment says why");
    let close = calls
        .iter()
        .position(|c| c == "PUT /projects/42/issues/7 state_event=close")
        .expect("and then the issue closes");
    assert!(note < close, "{calls:?}");
    // the record that drifted has no issue yet, so the label comes first and the issue is new
    let label = calls
        .iter()
        .position(|c| c.starts_with("POST /projects/42/labels name=strata-drift"))
        .expect("a label the project lacks is created first");
    let open = calls
        .iter()
        .position(|c| {
            c.starts_with("POST /projects/42/issues title=strata verify: lab-profile@1.3.0 drifted (releases/b.json)")
                && c.contains("labels=strata-drift")
        })
        .expect("the new issue carries the label");
    assert!(label < open, "{calls:?}");
    // the record that still cannot be checked updates the issue it already has
    assert!(
        calls.iter().any(|c| c.starts_with(
            "PUT /projects/42/issues/9 title=strata verify: releases/c.json could not be checked"
        )),
        "{calls:?}"
    );
    assert!(
        !calls.iter().any(|c| c.starts_with("POST /projects/42/issues title=strata verify: releases/c.json")),
        "no second issue for a record that has one: {calls:?}"
    );
    assert!(
        text(&o.stderr).contains("3 records: 1 unchanged, 1 drifted, 1 could not be checked"),
        "{}",
        text(&o.stderr)
    );
}

/// What a `strata` that checked nothing prints on standard output: nothing, or only white space.
const PRINTED_NOTHING: [&str; 2] = ["", " \n"];

/// The exit codes that a `strata` that checked nothing can have: a crash gives 2, and a `strata`
/// that fails without a code gives 0.
const EXITS_OF_NOTHING: [&str; 2] = ["0", "2"];

#[test]
fn verify_report_touches_no_issue_when_verify_checks_nothing() {
    for old_jq in [false, true] {
        for printed in PRINTED_NOTHING {
            for exit in EXITS_OF_NOTHING {
                let d = Report::verify();
                if old_jq {
                    d.use_jq(JQ_1_6);
                }
                d.put("summary.json", printed);
                d.put(
                    "summary.err",
                    r#"{"error":{"kind":"pattern","message":"'releases/*.json' matched no files"}}"#,
                );
                d.put("summary.exit", exit);
                let o = d.run(&["releases/*.json", "--repo", "o/r"]);
                let case =
                    format!("old jq: {old_jq}, strata printed {printed:?} and exited {exit}");
                assert_eq!(o.status.code(), Some(2), "{case}");
                assert!(d.log("gh.log").is_empty(), "{case}: {:?}", d.log("gh.log"));
                let err = text(&o.stderr);
                assert!(err.contains("matched no files"), "{case}: {err}");
                assert!(
                    err.contains(&format!(
                        "verify-report: strata verify checked nothing (exit {exit})"
                    )),
                    "{case}: {err}"
                );
            }
        }
    }
}

/// The stand-in `jq`s that a run of a script uses. They are the `jq` of the machine and the `jq`
/// of `ubuntu-latest`, which stops on a null in `rtrimstr`.
fn jq_stand_ins() -> [(&'static str, Option<&'static str>); 2] {
    [
        ("the jq of the machine", None),
        ("the jq of ubuntu-latest", Some(JQ_STRICT_TRIM)),
    ]
}

#[test]
fn the_jq_of_ubuntu_latest_stops_on_a_null_in_rtrimstr_and_trims_a_string() {
    let dir = tempfile::tempdir().unwrap();
    let bin = fake_bin(dir.path(), &[("jq", JQ_STRICT_TRIM)]);
    let jq = |program: &str| {
        Command::new(bin.join("jq"))
            .args(["-n", "-c", program])
            .env("PATH", path_with(&bin))
            .output_locked()
            .unwrap()
    };
    let o = jq(r#"null | rtrimstr("\r")"#);
    assert!(!o.status.success(), "{}", text(&o.stdout));
    assert!(
        text(&o.stderr).contains("endswith() requires string inputs"),
        "{}",
        text(&o.stderr)
    );
    let o = jq(r#"["a\r", "b", ""] | map(rtrimstr("\r"))"#);
    assert_eq!(
        text(&o.stdout),
        "[\"a\",\"b\",\"\"]\n",
        "{}",
        text(&o.stderr)
    );
}

#[test]
fn verify_report_goes_on_when_an_open_issue_has_no_text_in_its_body() {
    // GitHub gives `"body": null` for an issue that a person opens with no text
    for (jq, stand_in) in jq_stand_ins() {
        let d = Report::verify();
        if let Some(stand_in) = stand_in {
            d.use_jq(stand_in);
        }
        d.summary(
            1,
            &[
                ("releases/a.json", "unchanged", "lab-profile", "1.2.0"),
                ("releases/b.json", "drifted", "lab-profile", "1.3.0"),
            ],
        );
        d.record("b", 1, "**Result:** CHANGED\n", "");
        d.put(
            "issues.json",
            r#"[{"number":3,"title":"t","body":null},
                {"number":4,"title":"t","body":""},
                {"number":7,"title":"t","body":"<!-- strata-verify: releases/a.json -->\nold"}]"#,
        );
        let o = d.run(&["releases/*.json", "--repo", "o/r"]);
        assert_eq!(o.status.code(), Some(1), "{jq}: {}", text(&o.stderr));
        let calls = d.log("gh.log");
        assert!(
            calls
                .iter()
                .any(|c| c.starts_with("issue close 7 --repo o/r")),
            "{jq}: the issue of a record that verifies closes: {calls:#?}"
        );
        assert!(
            calls
                .iter()
                .any(|c| c.starts_with("issue create --repo o/r")),
            "{jq}: the record that drifted gets an issue: {calls:#?}"
        );
        assert!(
            calls
                .iter()
                .all(|c| !c.starts_with("issue close 3 ") && !c.starts_with("issue close 4 ")),
            "{jq}: an issue with no text is not touched: {calls:#?}"
        );
    }
}

#[test]
fn verify_report_dry_run_prints_each_report_and_calls_no_host() {
    let d = Report::verify();
    d.summary(
        1,
        &[
            ("releases/a.json", "unchanged", "lab-profile", "1.2.0"),
            ("releases/b.json", "drifted", "lab-profile", "1.3.0"),
        ],
    );
    d.record("b", 1, "**Result:** CHANGED\n", "");
    let o = d.run(&["releases/*.json", "--dry-run"]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    assert!(d.log("gh.log").is_empty(), "{:?}", d.log("gh.log"));
    let out = text(&o.stdout);
    assert!(
        out.contains("<!-- strata-verify: releases/b.json -->"),
        "{out}"
    );
    assert!(out.contains("**Result:** CHANGED"), "{out}");
    let err = text(&o.stderr);
    assert!(
        err.contains("would close the open issue for releases/a.json, if any"),
        "{err}"
    );
    assert!(
        err.contains("would open or update the issue for releases/b.json"),
        "{err}"
    );
}

#[test]
fn verify_report_opens_an_issue_for_an_unverified_record_and_passes_allow_no_digest_on() {
    let d = Report::verify();
    d.summary(
        1,
        &[
            ("releases/a.json", "unchanged", "lab-profile", "1.2.0"),
            ("releases/b.json", "unverified", "lab-profile", "1.3.0"),
        ],
    );
    d.record(
        "b",
        1,
        "**UNVERIFIED**: pack addon/x has no upstream digest\n",
        "",
    );
    let o = d.run(&["releases/*.json", "--dry-run"]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    let out = text(&o.stdout);
    assert!(out.contains("cannot verify the record"), "{out}");
    assert!(out.contains("**UNVERIFIED**: pack addon/x"), "{out}");
    let err = text(&o.stderr);
    assert!(
        err.contains("would open or update the issue for releases/b.json: strata verify: lab-profile@1.3.0 cannot be verified (releases/b.json)"),
        "{err}"
    );
    assert!(
        err.contains("2 records: 1 unchanged, 0 drifted, 1 unverified, 0 could not be checked"),
        "{err}"
    );

    // the option reaches both calls of verify
    let o = d.run(&["releases/*.json", "--allow-no-digest", "--dry-run"]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    let log = d.log("strata.log");
    assert!(
        log.len() == 2 && log.iter().all(|l| l.contains("--allow-no-digest")),
        "{log:?}"
    );
}

#[test]
fn verify_report_asks_who_changed_a_record_that_drifted_and_puts_the_answer_in_its_issue() {
    let d = Report::verify();
    d.summary(
        1,
        &[
            ("releases/a.json", "unchanged", "lab-profile", "1.2.0"),
            ("releases/b.json", "drifted", "lab-profile", "1.3.0"),
        ],
    );
    // what `strata verify --who` adds to the report of a record that drifted
    d.record(
        "b",
        1,
        "**DRIFT**: published version lab-profile@1.3.0 differs\n\n**who:** 1 write in the audit log since the record\n\n- 2026-09-18T14:02:11.482Z John Doe (user) update\n",
        "",
    );

    // without the option, the check of the record asks for no audit log
    let o = d.run(&["releases/*.json", "--dry-run"]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    assert!(
        d.log("strata.log").iter().all(|l| !l.contains("--who")),
        "{:?}",
        d.log("strata.log")
    );

    // with it, the check of each record that did not verify asks, and the summary does not
    let o = d.run(&["releases/*.json", "--who", "--dry-run"]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    let log = d.log("strata.log");
    assert_eq!(log.len(), 2, "{log:?}");
    assert!(
        log[0].contains("--summary") && !log[0].contains("--who"),
        "{log:?}"
    );
    assert_eq!(
        log[1],
        "verify releases/b.json --quiet --format markdown --who"
    );
    let out = text(&o.stdout);
    assert!(out.contains("John Doe (user) update"), "{out}");
    assert!(
        out.starts_with("<!-- strata-verify: releases/b.json -->"),
        "{out}"
    );

    // the issue that a run opens holds the same body
    let o = d.run(&["releases/*.json", "--who", "--repo", "o/r"]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    let create = d
        .log("gh.log")
        .iter()
        .position(|l| l.starts_with("issue create"))
        .expect("an issue is opened")
        + 1;
    assert!(
        d.handed("body", create).contains("John Doe (user) update"),
        "{}",
        d.handed("body", create)
    );
}

#[test]
fn live_scripts_refuse_a_strata_binary_older_than_the_sources() {
    let root = tempfile::tempdir().unwrap();
    let live = root.path().join("examples/live");
    let release = root.path().join("target/release");
    for d in [&live, &release, &root.path().join("src")] {
        std::fs::create_dir_all(d).unwrap();
    }
    copy_scripts(
        root.path(),
        &["examples/live/lib.sh", "examples/live/common.sh"],
    );
    executable(
        &live.join("probe.sh"),
        "script=probe\n. \"$(dirname \"$0\")/lib.sh\"\necho ran \"$strata\"\n",
    );
    let strata = release.join("strata");
    executable(&strata, "#!/bin/sh\n");
    let source = root.path().join("src/lib.rs");
    std::fs::write(&source, "// changed after the build\n").unwrap();
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(600);
    std::fs::File::options()
        .write(true)
        .open(&strata)
        .unwrap()
        .set_modified(old)
        .unwrap();
    let probe = || {
        Command::new("bash")
            .arg(live.join("probe.sh"))
            .env_remove("STRATA_BIN")
            .output_locked()
            .unwrap()
    };

    let o = probe();
    assert_eq!(o.status.code(), Some(2), "{}", text(&o.stdout));
    assert!(
        text(&o.stderr).contains(&format!(
            "probe: {} is older than {}; run just build",
            strata.display(),
            source.display()
        )),
        "{}",
        text(&o.stderr)
    );

    // rebuilt: the binary is newer than every source
    std::fs::File::options()
        .write(true)
        .open(&strata)
        .unwrap()
        .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(5))
        .unwrap();
    let o = probe();
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));

    // a binary chosen with STRATA_BIN is the caller's to keep current
    std::fs::write(&source, "// changed again\n").unwrap();
    std::fs::File::options()
        .write(true)
        .open(&source)
        .unwrap()
        .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(60))
        .unwrap();
    let o = Command::new("bash")
        .arg(live.join("probe.sh"))
        .env("STRATA_BIN", &strata)
        .output_locked()
        .unwrap();
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
}

/// The calls of `jq` that a script of the live example makes on an empty input, each with the exit
/// code it prints. lib.sh is the first argument.
const JQ_PROBE: &str = r#"
STRATA_BIN=/bin/true script=probe . "$1"
: | jq -e . > /dev/null;                          echo "empty input $?"
: > empty.json; jq -e '.a == 1' empty.json;       echo "empty file $?"
bash -c ': | jq -e . > /dev/null; echo "in bash -c $?"'
echo '{"a":1}' | jq -e '.a == 1' > /dev/null;     echo "true $?"
echo '{"a":1}' | jq -e '.a == 2' > /dev/null;     echo "false $?"
echo '{"a":1}' | jq -er '.b // empty' > /dev/null; echo "no output $?"
echo '{}' | jq -r '.a // empty';                  echo "no -e $?"
"#;

#[test]
fn live_scripts_take_jq_e_on_an_empty_input_as_a_failure_with_each_jq() {
    for old_jq in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let commands: &[(&str, &str)] = if old_jq { &[("jq", JQ_1_6)] } else { &[] };
        let bin = fake_bin(dir.path(), commands);
        let o = Command::new("bash")
            .args(["-c", JQ_PROBE, "probe"])
            .arg(live_script("lib.sh"))
            .current_dir(dir.path())
            .env("PATH", path_with(&bin))
            .env_remove("STRATA_BIN")
            .output_locked()
            .unwrap();
        assert_eq!(
            text(&o.stdout),
            "empty input 4\nempty file 4\nin bash -c 4\ntrue 0\nfalse 1\nno output 4\nno -e 0\n",
            "old jq: {old_jq}: {}",
            text(&o.stderr)
        );
    }
}

/// A `tofu` for lib.sh's applies: logs each call and has no outputs.
const FAKE_TOFU_APPLY: &str = r#"#!/usr/bin/env bash
echo "$*" >> "$FAKE_TF_LOG"
case "$*" in
  *"output -json"*) echo '{}' ;;
esac
"#;

#[test]
fn live_applies_take_the_script_flag_and_the_baseline_restore_turns_it_off() {
    let root = tempfile::tempdir().unwrap();
    let bin = fake_bin(root.path(), &[("tofu", FAKE_TOFU_APPLY)]);
    let log = root.path().join("tf.log");
    for flag in ["true", "false"] {
        let _ = std::fs::remove_file(&log);
        let probe = format!(
            "cd \"$WORK\" && script=probe flag={flag} && . \"{}\" && hold_versions && apply -var addon_version=1.0.1 && apply_baseline && echo done",
            live_script("lib.sh").display()
        );
        let o = Command::new("bash")
            .args(["-c", &probe])
            .env("PATH", path_with(&bin))
            .env("WORK", root.path())
            .env("HOME", root.path())
            .env("PALETTE_API_KEY", "k")
            .env("STRATA_BIN", "/bin/true")
            .env("FAKE_TF_LOG", &log)
            .output_locked()
            .unwrap();
        assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
        let calls = lines_of(&log);
        // each call starts with tf.sh's -chdir
        let applies: Vec<&str> = calls
            .iter()
            .filter_map(|c| c.split_once(' ').map(|(_, rest)| rest))
            .filter(|c| c.starts_with("apply "))
            .collect();
        // tf.sh reads one object at a time: Palette refuses a read now and then under a
        // parallel refresh.
        let with =
            "apply -parallelism=1 -input=false -auto-approve -no-color -var immutable_profiles=";
        assert_eq!(
            applies,
            [
                format!("{with}{flag}"),
                format!("{with}{flag} -var addon_version=1.0.1"),
                format!("{with}false"),
            ],
            "hold_versions and apply take the flag with no -target, read serially, and apply_baseline turns it off: {calls:?}"
        );
    }
}

// ---- upstream-report.sh ----------------------------------------------------------------------

/// A `strata` for upstream-report.sh. The call `behind ...` prints report.json and exits with
/// report.exit. Both files are under `FAKE_VERIFY`. The call also keeps the file that `--since`
/// names as since-used.json. This `strata` logs every call.
const FAKE_BEHIND_STRATA: &str = r#"#!/usr/bin/env bash
echo "$*" >> "$FAKE_VERIFY/strata.log"
[ "$1" = behind ] || exit 9
prev=""
for a in "$@"; do
  [ "$prev" = "--since" ] && cp "$a" "$FAKE_VERIFY/since-used.json"
  prev="$a"
done
cat "$FAKE_VERIFY/report.json"
exit "$(cat "$FAKE_VERIFY/report.exit")"
"#;

impl Report {
    /// upstream-report.sh, with the strata that answers `behind`.
    fn upstream() -> Report {
        Report::new("ci/upstream-report.sh", FAKE_BEHIND_STRATA)
    }

    /// The report that strata prints for upstream-report.sh: csi-aws-ebs pinned at 1.63.1 with
    /// `newer`, and kubernetes current.
    fn report(&self, exit: i32, newer: &[&str], arrived: Option<&[&str]>) {
        let mut csi = serde_json::json!({
            "name": "csi-aws-ebs", "layer": "csi", "registry": "Public Repo",
            "registryUid": "reg-public", "version": "1.63.1", "tag": "1.63.1",
            "pinnedBy": [{"path": "releases/dev.json", "subject": "aws-stack@1.1.0"}],
            "newer": newer.iter().map(|v| serde_json::json!({"version": v})).collect::<Vec<_>>(),
            "behind": !newer.is_empty(),
        });
        if let Some(first) = newer.first() {
            csi["bump"] = serde_json::json!({"to": first, "impact": "cluster-wide", "flags": ["IDENTITY", "AUTHORED"]});
        }
        if let Some(a) = arrived {
            csi["arrived"] = serde_json::json!(a);
        }
        let k8s = serde_json::json!({
            "name": "kubernetes", "layer": "k8s", "registry": "Public Repo",
            "registryUid": "reg-public", "version": "1.36.2", "tag": "1.36.2",
            "pinnedBy": [{"path": "releases/dev.json", "subject": "aws-stack@1.1.0"}],
            "newer": [], "behind": false,
        });
        self.put(
            "report.json",
            &serde_json::json!({"schema": "strata-behind/1", "tool": {"name": "strata", "version": "0.11.0"},
                "generatedAt": "2026-09-26T06:00:00Z", "records": ["releases/dev.json"],
                "pins": [csi, k8s], "summary": {"pins": 2, "behind": 1, "floating": 0, "errors": 0, "arrived": 0}})
            .to_string(),
        );
        self.put("report.exit", &exit.to_string());
    }
}

#[test]
fn upstream_report_opens_one_issue_per_pack_behind_names_what_arrived_and_closes_it_when_current() {
    let u = Report::upstream();
    // first run: csi-aws-ebs is behind, kubernetes is current, and no issue is open
    u.report(1, &["1.65.0"], None);
    let o = u.run(&["releases/*.json", "--repo", "o/r"]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    assert_eq!(
        u.log("strata.log"),
        ["behind --quiet --format json releases/*.json"],
        "no issue holds a previous run yet"
    );
    let calls = u.log("gh.log");
    assert_eq!(calls.len(), 4, "{calls:#?}");
    assert!(calls[0].starts_with("issue list --repo o/r --label strata-behind --state open"));
    assert!(
        calls[2].starts_with("label create strata-behind --repo o/r"),
        "{}",
        calls[2]
    );
    assert!(
        calls[3].starts_with(
            "issue create --repo o/r --title strata behind: csi-aws-ebs 1.63.1 behind 1.65.0 in Public Repo"
        ),
        "{}",
        calls[3]
    );
    assert!(
        !calls[3].contains("--milestone"),
        "the shipped script sets no milestone: {}",
        calls[3]
    );
    let body = u.handed("body", 4);
    let lines: Vec<&str> = body.lines().collect();
    assert_eq!(lines[0], "<!-- strata-behind: csi-aws-ebs reg-public -->");
    assert!(
        lines[1].starts_with(
            "<!-- strata-behind-state: {\"generatedAt\":\"2026-09-26T06:00:00Z\",\"pins\":[{"
        ),
        "{}",
        lines[1]
    );
    assert!(
        body.contains("| `1.63.1` | 1.65.0 | 1.65.0: cluster-wide IDENTITY AUTHORED | - | `releases/dev.json` (aws-stack@1.1.0) |"),
        "{body}"
    );
    assert!(
        body.contains("this run is the first to find it behind"),
        "{body}"
    );
    assert!(
        text(&o.stderr).contains("2 packs: 1 behind, 1 current, 0 could not be checked; opened #41 for csi-aws-ebs in Public Repo"),
        "{}",
        text(&o.stderr)
    );

    // second run: the issue holds the first run, which goes to --since; 1.66.0 arrived since
    let issue = serde_json::json!([{"number": 41, "title": "t", "body": body}]);
    u.put("issues.json", &issue.to_string());
    u.report(1, &["1.66.0", "1.65.0"], Some(&["1.66.0"]));
    let o = u.run(&["releases/*.json", "--repo", "o/r"]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    let strata_call = &u.log("strata.log")[0];
    assert!(strata_call.contains("--since "), "{strata_call}");
    let previous = read_json(&u.dir.path().join("since-used.json"));
    assert_eq!(previous["schema"], "strata-behind/1");
    assert_eq!(previous["generatedAt"], "2026-09-26T06:00:00Z");
    assert_eq!(previous["pins"][0]["newer"][0]["version"], "1.65.0");
    let calls = u.log("gh.log");
    assert_eq!(calls.len(), 2, "{calls:#?}");
    assert!(
        calls[1].starts_with("issue edit 41 --repo o/r --title strata behind: csi-aws-ebs 1.63.1 behind 1.66.0 in Public Repo"),
        "{}",
        calls[1]
    );
    assert!(
        u.handed("body", 2)
            .contains("| 1.66.0, 1.65.0 | 1.66.0: cluster-wide IDENTITY AUTHORED | 1.66.0 |"),
        "{}",
        u.handed("body", 2)
    );
    assert!(
        u.handed("body", 2)
            .contains("**arrived** names the versions that arrived since")
    );

    // third run: the pin is current, so the issue closes
    u.report(0, &[], None);
    let o = u.run(&["releases/*.json", "--repo", "o/r"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let calls = u.log("gh.log");
    assert!(
        calls[1].starts_with("issue close 41 --repo o/r --comment"),
        "{calls:#?}"
    );
    assert!(u.handed("comment", 2).contains(
        "Each pin of `csi-aws-ebs` in Public Repo is at the newest version it can move to"
    ));
}

#[test]
fn upstream_report_writes_the_answer_of_palette_to_the_bump_as_behind_writes_it() {
    let u = Report::upstream();
    u.report(1, &["1.65.0"], None);
    let error = |pack: &str, code: &str, message: &str, clusters: &[&str]| serde_json::json!({"pack": pack, "code": code, "message": message, "clusters": clusters});
    let refuses = |errors: Vec<serde_json::Value>| {
        Some(serde_json::json!({"accepts": false, "errors": errors}))
    };
    let upgrade =
        |clusters: &[&str]| error("csi-aws-ebs", UPGRADE_NOT_SUPPORTED_CODE, "m", clusters);
    // the answer of Palette in the bump, and the message of a request that failed
    let cases = [
        (Some(serde_json::json!({"accepts": true})), None),
        (refuses(vec![upgrade(&["fred"])]), None),
        (refuses(vec![upgrade(&["fred", "wilma"])]), None),
        (refuses(vec![upgrade(&["betty", "fred", "wilma"])]), None),
        // an error that names no cluster gives its pack and its message, or only its message
        (
            refuses(vec![error(
                "csi-aws-ebs",
                "DisabledPacksForbidden",
                "m",
                &[],
            )]),
            None,
        ),
        (refuses(vec![error("", "NameInvalid", "m", &[])]), None),
        // one line for each error, and a message with the mark of a column
        (
            refuses(vec![
                upgrade(&["fred", "wilma"]),
                error("csi-aws-ebs", "DisabledPacksForbidden", "a | b", &[]),
            ]),
            None,
        ),
        // a request that failed, with no answer or with a refusal that stands
        (None, Some("HTTP 403: no access")),
        (
            refuses(vec![upgrade(&["fred"])]),
            Some("1 record of project prod was not asked"),
        ),
    ];
    for (validation, failed) in cases {
        let mut report = read_json(&u.dir.path().join("report.json"));
        if let Some(validation) = validation {
            report["pins"][0]["bump"]["paletteValidation"] = validation;
        }
        if let Some(failed) = failed {
            report["pins"][0]["bump"]["paletteValidationError"] = serde_json::json!(failed);
        }
        u.put("report.json", &report.to_string());

        // the cell is the bump line of `behind --format markdown` without its "bump to", and then
        // the same lines under it
        let behind: strata::ops::BehindReport = serde_json::from_value(report).unwrap();
        let md = common::markdown().render_behind(&behind).unwrap();
        let row = md
            .lines()
            .find(|l| l.starts_with("| `csi-aws-ebs`"))
            .unwrap();
        let cell = row
            .split(" | ")
            .nth(4)
            .unwrap()
            .strip_prefix("bump to ")
            .unwrap();
        let o = u.run(&["releases/*.json", "--dry-run"]);
        assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
        let body = text(&o.stdout);
        assert!(
            body.contains(&format!("| 1.65.0 | {cell} | - |")),
            "{cell}\n{body}"
        );
    }

    // the cell, word for word
    let mut report = read_json(&u.dir.path().join("report.json"));
    report["pins"][0]["bump"]["paletteValidation"] =
        refuses(vec![upgrade(&["betty", "fred", "wilma"])]).unwrap();
    report["pins"][0]["bump"]["paletteValidationError"] = serde_json::json!("m");
    u.put("report.json", &report.to_string());
    let o = u.run(&["releases/*.json", "--dry-run"]);
    assert!(
        text(&o.stdout).contains(&format!(
            "| 1.65.0 | 1.65.0: cluster-wide IDENTITY AUTHORED<br>palette refuses: \
             {UPGRADE_NOT_SUPPORTED_CODE} on betty, fred, and wilma<br>palette gave no answer: m \
             | - |"
        )),
        "{}",
        text(&o.stdout)
    );

    // a report with no answer of Palette, as --no-impact writes it, keeps the cell as it was
    let mut report = read_json(&u.dir.path().join("report.json"));
    let bump = report["pins"][0]["bump"].as_object_mut().unwrap();
    bump.remove("paletteValidation");
    bump.remove("paletteValidationError");
    u.put("report.json", &report.to_string());
    let o = u.run(&["releases/*.json", "--dry-run"]);
    assert!(
        text(&o.stdout).contains("| 1.65.0 | 1.65.0: cluster-wide IDENTITY AUTHORED | - |"),
        "{}",
        text(&o.stdout)
    );
}

#[test]
fn upstream_report_dry_run_prints_each_issue_and_calls_no_host() {
    let u = Report::upstream();
    u.report(1, &["1.65.0"], None);
    let o = u.run(&["releases/*.json", "--dry-run", "--no-impact"]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    assert!(u.log("gh.log").is_empty(), "no host call");
    assert_eq!(
        u.log("strata.log"),
        ["behind --quiet --format json --no-impact releases/*.json"]
    );
    assert!(text(&o.stdout).starts_with("<!-- strata-behind: csi-aws-ebs reg-public -->"));
    assert!(
        text(&o.stderr).contains("would open or update the issue for csi-aws-ebs in Public Repo")
    );
    assert!(
        text(&o.stderr)
            .contains("would close the open issue for kubernetes in Public Repo, if any")
    );
}

/// A dry run calls no host, so it needs no token. The three scripts that have a host agree on
/// this on GitLab too, where the token is the only credential.
#[test]
fn a_dry_run_on_gitlab_needs_no_token_in_each_script_that_has_a_host() {
    let no_token = [("GITLAB_TOKEN", "")];
    let not_asked = |o: &Output| {
        assert!(
            !text(&o.stderr).contains("set GITLAB_TOKEN"),
            "{}",
            text(&o.stderr)
        );
    };

    let h = host();
    let o = h.run_gitlab(&[&h.a, &h.b, "--dry-run"], &no_token);
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    not_asked(&o);
    assert!(text(&o.stdout).starts_with("<!-- strata -->\n"));
    assert!(h.gl_calls().is_empty(), "a dry run calls no host");

    let report = preflight_report(&h);
    let o = h.run_gitlab(&["--preflight", &report, "--dry-run"], &no_token);
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    assert!(text(&o.stdout).starts_with("<!-- strata-preflight -->\n"));
    assert!(h.gl_calls().is_empty(), "a dry run calls no host");

    for report in [Report::verify(), Report::upstream()] {
        report.summary(
            0,
            &[("releases/a.json", "unchanged", "lab-profile", "1.2.0")],
        );
        report.report(0, &[], None);
        let dir = report.dir.path().display().to_string();
        let mut job = gitlab_job(&dir);
        job.extend_from_slice(&no_token);
        let o = report.run_with(&["releases/*.json", "--dry-run"], &job);
        assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
        not_asked(&o);
        assert!(report.log("curl.log").is_empty(), "a dry run calls no host");
    }

    // the same job without the option writes to the host, and so it asks for the token
    let o = h.run_gitlab(&[&h.a, &h.b], &no_token);
    assert_eq!(o.status.code(), Some(2));
    assert!(text(&o.stderr).contains("set GITLAB_TOKEN"));
}

/// The host layer of `verify-report.sh` is the host layer of `upstream-report.sh`, line for line.
/// Each script stays one download that stands alone, so the copy is on purpose. A change to one
/// copy must reach the other. Only the label, its description, the variables of the options, the
/// options, and the line of usage differ.
#[test]
fn the_host_layer_of_the_two_report_scripts_differs_only_in_the_lines_of_their_options() {
    const OWN: [&str; 11] = [
        "label=",
        "label_description=",
        "allow=(",
        "who=(",
        "impact=(",
        "save=\"\"",
        "--allow-no-digest)",
        "--who)",
        "--no-impact)",
        "--save)",
        "[ ${#patterns[@]} -gt 0 ]",
    ];
    // from the first function of the layer to the line where the check starts
    let layer = |name: &str| -> Vec<String> {
        let text = repo_file(&format!("scripts/ci/{name}"));
        text.lines()
            .skip_while(|l| !l.starts_with("die() {"))
            .take_while(|l| !l.starts_with("strata=${STRATA_BIN"))
            .filter(|l| !OWN.iter().any(|own| l.trim_start().starts_with(own)))
            .map(String::from)
            .collect()
    };
    let verify = layer("verify-report.sh");
    let upstream = layer("upstream-report.sh");
    assert!(
        verify.len() > 80 && verify.iter().any(|l| l.starts_with("gitlab_api()")),
        "the layer is not found: {} lines",
        verify.len()
    );
    for (i, (v, u)) in verify.iter().zip(&upstream).enumerate() {
        assert_eq!(v, u, "the layers differ at line {} of the layer", i + 1);
    }
    assert_eq!(verify.len(), upstream.len(), "one layer is longer");
}

/// The GitLab functions and the host block of `pr-comment.sh` are those of `verify-report.sh`, line
/// for line. Like the two report scripts, `pr-comment.sh` is one download with no other file. So
/// the copy is on purpose, and a change to one copy must reach the others. Only the name of the
/// script in a message, and the object that the token writes, differ.
#[test]
fn the_gitlab_layer_of_pr_comment_is_the_gitlab_layer_of_the_report_scripts() {
    /// Where a part of a script ends.
    enum End {
        /// The line that starts the part is the whole part.
        SameLine,
        /// The first line that is this text, which ends a function.
        Line(&'static str),
        /// The line before the first empty line.
        BeforeEmpty,
    }
    let part = |name: &str, from: &str, end: &End| -> Vec<String> {
        let text = repo_file(&format!("scripts/ci/{name}"))
            .replace("pr-comment:", "$script:")
            .replace("cannot write notes", "cannot write issues");
        let lines: Vec<&str> = text.lines().skip_while(|l| !l.starts_with(from)).collect();
        let length = match end {
            End::SameLine => 1,
            End::Line(last) => lines.iter().position(|l| l == last).map_or(0, |n| n + 1),
            End::BeforeEmpty => lines.iter().position(|l| l.is_empty()).unwrap_or(0),
        };
        lines[..length].iter().map(|l| l.to_string()).collect()
    };
    let parts = [
        ("the call of the API", "gitlab_api() {", End::Line("}")),
        ("the repo", "gitlab_repo() {", End::SameLine),
        ("the dispatch", "host() {", End::SameLine),
        (
            "the host block",
            "# The CI variables name the host when nothing else does.",
            End::BeforeEmpty,
        ),
    ];
    for (what, from, end) in parts {
        let comment = part("pr-comment.sh", from, &end);
        assert!(!comment.is_empty(), "{what} is not found in pr-comment.sh");
        assert_eq!(
            comment,
            part("verify-report.sh", from, &end),
            "{what} differs in pr-comment.sh"
        );
    }
}

#[test]
fn upstream_report_keeps_the_report_of_the_run_in_the_file_that_save_names() {
    let u = Report::upstream();
    u.report(1, &["1.65.0"], None);
    let saved = u.dir.path().join("kept/behind.json");
    std::fs::create_dir(saved.parent().unwrap()).unwrap();
    let o = u.run(&[
        "releases/*.json",
        "--dry-run",
        "--save",
        saved.to_str().unwrap(),
    ]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    let kept: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&saved).expect("the report is kept"))
            .unwrap();
    assert_eq!(kept["schema"], "strata-behind/1");
    assert_eq!(kept["summary"]["behind"], 1);
}

#[test]
fn upstream_report_touches_no_issue_when_behind_checks_nothing() {
    for old_jq in [false, true] {
        for printed in PRINTED_NOTHING {
            for exit in EXITS_OF_NOTHING {
                let u = Report::upstream();
                if old_jq {
                    u.use_jq(JQ_1_6);
                }
                u.put("report.json", printed);
                u.put("report.exit", exit);
                let o = u.run(&["releases/*.json", "--repo", "o/r"]);
                let case =
                    format!("old jq: {old_jq}, strata printed {printed:?} and exited {exit}");
                assert_eq!(o.status.code(), Some(2), "{case}");
                assert!(
                    text(&o.stderr).contains(&format!(
                        "strata behind checked nothing (exit {exit}); no issue was touched"
                    )),
                    "{case}: {}",
                    text(&o.stderr)
                );
                assert_eq!(
                    u.log("gh.log").len(),
                    1,
                    "{case}: only the list of open issues"
                );
            }
        }
    }
}

#[test]
fn upstream_report_goes_on_when_an_open_issue_has_no_text_in_its_body() {
    // GitHub gives `"body": null` for an issue that a person opens with no text
    for (jq, stand_in) in jq_stand_ins() {
        let u = Report::upstream();
        if let Some(stand_in) = stand_in {
            u.use_jq(stand_in);
        }
        u.put(
            "issues.json",
            r#"[{"number":3,"title":"t","body":null},
                {"number":4,"title":"t","body":""},
                {"number":41,"title":"t","body":"<!-- strata-behind: csi-aws-ebs reg-public -->\nold"}]"#,
        );
        // the pack is behind: the issue that holds its marker is the one that changes
        u.report(1, &["1.65.0"], None);
        let o = u.run(&["releases/*.json", "--repo", "o/r"]);
        assert_eq!(o.status.code(), Some(1), "{jq}: {}", text(&o.stderr));
        let calls = u.log("gh.log");
        assert!(
            calls
                .iter()
                .any(|c| c.starts_with("issue edit 41 --repo o/r")),
            "{jq}: {calls:#?}"
        );
        // the pack is current: its issue closes
        u.report(0, &[], None);
        let o = u.run(&["releases/*.json", "--repo", "o/r"]);
        assert_eq!(o.status.code(), Some(0), "{jq}: {}", text(&o.stderr));
        let calls = u.log("gh.log");
        assert!(
            calls
                .iter()
                .any(|c| c.starts_with("issue close 41 --repo o/r")),
            "{jq}: {calls:#?}"
        );
        assert!(
            calls.iter().all(|c| !c.starts_with("issue close 3 ")
                && !c.starts_with("issue close 4 ")
                && !c.starts_with("issue edit 3 ")
                && !c.starts_with("issue edit 4 ")),
            "{jq}: an issue with no text is not touched: {calls:#?}"
        );
    }
}

// ---- preflight.sh ------------------------------------------------------------------------------

/// A job directory with the stand-in `strata` and `gh`, and a record that a pattern could match.
struct PreflightJob {
    dir: tempfile::TempDir,
    bin: PathBuf,
}

fn preflight_job() -> PreflightJob {
    let dir = tempfile::tempdir().unwrap();
    let bin = fake_bin(dir.path(), &[("strata", LOGGING_TOOL), ("gh", FAKE_GH)]);
    // a shell without globstar reads `releases/**/*.json` as `releases/*/*.json`, and finds this
    std::fs::create_dir_all(dir.path().join("releases/prod")).unwrap();
    std::fs::write(dir.path().join("releases/prod/eks.json"), "{}").unwrap();
    PreflightJob { dir, bin }
}

impl PreflightJob {
    /// Run `preflight.sh` with the variables `envs` and the arguments `args`, and no key. A runner
    /// sets `GITHUB_REPOSITORY`, which `pr-comment.sh` reads: the stand-in `gh` must give the repo.
    fn run(&self, args: &[&str], envs: &[(&str, &str)]) -> Output {
        let d = self.dir.path();
        std::fs::write(d.join("strata.log"), "").unwrap();
        std::fs::write(d.join("gh.log"), "").unwrap();
        let mut c = Command::new("bash");
        c.arg(script("ci/preflight.sh"))
            .args(args)
            .current_dir(d)
            .env("PATH", path_with(&self.bin))
            .env("STRATA_BIN", self.bin.join("strata"))
            .env("FAKE_LOG", d.join("strata.log"))
            .env("FAKE_GH_LOG", d.join("gh.log"))
            .env("FAKE_GH_SAVED_BODY", d.join("saved-body.md"))
            .env_remove("GITHUB_REPOSITORY")
            .env_remove("PALETTE_API_KEY");
        for (k, v) in envs {
            c.env(k, v);
        }
        c.output_locked().unwrap()
    }

    /// The calls of the stand-in `strata`, one for each line.
    fn strata_calls(&self) -> Vec<String> {
        lines_of(&self.dir.path().join("strata.log"))
    }

    fn gh_calls(&self) -> Vec<String> {
        lines_of(&self.dir.path().join("gh.log"))
    }
}

#[test]
fn preflight_sh_builds_the_list_of_arguments_from_the_variables() {
    let job = preflight_job();
    let o = job.run(
        &[],
        &[
            ("STRATA_PREFLIGHT_SOURCE", "cluster:64f0"),
            ("STRATA_PREFLIGHT_CHANGE", "change.json"),
            ("STRATA_PREFLIGHT_CLOUD", "eks"),
            ("STRATA_PREFLIGHT_TIER", "static,palette"),
            ("STRATA_PREFLIGHT_PROBES", "probes.yaml"),
            ("STRATA_PREFLIGHT_RELEASE", "4.10"),
            ("STRATA_PREFLIGHT_READY", "80"),
            ("STRATA_PREFLIGHT_EVERY", "5"),
            ("STRATA_PREFLIGHT_LIMIT", "60"),
            ("STRATA_PREFLIGHT_STRICT", "true"),
            ("STRATA_PREFLIGHT_NO_EXEC", "true"),
            ("STRATA_PREFLIGHT_WATCH", "true"),
            ("STRATA_PREFLIGHT_RULE", "eks-subnets  eks-roles"),
            (
                "STRATA_PREFLIGHT_RECORDS",
                "releases/**/*.json other/*.json",
            ),
            ("STRATA_PREFLIGHT_OUT", "out"),
            ("PALETTE_API_KEY", "a-key"),
            ("FAKE_OUT", PREFLIGHT_REPORT),
        ],
    );
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert_eq!(
        job.strata_calls(),
        [
            "strata preflight cluster:64f0 change.json --cloud eks --tier static,palette \
             --probes probes.yaml --release 4.10 --ready 80 --every 5 --limit 60 --strict \
             --no-exec --watch --quiet --rule eks-subnets --rule eks-roles \
             --records releases/**/*.json --records other/*.json --out out --format markdown"
        ]
    );
    assert_eq!(text(&o.stdout), PREFLIGHT_REPORT);
}

/// Strata expands a record pattern. A shell that expands it first reads `**` as `*`, and drops the
/// records that only `**` finds.
#[test]
fn preflight_sh_passes_each_record_pattern_as_typed_and_expands_none() {
    let job = preflight_job();
    let o = job.run(
        &[],
        &[
            ("STRATA_PREFLIGHT_SOURCE", "clusters/payments.json"),
            (
                "STRATA_PREFLIGHT_RECORDS",
                "releases/**/*.json releases/*/*.json",
            ),
        ],
    );
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert_eq!(
        job.strata_calls(),
        [
            "strata preflight clusters/payments.json --quiet --offline --records releases/**/*.json \
          --records releases/*/*.json --out preflight --format markdown"
        ]
    );
}

#[test]
fn preflight_sh_gives_no_option_for_an_empty_variable_and_a_value_other_than_true() {
    let job = preflight_job();
    let o = job.run(
        &[],
        &[
            ("STRATA_PREFLIGHT_SOURCE", "clusters/payments.json"),
            ("STRATA_PREFLIGHT_CLOUD", ""),
            ("STRATA_PREFLIGHT_STRICT", "false"),
            ("STRATA_PREFLIGHT_VERBOSE", "false"),
            ("PALETTE_API_KEY", "a-key"),
        ],
    );
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert_eq!(
        job.strata_calls(),
        ["strata preflight clusters/payments.json --quiet --out preflight --format markdown"]
    );
}

#[test]
fn preflight_sh_reads_nothing_from_palette_without_a_key_or_with_the_option_offline() {
    let job = preflight_job();
    let source = ("STRATA_PREFLIGHT_SOURCE", "clusters/payments.json");
    for (args, envs, offline) in [
        (vec![], vec![source], true),
        (vec![], vec![source, ("PALETTE_API_KEY", "a-key")], false),
        (
            vec!["--offline"],
            vec![source, ("PALETTE_API_KEY", "a-key")],
            true,
        ),
    ] {
        let o = job.run(&args, &envs);
        assert!(o.status.success(), "{}", text(&o.stderr));
        let calls = job.strata_calls();
        assert_eq!(
            calls[0].contains(" --offline "),
            offline,
            "{args:?} {envs:?}: {calls:?}"
        );
    }
}

#[test]
fn preflight_sh_omits_quiet_for_a_verbose_run() {
    let job = preflight_job();
    let o = job.run(
        &[],
        &[
            ("STRATA_PREFLIGHT_SOURCE", "clusters/payments.json"),
            ("STRATA_PREFLIGHT_VERBOSE", "true"),
        ],
    );
    assert!(o.status.success(), "{}", text(&o.stderr));
    let call = &job.strata_calls()[0];
    assert!(call.contains(" --verbose "), "{call}");
    assert!(!call.contains("--quiet"), "{call}");
}

#[test]
fn preflight_sh_exits_with_the_status_of_strata_and_prints_the_report_of_a_failed_run() {
    let job = preflight_job();
    for code in ["1", "2"] {
        let o = job.run(
            &[],
            &[
                ("STRATA_PREFLIGHT_SOURCE", "clusters/payments.json"),
                ("FAKE_EXIT", code),
                ("FAKE_OUT", PREFLIGHT_REPORT),
            ],
        );
        assert_eq!(o.status.code(), code.parse::<i32>().ok());
        assert_eq!(text(&o.stdout), PREFLIGHT_REPORT);
    }
}

#[test]
fn preflight_sh_stops_with_exit_2_before_strata_runs_when_it_cannot_run() {
    let job = preflight_job();
    let source = ("STRATA_PREFLIGHT_SOURCE", "clusters/payments.json");
    for (args, envs, said) in [
        (vec![], vec![], "STRATA_PREFLIGHT_SOURCE"),
        (
            vec![],
            vec![("STRATA_PREFLIGHT_SOURCE", "")],
            "STRATA_PREFLIGHT_SOURCE",
        ),
        (vec!["--nope"], vec![source], "unknown option --nope"),
        (
            vec![],
            vec![source, ("STRATA_BIN", "/nonexistent/strata")],
            "no strata at /nonexistent/strata",
        ),
    ] {
        let o = job.run(&args, &envs);
        assert_eq!(o.status.code(), Some(2), "{args:?} {envs:?}");
        assert!(text(&o.stderr).contains(said), "{}", text(&o.stderr));
        assert!(job.strata_calls().is_empty(), "{:?}", job.strata_calls());
    }
}

#[test]
fn preflight_sh_posts_the_report_also_after_a_failed_preflight_when_the_comment_is_on() {
    let job = preflight_job();
    let source = ("STRATA_PREFLIGHT_SOURCE", "clusters/payments.json");
    let o = job.run(
        &[],
        &[
            source,
            ("STRATA_PREFLIGHT_COMMENT", "true"),
            ("FAKE_EXIT", "1"),
            ("FAKE_OUT", PREFLIGHT_REPORT),
        ],
    );
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    let calls = job.gh_calls();
    assert!(
        calls
            .iter()
            .any(|c| c.starts_with("pr comment 7 --repo o/r --body-file ")),
        "{calls:?}"
    );
    assert_eq!(
        std::fs::read_to_string(job.dir.path().join("saved-body.md")).unwrap(),
        format!("<!-- strata-preflight -->\n{PREFLIGHT_REPORT}"),
        "the comment is the report of --out"
    );

    // a comment is off unless the variable is true
    for value in ["", "false"] {
        let o = job.run(
            &[],
            &[
                source,
                ("STRATA_PREFLIGHT_COMMENT", value),
                ("FAKE_OUT", PREFLIGHT_REPORT),
            ],
        );
        assert!(o.status.success(), "{}", text(&o.stderr));
        assert!(job.gh_calls().is_empty(), "{:?}", job.gh_calls());
    }
}

#[test]
fn preflight_sh_turns_a_failed_comment_into_exit_2_and_keeps_a_failed_status_of_strata() {
    let job = preflight_job();
    // strata wrote no report, so pr-comment.sh stops: the comment of the last push stays
    for (strata_exit, exit) in [("0", 2), ("1", 1)] {
        let o = job.run(
            &[],
            &[
                ("STRATA_PREFLIGHT_SOURCE", "clusters/payments.json"),
                ("STRATA_PREFLIGHT_COMMENT", "true"),
                ("FAKE_EXIT", strata_exit),
            ],
        );
        assert_eq!(o.status.code(), Some(exit), "{}", text(&o.stderr));
        assert!(
            text(&o.stderr).contains("missing or empty"),
            "{}",
            text(&o.stderr)
        );
    }
}
