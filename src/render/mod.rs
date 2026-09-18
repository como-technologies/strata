//! Renderers for the diff document, one per output format, behind one trait.

mod json;
mod markdown;
mod terminal;
mod unified;

use serde_json::Value;

use crate::color::Paint;
use crate::diff::{Diff, Leaf, PackDiff, Status, VariableSummary, VariablesByName, Verify};
use crate::error::Result;
use crate::ops::{Chain, ConsumersReport, Listing, PackListing, Pair, PairStatus, VerifySummary};
use crate::snapshot::{Snapshot, Source};

pub use json::JsonRenderer;
pub use markdown::MarkdownRenderer;
pub use terminal::TerminalRenderer;

/// The output formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Colored text for a terminal, plain for a pipe.
    Terminal,
    /// A pull request comment.
    Markdown,
    /// The `strata-diff/1` document.
    Json,
}

/// Turns a diff into text.
pub trait Renderer {
    /// Render the diff. `full` adds unified diffs of raw values and changed manifests where the
    /// format supports them.
    fn render(&self, d: &Diff, a: &Snapshot, b: &Snapshot, full: bool) -> Result<String>;

    /// One line stating the outcome of `verify`, or `None` for formats where the document itself
    /// carries it.
    fn verdict(&self, _v: &Verify, _recorded: &Source) -> Option<String> {
        None
    }

    /// Render the outcome of `verify` over several records.
    fn render_verify_summary(&self, s: &VerifySummary) -> Result<String>;

    /// Render the outcome of `same-release`.
    fn render_chain(&self, c: &Chain) -> Result<String>;

    /// Render the clusters that use a profile version.
    fn render_consumers(&self, r: &ConsumersReport) -> Result<String>;

    /// Render the profiles, clusters, and projects `list` found.
    fn render_list(&self, l: &Listing) -> Result<String>;

    /// The versions a registry offers for a pack.
    fn render_packs(&self, l: &PackListing) -> Result<String>;
}

/// `project lab (uid p1)`, `project p1` when the name is unknown, or `tenant scope`.
pub(crate) fn list_scope(l: &Listing) -> String {
    match (l.project_uid.as_str(), l.project_name.as_str()) {
        ("", _) => "tenant scope".to_string(),
        (uid, "") => format!("project {uid}"),
        (uid, name) => format!("project {name} (uid {uid})"),
    }
}

/// `2 profiles`, `1 profile`, or `no profiles`.
pub(crate) fn count_of(n: usize, singular: &str) -> String {
    match n {
        0 => format!("no {singular}s"),
        1 => format!("1 {singular}"),
        n => format!("{n} {singular}s"),
    }
}

/// `2 profiles`, or, with the `--pack` filter, `2 profiles with a pack matching 'cilium'`.
pub(crate) fn list_count(n: usize, singular: &str, pack: Option<&str>) -> String {
    let count = count_of(n, singular);
    match pack {
        Some(text) => format!("{count} with a pack matching '{text}'"),
        None => count,
    }
}

/// `2 clusters`, or, with the `--cluster` filter, `2 clusters matching 'payments'`.
pub(crate) fn cluster_count(n: usize, text: Option<&str>) -> String {
    let count = count_of(n, "cluster");
    match text {
        Some(text) => format!("{count} matching '{text}'"),
        None => count,
    }
}

/// `1 consumer`, `2 consumers`, or `no consumers`.
pub(crate) fn consumer_count(n: usize) -> String {
    count_of(n, "consumer")
}

/// A listed pack's version, with the tag when the profile pins another name for it:
/// `24.04 (tag LTS__24.x)`.
pub(crate) fn pack_version(pk: &crate::ops::ListedPack) -> String {
    if pk.tag.is_empty() || pk.tag == pk.version {
        pk.version.clone()
    } else {
        format!("{} (tag {})", pk.version, pk.tag)
    }
}

/// The error message of a listed entry, when it has one.
pub(crate) fn entry_error(error: Option<&Value>) -> Option<&str> {
    error.and_then(|e| e.get("message")).and_then(Value::as_str)
}

/// `<name>@<version>` of a profile a cluster applies, as `list` and the reports print it.
pub(crate) fn applied_ref(a: &crate::snapshot::AppliedProfile) -> String {
    record_label(&a.name, &a.version)
}

/// True when Palette lists the clusters that use a side: a profile version has consumers, but a
/// side that records a cluster, or one pack from a registry, has none.
pub(crate) fn lists_consumers(s: &Source) -> bool {
    s.cluster.is_none() && s.pack.is_none()
}

/// True when both sides are profile versions, and the reports compare the two counts.
pub(crate) fn consumers_comparable(d: &Diff) -> bool {
    lists_consumers(&d.a) && lists_consumers(&d.b)
}

/// What the consumers of the profile side say, when the other side is a cluster or a pack:
/// `aws-stack@1.1.0 has 1 consumer; payments is one of them`. The count of the other side is 0
/// because it has no consumers, so the reports state the profile side alone. None when both sides
/// are profile versions, or when neither is.
pub(crate) fn consumers_sentence(d: &Diff) -> Option<String> {
    let (profile, other) = match (lists_consumers(&d.a), lists_consumers(&d.b)) {
        (true, false) => (&d.a, &d.b),
        (false, true) => (&d.b, &d.a),
        _ => return None,
    };
    let count = consumer_count(profile.consumers.count);
    let has = format!("{}@{} has {count}", profile.name, profile.version);
    match &other.cluster {
        Some(c) if profile.consumers.count > 0 => Some(format!(
            "{has}; {} is {}one of them",
            c.name,
            if profile.consumers.clusters.contains(&c.uid) {
                ""
            } else {
                "not "
            }
        )),
        _ => Some(has),
    }
}

/// `lab-profile@1.3.0 (uid <uid>): 2 consumers`, or `...: no consumers`.
pub(crate) fn consumers_heading(r: &ConsumersReport) -> String {
    let count = consumer_count(r.count);
    format!(
        "{}@{} (uid {}): {count}",
        r.profile.name,
        r.profile.version,
        dash(&r.profile.uid)
    )
}

/// The status word of a pair, as the text reports print it.
pub(crate) fn pair_word(status: PairStatus) -> &'static str {
    match status {
        PairStatus::Identical => "IDENTICAL",
        PairStatus::SameContent => "SAME CONTENT",
        PairStatus::RegistryMove => "REGISTRY MOVE",
        PairStatus::Different => "DIFFERENT",
    }
}

/// The short sentence after a pair's status.
pub(crate) fn pair_detail(p: &Pair) -> String {
    match p.status {
        PairStatus::Identical => format!("root {}", short(&p.root_a)),
        PairStatus::SameContent if p.packs_retagged > 0 => format!(
            "root {} -> {}, {} pinned under another tag name, the same build",
            short(&p.root_a),
            short(&p.root_b),
            count_of(p.packs_retagged, "pack")
        ),
        PairStatus::SameContent => format!(
            "root {} -> {}, only the packUid minted for each manifest pack differs",
            short(&p.root_a),
            short(&p.root_b)
        ),
        PairStatus::RegistryMove => {
            format!("{} packs re-pinned, same configuration", p.packs_repinned)
        }
        PairStatus::Different => format!("root {} -> {}", short(&p.root_a), short(&p.root_b)),
    }
}

/// `SAME RELEASE: 2 pairs passed`, or `NOT THE SAME RELEASE: 1 of 2 pairs failed`.
pub(crate) fn chain_verdict(c: &Chain) -> String {
    if c.same {
        format!("SAME RELEASE: {} pairs passed", c.summary.passed)
    } else {
        format!(
            "NOT THE SAME RELEASE: {} of {} pairs failed",
            c.summary.failed, c.summary.pairs
        )
    }
}

/// What a verdict line is about: a published version, or what a cluster runs.
pub(crate) fn subject(recorded: &Source) -> String {
    if let Some(p) = &recorded.pack {
        let registry = if p.registry_name.is_empty() {
            p.registry_uid.clone()
        } else {
            p.registry_name.clone()
        };
        return format!("pack {}@{} in registry {registry}", p.name, p.version);
    }
    match &recorded.cluster {
        Some(c) => format!(
            "cluster {} running {}@{}",
            c.name, recorded.name, recorded.version
        ),
        None => format!("published version {}@{}", recorded.name, recorded.version),
    }
}

/// `name@version`, or `-` when the record could not be read.
pub(crate) fn record_label(name: &str, version: &str) -> String {
    if name.is_empty() {
        "-".to_string()
    } else {
        format!("{name}@{version}")
    }
}

/// For a cluster record whose running profile versions are gone from Palette: `cluster c runs
/// name@1.2.0, but uid u now names 1.3.0: the version was overwritten in place`, or `but uid u no
/// longer exists`, one clause per profile.
pub(crate) fn profiles_overwritten_sentence(v: &Verify, recorded: &Source) -> Option<String> {
    if v.profiles_overwritten.is_empty() {
        return None;
    }
    let cluster = recorded
        .cluster
        .as_ref()
        .map(|c| c.name.as_str())
        .unwrap_or_default();
    Some(
        v.profiles_overwritten
            .iter()
            .map(|o| {
                let runs = format!(
                    "cluster {cluster} runs {}@{}, but uid {}",
                    o.name, o.cluster_version, o.uid
                );
                if o.profile_version.is_empty() {
                    format!("{runs} no longer exists")
                } else {
                    format!(
                        "{runs} now names {}: the version was overwritten in place",
                        o.profile_version
                    )
                }
            })
            .collect::<Vec<_>>()
            .join("; "),
    )
}

/// The sentence after a record's status.
fn overwritten_by(r: &crate::ops::VerifiedRecord) -> Option<&str> {
    r.diff
        .as_ref()
        .and_then(|d| d.verify.as_ref())
        .and_then(|v| v.overwritten_by.as_deref())
}

/// `uid u is name@1.2.0 on one side and name@1.3.0 on the other: the version was overwritten in
/// place`, one per profile uid the two sides name under different versions. A verify whose verdict
/// already says the recorded version was overwritten gets none.
pub(crate) fn overwritten_in_place_notes(d: &Diff) -> Vec<String> {
    if d.verify
        .as_ref()
        .is_some_and(|v| v.overwritten_by.is_some())
    {
        return Vec::new();
    }
    d.overwritten_in_place
        .iter()
        .map(|o| {
            let name_b = if o.name_b.is_empty() { &o.name } else { &o.name_b };
            format!(
                "uid {} is {}@{} on one side and {}@{} on the other: the version was overwritten in place",
                o.uid, o.name, o.version_a, name_b, o.version_b
            )
        })
        .collect()
}

/// `the roots differ only by the tag name a pack is pinned under: the same pack build`, or by the
/// packUid Palette mints for each profile's manifest pack, for a diff whose roots differ in
/// nothing else.
pub(crate) fn same_content_note(d: &Diff) -> Option<String> {
    if d.same || !d.same_content {
        return None;
    }
    let retagged = d.packs.iter().any(|p| p.tag_renamed);
    let minted = d.packs.iter().any(|p| p.minted_uid_only);
    Some(match (retagged, minted) {
        (true, false) => {
            "the roots differ only by the tag name a pack is pinned under: the same pack build"
        }
        (true, true) => "the roots differ only by the tag name a pack is pinned under and the packUid Palette mints for each profile's manifest pack: the same content",
        _ => "the roots differ only by the packUid Palette mints for each profile's manifest pack: the same content",
    }.to_string())
}

/// `tag 24.04 -> LTS__24.04`, for a pack whose identity moved only by its tag name.
pub(crate) fn tag_rename(pd: &PackDiff) -> Option<String> {
    match (pd.tag_renamed, &pd.a, &pd.b) {
        (true, Some(a), Some(b)) => Some(format!("tag {} -> {}", a.tag, b.tag)),
        _ => None,
    }
}

/// `both sides are name@version (uid u): the version was edited in place`, for a diff whose sides
/// name one profile version with different content.
pub(crate) fn edited_in_place_note(d: &Diff) -> Option<String> {
    d.edited_in_place.then(|| {
        let clusters = d.a.cluster.is_some() || d.b.cluster.is_some();
        format!(
            "both sides are {}@{} (uid {}): the version was edited in place{}",
            d.a.name,
            d.a.version,
            d.a.uid,
            if clusters {
                ", or changed on a cluster"
            } else {
                ""
            }
        )
    })
}

fn record_values_changed(r: &crate::ops::VerifiedRecord) -> usize {
    r.cluster_variables.as_ref().map_or(0, |c| c.values.len())
}

fn record_profiles_overwritten(r: &crate::ops::VerifiedRecord) -> Option<String> {
    let d = r.diff.as_ref()?;
    profiles_overwritten_sentence(d.verify.as_ref()?, &d.a)
}

pub(crate) fn record_detail(r: &crate::ops::VerifiedRecord) -> String {
    use crate::ops::RecordStatus;
    match r.status {
        RecordStatus::Unchanged => format!(
            "unchanged since {}{}",
            r.recorded_at,
            values_changed_suffix(record_values_changed(r))
        ),
        RecordStatus::Drifted if record_profiles_overwritten(r).is_some() => format!(
            "{}{}",
            record_profiles_overwritten(r).unwrap_or_default(),
            values_changed_suffix(record_values_changed(r))
        ),
        RecordStatus::Drifted if overwritten_by(r).is_some() => format!(
            "now names {}: the version recorded {} was overwritten in place{}",
            overwritten_by(r).unwrap_or_default(),
            r.recorded_at,
            values_changed_suffix(record_values_changed(r))
        ),
        RecordStatus::Drifted => format!(
            "differs from the copy recorded {}; max impact {}, {} packs changed{}",
            r.recorded_at,
            r.max_impact
                .and_then(|l| serde_json::to_value(l).ok())
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default(),
            r.packs_changed.unwrap_or(0),
            values_changed_suffix(record_values_changed(r))
        ),
        RecordStatus::Error => r
            .error
            .as_ref()
            .and_then(|e| e.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("could not be checked")
            .to_string(),
    }
}

/// `3 records: 2 unchanged, 1 drifted, 0 errors`.
pub(crate) fn counts_line(s: &VerifySummary) -> String {
    let c = &s.summary;
    format!(
        "{} records: {} unchanged, {} drifted, {} errors",
        c.records, c.unchanged, c.drifted, c.errors
    )
}

/// The renderer for a format.
pub fn renderer(format: Format, paint: Paint, compact: bool) -> Box<dyn Renderer> {
    match format {
        Format::Terminal => Box::new(TerminalRenderer { paint }),
        Format::Markdown => Box::new(MarkdownRenderer),
        Format::Json => Box::new(JsonRenderer { compact }),
    }
}

/// The first twelve characters of a hash, or `-` for an empty one.
pub(crate) fn short(s: &str) -> &str {
    if s.is_empty() {
        "-"
    } else {
        let end = s.char_indices().nth(12).map(|(i, _)| i).unwrap_or(s.len());
        &s[..end]
    }
}

/// `-` for an empty string.
pub(crate) fn dash(s: &str) -> &str {
    if s.is_empty() {
        "-"
    } else {
        s
    }
}

/// JSON rendering of a leaf value, truncated for the text reports; the JSON document carries all of it.
pub(crate) fn show(v: &Value) -> String {
    let t = serde_json::to_string(v).unwrap_or_default();
    let n = t.chars().count();
    if n > 120 {
        format!(
            "{}... ({n} chars, see --full)",
            t.chars().take(100).collect::<String>()
        )
    } else {
        t
    }
}

/// `IDENTITY UPSTREAM AUTHORED`, only the flags that are set, uncolored; a renamed tag in place of
/// `IDENTITY`.
pub(crate) fn plain_flags(pd: &PackDiff) -> String {
    let mut f = Vec::new();
    let rename = tag_rename(pd);
    if let Some(t) = &rename {
        f.push(t.as_str());
    } else if pd.identity {
        f.push("IDENTITY");
    }
    if pd.upstream {
        f.push("UPSTREAM");
    }
    if pd.authored {
        f.push("AUTHORED");
    }
    f.join(" ")
}

/// The version column: `a -> b` when the identity moved and the versions differ.
pub(crate) fn version_display(pd: &PackDiff) -> String {
    let av = pd.a.as_ref().map(|s| s.version.as_str()).unwrap_or("");
    let bv = pd.b.as_ref().map(|s| s.version.as_str()).unwrap_or("");
    match pd.status {
        Status::Added => bv.to_string(),
        Status::Removed => av.to_string(),
        _ if pd.identity && av != bv => format!("{av} -> {bv}"),
        _ => av.to_string(),
    }
}

/// One printable variable change: a whole variable added or removed, or one changed field.
pub(crate) enum VarLine<'a> {
    /// The variable exists only in B.
    Added(&'a VariableSummary),
    /// The variable exists only in A.
    Removed(&'a VariableSummary),
    /// One field of a variable present on both sides; the leaf path is relative to the variable.
    Field(&'a str, &'a Leaf),
}

/// The variable changes in name order, one entry per line the text reports print.
pub(crate) fn variable_lines(g: &VariablesByName) -> Vec<VarLine<'_>> {
    let mut lines: Vec<(&str, VarLine<'_>)> = Vec::new();
    lines.extend(g.added.iter().map(|v| (v.name.as_str(), VarLine::Added(v))));
    lines.extend(
        g.removed
            .iter()
            .map(|v| (v.name.as_str(), VarLine::Removed(v))),
    );
    for c in &g.changed {
        lines.extend(
            c.fields
                .iter()
                .map(|l| (c.name.as_str(), VarLine::Field(&c.name, l))),
        );
    }
    lines.sort_by_key(|(name, _)| *name);
    lines.into_iter().map(|(_, l)| l).collect()
}

/// `1 variable value changed`, `2 variable values changed`.
pub(crate) fn values_changed(n: usize) -> String {
    format!(
        "{n} variable {} changed",
        if n == 1 { "value" } else { "values" }
    )
}

/// `; 1 variable value changed`, or nothing when none did.
pub(crate) fn values_changed_suffix(n: usize) -> String {
    if n == 0 {
        String::new()
    } else {
        format!("; {}", values_changed(n))
    }
}

/// A cluster value as the reports print it: quoted, or `(sensitive)`.
pub(crate) fn cluster_value(value: &Option<String>, sensitive: bool) -> String {
    match value {
        Some(v) => show(&Value::from(v.as_str())),
        None if sensitive => "(sensitive)".to_string(),
        None => "null".to_string(),
    }
}

/// ` (required, sensitive)`, or nothing when no trait is set.
pub(crate) fn traits_suffix(v: &VariableSummary) -> String {
    let t = v.traits();
    if t.is_empty() {
        String::new()
    } else {
        format!(" ({})", t.join(", "))
    }
}
