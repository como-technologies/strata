//! The runner: it runs each strata command that the book shows and writes its output into
//! `docs/src/output/`.
//!
//! Each step gives the exit code that the book states, and a different one stops the run. Each
//! time stamp and each duration becomes one fixed value, so an output file changes only when
//! strata changes.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use regex::Regex;

use crate::checks::OutputState;
use crate::files::files_with_ext;
use crate::tenant::{Edit, PROJECT_LISTING, TENANT, Tenant};

/// What the run is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Write `docs/src/output/`.
    Write,
    /// Fail when `docs/src/output/` is not the output of this build.
    Check,
    /// Build one chapter's tenant, serve it, and wait. The number is which of its states.
    Serve {
        /// The chapter to serve.
        chapter: String,
        /// Which of its states: `CHAPTER-2` serves the state after its second change.
        seed: u32,
    },
}

/// Split the name that `just playground NAME` takes into its chapter and its state.
///
/// `NAME` is `CHAPTER` or `CHAPTER-N`, with one digit in `N`. `CHAPTER` alone asks for state 1.
pub fn playground(name: &str) -> (&str, u32) {
    if let Some((chapter, tail)) = name.rsplit_once('-')
        && tail.len() == 1
        && let Ok(seed) = tail.parse()
    {
        return (chapter, seed);
    }
    (name, 1)
}

/// One run of the book's commands.
pub struct Runner {
    /// The tenant the commands talk to.
    tenant: Tenant,
    mode: Mode,
    repo: PathBuf,
    work: PathBuf,
    out: PathBuf,
    strata: PathBuf,
    port: u16,
    server: Child,
    /// Closing this stops the server, whatever ends this run.
    _hold: Option<std::process::ChildStdin>,
    /// The chapter now building, and the directory its commands run in.
    chapter: String,
    dir: PathBuf,
    env: Vec<(String, String)>,
    skip: bool,
    served: bool,
    /// How many states of the served chapter have been reached.
    seed: u32,
    /// The seed that the output of the next step belongs to, as `serves_seen + mutated`.
    serves_seen: u32,
    mutated: u32,
    /// Which seed reaches each output, for the page check. One name can be written by two steps.
    states: BTreeSet<OutputState>,
    /// The name of each chapter that the run defined, and how many states it has. The playground
    /// and the page check read it, so no second list of the chapters can disagree with the runner.
    chapters: BTreeMap<String, u32>,
    /// What each (project, uid, version) was seeded from, so two chapters cannot print two roots
    /// for one identity.
    seeded: BTreeMap<(String, String, String), (String, String)>,
    clock: Clock,
    commit: Regex,
}

impl Runner {
    /// Start the stand-in for Palette and prepare a run.
    pub fn start(repo: &Path, strata: &Path, xtask: &Path, mode: Mode) -> std::io::Result<Self> {
        let work = std::env::temp_dir().join(format!("strata-book-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&work);
        let tenant_dir = work.join(TENANT);
        let out = work.join("output");
        let bin = work.join("bin");
        std::fs::create_dir_all(&tenant_dir)?;
        std::fs::create_dir_all(&out)?;
        std::fs::create_dir_all(&bin)?;
        std::fs::create_dir_all(work.join("home"))?;
        symlink(strata, &bin.join("strata"))?;

        let port_file = work.join("port");
        // The server gets a pipe for standard input and no share of this process's output. It
        // stops when the pipe closes, so it cannot outlive this run, and it cannot hold open a
        // pipe that this run's output was written into.
        let mut server = Command::new(xtask)
            .args(["serve"])
            .arg(&tenant_dir)
            .arg(&port_file)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()?;
        let hold = server.stdin.take();
        let mut port = None;
        for _ in 0..100 {
            if let Ok(text) = std::fs::read_to_string(&port_file)
                && let Ok(p) = text.trim().parse()
            {
                port = Some(p);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let Some(port) = port else {
            return Err(std::io::Error::other(
                "the stand-in for Palette did not start",
            ));
        };

        Ok(Runner {
            tenant: Tenant::new(&tenant_dir, repo),
            mode,
            repo: repo.to_path_buf(),
            work,
            out,
            strata: bin.join("strata"),
            port,
            server,
            _hold: hold,
            chapter: String::new(),
            dir: PathBuf::new(),
            env: Vec::new(),
            skip: false,
            served: false,
            seed: 0,
            serves_seen: 0,
            mutated: 1,
            states: BTreeSet::new(),
            chapters: BTreeMap::new(),
            seeded: BTreeMap::new(),
            clock: Clock::new(),
            commit: Regex::new(r"(?m) in [0-9a-f]{7,40}$").expect("a pattern"),
        })
    }

    fn serving(&self) -> bool {
        matches!(self.mode, Mode::Serve { .. })
    }

    /// True when this run builds the chapter: a served run builds one chapter only.
    fn building(&self) -> bool {
        !self.skip
    }

    /// Start a chapter: an empty tenant, a new working directory, and the environment of the
    /// commands that follow.
    pub fn chapter(&mut self, name: &str, env: &[(&str, &str)]) {
        // a chapter starts with one state: the end of the chapter
        if self.chapters.insert(name.to_string(), 1).is_some() {
            die(&format!("two chapters have the name '{name}'"));
        }
        if let Mode::Serve { chapter, .. } = self.mode.clone() {
            if self.served {
                self.serve_last();
            }
            self.skip = name != chapter;
            if !self.skip {
                self.served = true;
            }
        }
        self.chapter = name.to_string();
        self.serves_seen = 0;
        self.mutated = 1;
        self.dir = self.work.join("chapters").join(name);
        std::fs::create_dir_all(&self.dir).expect("a chapter directory");
        let _ = symlink(&self.repo.join("examples"), &self.dir.join("examples"));
        let _ = symlink(&self.repo.join("scripts"), &self.dir.join("scripts"));
        self.env = env
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        self.tenant.clear();
    }

    /// A tenant helper ran, so the output of the next step needs a later seed.
    pub fn changed(&mut self) {
        self.mutated = 1;
    }

    /// Run the tenant helper `f` when this run builds the chapter, and mark the change.
    ///
    /// A chapter changes the tenant through it, with the methods of [`Tenant`]: a profile, a
    /// cluster, an edit in place. A served run that does not build this chapter does nothing.
    pub fn tenant(&mut self, f: impl FnOnce(&Tenant)) {
        if !self.building() {
            return;
        }
        self.changed();
        f(&self.tenant);
    }

    /// Publish a copy of an example profile.
    ///
    /// Each `<name>@<version>` under one uid must be seeded from one content, in every chapter.
    /// The tag-name chapter once gave uid `...d4` version 1.1.2 the tag change, and the compare
    /// chapter gave the same uid and version a changed value, so two pages printed two roots for
    /// one identity. A chapter that needs its own content says so with `profile_own_copy`.
    pub fn profile(
        &mut self,
        project: &str,
        uid: &str,
        version: &str,
        example: &str,
        edits: &[Edit],
    ) {
        self.seed_once(project, uid, version, example, edits);
        self.tenant(|t| t.profile(project, uid, version, example, edits));
    }

    /// Publish a copy of an example profile at tenant scope. Palette lists a profile of the
    /// tenant in each project, beside the profiles of the project, and serves its document
    /// there. The rule of [`Runner::profile`] on one content for one identity holds for it too.
    pub fn tenant_profile(&mut self, uid: &str, version: &str, example: &str, edits: &[Edit]) {
        self.seed_once(TENANT, uid, version, example, edits);
        self.tenant(|t| t.tenant_profile(uid, version, example, edits));
    }

    /// Stop when another chapter seeds `project` `uid` `version` from another content.
    fn seed_once(
        &mut self,
        project: &str,
        uid: &str,
        version: &str,
        example: &str,
        edits: &[Edit],
    ) {
        let key = (project.to_string(), uid.to_string(), version.to_string());
        let content = format!("{example} {edits:?}");
        match self.seeded.get(&key) {
            Some((first, chapter)) if *first != content => die(&format!(
                "the chapter '{}' seeds {project} {uid} {version} from '{content}', and the \
                 chapter '{chapter}' seeds it from '{first}'. Two chapters that print a root for \
                 one identity must print one root. Give this chapter its own version and uid, or \
                 call profile_own_copy with the reason.",
                self.chapter
            )),
            Some(_) => {}
            None => {
                self.seeded.insert(key, (content, self.chapter.clone()));
            }
        }
    }

    /// Publish a copy that differs from the one another chapter gives the same identity, on
    /// purpose. `why` says why, for the reader of the chapter.
    pub fn profile_own_copy(
        &mut self,
        project: &str,
        uid: &str,
        version: &str,
        example: &str,
        edits: &[Edit],
        _why: &str,
    ) {
        self.tenant(|t| t.profile(project, uid, version, example, edits));
    }

    /// Create a profile version with no publish: see [`Tenant::unpublish`].
    ///
    /// The rule of [`Runner::profile`] on one content for one identity holds for it.
    pub fn unpublished(
        &mut self,
        project: &str,
        uid: &str,
        version: &str,
        example: &str,
        edits: &[Edit],
    ) {
        self.profile(project, uid, version, example, edits);
        self.tenant(|t| t.unpublish(project, uid));
    }

    /// Write the answer of one request of the tenant, for a route no other helper writes.
    pub fn answer(&mut self, project: &str, path: &str, value: &serde_json::Value) {
        if self.serving() {
            return;
        }
        self.changed();
        self.tenant.put(project, path, value);
    }

    /// Write a file into the chapter's working directory, as a candidate file a page shows.
    pub fn write_file(&mut self, name: &str, text: &str) {
        if self.serving() {
            return;
        }
        std::fs::write(self.dir.join(name), text).expect("a chapter file");
    }

    fn state(&self) -> u32 {
        self.serves_seen + self.mutated
    }

    /// Note that the output `name` is the output of the state that the chapter has now.
    fn note_state(&mut self, name: &str) {
        self.states.insert(OutputState {
            output: name.to_string(),
            chapter: self.chapter.clone(),
            state: self.state(),
        });
    }

    /// The playground hands the tenant over at this line and builds no more of the chapter.
    pub fn serve_here(&mut self) {
        self.serves_seen += 1;
        self.mutated = 0;
        if let Some(states) = self.chapters.get_mut(&self.chapter) {
            *states = self.serves_seen + 1;
        }
        if !self.serving() || !self.served {
            return;
        }
        self.seed += 1;
        if let Mode::Serve { seed, .. } = self.mode
            && self.seed == seed
        {
            self.serve_wait();
        }
    }

    /// The end of the served chapter is its last state. A seed past the end is an error that says
    /// how many the chapter has, so a wrong number never serves the wrong tenant.
    fn serve_last(&mut self) -> ! {
        self.seed += 1;
        if let Mode::Serve { chapter, seed } = self.mode.clone() {
            if self.seed == seed {
                self.serve_wait();
            }
            die(&format!(
                "the chapter '{chapter}' has {} states; '{chapter}-{seed}' asks for state {seed}",
                self.seed
            ));
        }
        unreachable!("serve_last runs in a served run")
    }

    /// Run one command and add `$ COMMAND` and its output to `name`. `stderr` keeps standard
    /// error too, as `show2` does.
    pub fn step(&mut self, name: &str, want: i32, stderr: bool, command: &str) {
        if self.serving() {
            return;
        }
        let (got, kept, rest) = self.run(command, stderr);
        let mut text = format!("$ {command}\n");
        text.push_str(&kept);
        if got != want {
            eprintln!("{text}{rest}");
            die(&format!(
                "{name}: '{command}' exited with {got}, and the book says {want}"
            ));
        }
        let text = self.clock.fixed(&text);
        let text = self
            .commit
            .replace_all(&text, " in 8c1d4f2")
            .replace("\r\n", "\n");
        let file = self.out.join(format!("{name}.txt"));
        let mut existing = std::fs::read_to_string(&file).unwrap_or_default();
        existing.push_str(&text);
        std::fs::write(&file, existing).expect("an output file");
        self.note_state(name);
    }

    /// A step the book does not show. It must pass.
    pub fn quiet(&mut self, command: &str) {
        if self.serving() {
            return;
        }
        let (got, kept, _) = self.run(command, true);
        if got != 0 {
            eprintln!("{kept}");
            die(&format!("the command '{command}' failed"));
        }
    }

    /// Run one command with bash, and give back its exit code, what the book keeps, and what to
    /// print when the exit code is wrong.
    ///
    /// `join` is `show2`: standard output and standard error go to one file, so the block holds
    /// them interleaved as a terminal shows them. Two pipes read separately would put every
    /// progress note after the result, which is not what the reader sees.
    fn run(&self, command: &str, join: bool) -> (i32, String, String) {
        let out_file = self.work.join("step.out");
        let err_file = self.work.join("step.err");
        let out = std::fs::File::create(&out_file).expect("a step file");
        let err = if join {
            out.try_clone().expect("a second handle on the step file")
        } else {
            std::fs::File::create(&err_file).expect("a step file")
        };
        let mut c = Command::new("bash");
        c.arg("-o").arg("pipefail").arg("-c").arg(command);
        c.current_dir(&self.dir);
        c.env_clear();
        c.env(
            "PATH",
            format!(
                "{}:{}",
                self.work.join("bin").display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        );
        c.env("HOME", self.work.join("home"));
        c.env("LANG", "C.UTF-8");
        c.env("PALETTE_API", format!("http://127.0.0.1:{}", self.port));
        c.env("PALETTE_API_KEY", "docs-example-key");
        c.env("STRATA_BIN", &self.strata);
        for (k, v) in &self.env {
            c.env(k, v);
        }
        c.stdout(Stdio::from(out));
        c.stderr(Stdio::from(err));
        let status = c.status().expect("bash runs");
        let kept = std::fs::read_to_string(&out_file).unwrap_or_default();
        let rest = if join {
            String::new()
        } else {
            std::fs::read_to_string(&err_file).unwrap_or_default()
        };
        (status.code().unwrap_or(-1), kept, rest)
    }

    /// The lines of `from` that a page teaches, written as `name`. The command line is kept, and
    /// each gap becomes `...`. Nothing matching is an error: when the line a page is built on
    /// stops being printed, this build fails instead of the page going quietly wrong.
    pub fn excerpt(&mut self, name: &str, from: &str, pattern: &str) {
        if self.serving() {
            return;
        }
        let source = self.out.join(format!("{from}.txt"));
        let text = std::fs::read_to_string(&source).unwrap_or_else(|_| {
            die(&format!(
                "excerpt {name}: there is no output named '{from}'"
            ))
        });
        let re = Regex::new(pattern).unwrap_or_else(|e| {
            die(&format!(
                "excerpt {name}: the pattern does not compile: {e}"
            ))
        });
        let mut out = String::new();
        let mut gap = false;
        let mut hits = 0;
        for (index, line) in text.lines().enumerate() {
            if index == 0 {
                out.push_str(line);
                out.push('\n');
                continue;
            }
            if re.is_match(line) {
                if gap {
                    out.push_str("...\n");
                }
                gap = false;
                out.push_str(line);
                out.push('\n');
                hits += 1;
            } else {
                gap = true;
            }
        }
        if gap {
            out.push_str("...\n");
        }
        if hits == 0 {
            die(&format!(
                "excerpt {name}: no line of {from} matches /{pattern}/; \
                 the page's claim is no longer printed"
            ));
        }
        std::fs::write(self.out.join(format!("{name}.txt")), out).expect("an excerpt");
        self.note_state(name);
    }

    /// Write `docs/src/output/`, or say what is not current, and return the exit code.
    pub fn finish(mut self) -> std::process::ExitCode {
        if let Mode::Serve { chapter, .. } = self.mode.clone() {
            if self.served {
                self.serve_last();
            }
            let names: Vec<&str> = self.chapters.keys().map(String::as_str).collect();
            die(&format!(
                "no chapter '{chapter}'. The chapters are: {}",
                names.join(" ")
            ));
        }
        let problems = crate::checks::check(&self.repo, &self.chapters, &self.states);
        if !problems.is_empty() {
            for problem in &problems {
                eprintln!("docs-examples: {problem}");
            }
            std::process::exit(1);
        }
        let dest = self.repo.join("docs/src/output");
        let files = sorted_files(&self.out);
        if self.mode == Mode::Check {
            let current = sorted_files(&dest);
            let mut same = current.len() == files.len();
            for (name, text) in &files {
                match current.get(name) {
                    Some(have) if have == text => {}
                    Some(_) => {
                        eprintln!("docs-examples: {name} is not current");
                        same = false;
                    }
                    None => {
                        eprintln!("docs-examples: {name} is new");
                        same = false;
                    }
                }
            }
            for name in current.keys() {
                if !files.contains_key(name) {
                    eprintln!("docs-examples: {name} is no longer written");
                    same = false;
                }
            }
            if !same {
                eprintln!(
                    "docs-examples: docs/src/output/ is not current. Run: just docs-examples"
                );
                return std::process::ExitCode::from(1);
            }
            println!(
                "docs-examples: docs/src/output/ is current ({} files)",
                files.len()
            );
        } else {
            let _ = std::fs::remove_dir_all(&dest);
            std::fs::create_dir_all(&dest).expect("the output directory");
            for (name, text) in &files {
                std::fs::write(dest.join(name), text).expect("an output file");
            }
            println!(
                "docs-examples: wrote {} files to docs/src/output/",
                files.len()
            );
        }
        std::process::ExitCode::SUCCESS
    }

    fn serve_wait(&mut self) -> ! {
        let project = self
            .env
            .iter()
            .find(|(k, _)| k == "PALETTE_PROJECT")
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        let listing = self.tenant.doc(TENANT, PROJECT_LISTING);
        let text = std::fs::read_to_string(&listing).unwrap_or_default();
        let doc: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::json!({}));
        let items = doc["items"].as_array().cloned().unwrap_or_default();
        let others: Vec<String> = items
            .iter()
            .map(|p| {
                format!(
                    "#   {}  {}",
                    p["metadata"]["name"].as_str().unwrap_or_default(),
                    p["metadata"]["uid"].as_str().unwrap_or_default()
                )
            })
            .collect();
        let scope = if project.is_empty() {
            // An empty value, not `unset`: strata reads ~/.palette.env when a variable is unset,
            // so `unset` would leave a real project in place. An empty one says tenant scope.
            "export PALETTE_PROJECT_UID=                  # this chapter is tenant scope\n\
             export PALETTE_PROJECT=                    # an empty value beats a ~/.palette.env"
                .to_string()
        } else {
            let uid = items
                .iter()
                .find(|p| p["metadata"]["name"] == serde_json::json!(project))
                .map(|p| {
                    p["metadata"]["uid"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string()
                })
                .unwrap_or_default();
            format!(
                "export PALETTE_PROJECT={project}              # the book names the project\n\
                 export PALETTE_PROJECT_UID={uid}            # or its uid, which has priority"
            )
        };
        let chapter = match &self.mode {
            Mode::Serve { chapter, .. } => chapter.clone(),
            _ => self.chapter.clone(),
        };
        println!(
            "\nThe tenant of the chapter \"{chapter}\" is served on this machine. Export these, \
             and the commands of\nthe book run as the book prints them:\n\n\
             export PALETTE_API=http://127.0.0.1:{}\n\
             export PALETTE_API_KEY=docs-example-key\n{scope}\n\n\
             # every project of this tenant, for a page that reads more than one:\n{}\n\n\
             They beat a ~/.palette.env that names a real tenant. When a page describes a change \
             to this\ntenant -- an edit in place, a version bump, a second publish under one name \
             -- the playground\nserves the tenant as the steps before that change see it. For \
             example:\n\n  strata list\n  strata compare aws-stack@1.0.0 aws-stack@1.1.0\n  \
             strata snapshot aws-stack@1.1.0 -o dev.json && strata verify dev.json\n  \
             strata compare dev.json cluster:payments\n  strata consumers aws-stack@1.1.0\n\n\
             Nothing here reaches Palette, and nothing costs anything. Stop with Ctrl-C.",
            self.port,
            others.join("\n")
        );
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
            if self.server.try_wait().ok().flatten().is_some() {
                std::process::exit(0);
            }
        }
    }
}

impl Drop for Runner {
    fn drop(&mut self) {
        let _ = self.server.kill();
        let _ = self.server.wait();
        let _ = std::fs::remove_dir_all(&self.work);
    }
}

fn sorted_files(dir: &Path) -> BTreeMap<String, String> {
    files_with_ext(dir, "txt")
        .into_iter()
        .filter_map(|p| {
            let name = p.file_name()?.to_string_lossy().into_owned();
            Some((name, std::fs::read_to_string(&p).ok()?))
        })
        .collect()
}

#[cfg(unix)]
fn symlink(from: &Path, to: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(from, to)
}

/// The values of an output that come from the clock: a time stamp, and the time that a step
/// took. The time of a step changes with the load of the machine, so the runner writes one fixed
/// value in its place.
struct Clock {
    stamp: Regex,
    /// The milliseconds of a preflight check, in each form that preflight prints.
    millis: Regex,
    /// The seconds of a preflight watch.
    seconds: Regex,
}

impl Clock {
    fn new() -> Clock {
        Clock {
            stamp: Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z").expect("a pattern"),
            // the column of the text table, the cell of the Markdown table, the field of the JSON
            // report, and the end of a verbose line
            millis: Regex::new(
                r#"(?P<pad> +)\d+ ms\b|\| \d+ ms \||"elapsedMs": \d+|(?P<lead>-> |, )\d+ ms,"#,
            )
            .expect("a pattern"),
            seconds: Regex::new(r"\bafter \d+s\b").expect("a pattern"),
        }
    }

    /// Replace each time stamp with `2026-09-17T09:30:00Z`, each count of milliseconds with 1,
    /// and each count of seconds with 0.
    fn fixed(&self, text: &str) -> String {
        let text = self.stamp.replace_all(text, "2026-09-17T09:30:00Z");
        let text = self.millis.replace_all(&text, |c: &regex::Captures| {
            if c.name("pad").is_some() {
                // the same width as the field had, so the columns stay where they were
                let width = c[0].len() - " ms".len();
                format!("{:>width$} ms", 1)
            } else if let Some(lead) = c.name("lead") {
                format!("{}1 ms,", lead.as_str())
            } else if c[0].starts_with('|') {
                "| 1 ms |".to_string()
            } else {
                "\"elapsedMs\": 1".to_string()
            }
        });
        self.seconds.replace_all(&text, "after 0s").into_owned()
    }
}

/// Stop the run, as the book's build must fail loudly rather than write a wrong page.
pub fn die(message: &str) -> ! {
    eprintln!("docs-examples: {message}");
    std::process::exit(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_playground_name_splits_into_its_chapter_and_its_state() {
        assert_eq!(playground("loop"), ("loop", 1));
        assert_eq!(playground("loop-3"), ("loop", 3));
        assert_eq!(playground("os-image"), ("os-image", 1));
        assert_eq!(playground("os-image-2"), ("os-image", 2));
        assert_eq!(
            playground("merge-gate-rollback"),
            ("merge-gate-rollback", 1)
        );
        assert_eq!(playground("loop-12"), ("loop-12", 1));
    }

    #[test]
    fn each_duration_that_preflight_prints_becomes_one_fixed_value() {
        let clock = Clock::new();
        let cases = [
            (
                "strata: static: bastion-cost: jq over the world -> 12 ms, NOTE: no bastion",
                "strata: static: bastion-cost: jq over the world -> 1 ms, NOTE: no bastion",
            ),
            (
                "strata: static: floating-tag: jq over the world -> [\"a\"], 3 ms, WARN",
                "strata: static: floating-tag: jq over the world -> [\"a\"], 1 ms, WARN",
            ),
            (
                "  WARN  floating-tag  No profile pins a tag    104 ms",
                "  WARN  floating-tag  No profile pins a tag      1 ms",
            ),
            (
                // a title of 60 characters leaves one space before a count of five digits
                concat!(
                    "  WARN  floating-tag  ",
                    "No profile of the cluster pins a tag that moves to new image",
                    " 12345 ms"
                ),
                concat!(
                    "  WARN  floating-tag  ",
                    "No profile of the cluster pins a tag that moves to new image",
                    "     1 ms"
                ),
            ),
            (
                "| WARN | `floating-tag` | 7 ms | |",
                "| WARN | `floating-tag` | 1 ms | |",
            ),
            ("\"elapsedMs\": 250,", "\"elapsedMs\": 1,"),
            (
                "strata: watch: Running after 3s, reading 1",
                "strata: watch: Running after 0s, reading 1",
            ),
            (
                "made at 2026-10-03T11:02:59Z",
                "made at 2026-09-17T09:30:00Z",
            ),
        ];
        for (from, to) in cases {
            assert_eq!(clock.fixed(from), to, "{from}");
        }
    }
}
