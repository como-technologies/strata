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

    /// The pack versions that records pin, against the newer versions the registries offer.
    fn render_behind(&self, r: &crate::ops::BehindReport) -> Result<String>;
}

/// `env=prod, team`: the labels of a cluster, with a tag that has no value by its key alone.
pub(crate) fn labels_text(labels: &std::collections::BTreeMap<String, String>) -> String {
    labels
        .iter()
        .map(|(k, v)| {
            if v == crate::ops::TAG_WITHOUT_VALUE {
                k.clone()
            } else {
                format!("{k}={v}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// `the clusters with env=prod and the saved filter prod (region=us), from the search of
/// Palette`, for a listing with a query.
pub(crate) fn query_line(l: &Listing) -> Option<String> {
    let q = l.query.as_ref()?;
    let mut parts: Vec<String> = q.tags.clone();
    if let Some(name) = &q.filter {
        let expr = l
            .query_filter
            .as_ref()
            .map(|f| format!(" ({})", f.expression()))
            .unwrap_or_default();
        parts.push(format!("the saved filter {name}{expr}"));
    }
    let how = match l.resolved {
        Some(crate::ops::Resolved::Search) => "from the search of Palette, at tenant scope",
        Some(crate::ops::Resolved::Listing) => "from the cluster listing, labels matched by strata",
        None => "",
    };
    Some(format!("the clusters with {}, {how}", parts.join(" and ")))
}

/// `4 pins from 2 records: 1 behind`, with the floating pins, the pins not checked, and the
/// versions that arrived when there are any.
pub(crate) fn behind_heading(r: &crate::ops::BehindReport) -> String {
    let s = &r.summary;
    let mut line = format!(
        "{} from {}: {} behind",
        count_of(s.pins, "pin"),
        count_of(r.records.len(), "record"),
        s.behind
    );
    if s.floating > 0 {
        line.push_str(&format!(", {} floating", s.floating));
    }
    if s.errors > 0 {
        line.push_str(&format!(", {} not checked", s.errors));
    }
    if let Some(since) = &r.since {
        line.push_str(&format!(
            "; {} arrived since {since}",
            count_of(s.arrived, "version")
        ));
    }
    line
}

/// The text of an impact level, as the reports print it: `in-place`, `may-repave`.
pub(crate) fn impact_word(level: crate::impact::ImpactLevel) -> String {
    serde_json::to_value(level)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// What a pin is: `behind: 1.65.0, 1.64.0`, `current`, or `not checked`, with its floating tag and
/// the version the tag names now, and the state of the pinned version when the registry gives one.
pub(crate) fn pin_status(pin: &crate::ops::Pin) -> String {
    let mut text = if pin.error.is_some() {
        "not checked".to_string()
    } else if pin.behind {
        let newer: Vec<String> = pin
            .newer
            .iter()
            .map(|n| match &n.state {
                Some(state) => format!("{} ({state})", n.version),
                None => n.version.clone(),
            })
            .collect();
        format!("behind: {}", newer.join(", "))
    } else {
        "current".to_string()
    };
    if pin.floating {
        let now = pin
            .resolved
            .as_deref()
            .map(|v| format!(", now {v}"))
            .unwrap_or_default();
        text.push_str(&format!(" (floating tag {}{now})", pin.tag));
    }
    if let Some(state) = &pin.pinned_state {
        text.push_str(&format!("; the pinned version is {state}"));
    }
    text
}

/// `bump to 1.65.0: in-place IDENTITY UPSTREAM AUTHORED`, for a pin with a bump.
pub(crate) fn bump_line(pin: &crate::ops::Pin) -> Option<String> {
    let b = pin.bump.as_ref()?;
    let mut line = format!("bump to {}: {}", b.to, impact_word(b.impact));
    if !b.flags.is_empty() {
        line.push_str(&format!(" {}", b.flags.join(" ")));
    }
    Some(line)
}

/// `project lab (uid p1)`, `project p1` when the name is unknown, or `tenant scope`.
pub(crate) fn list_scope(l: &Listing) -> String {
    match (l.project_uid.as_str(), l.project_name.as_str()) {
        ("", _) => "tenant scope".to_string(),
        (uid, "") => format!("project {uid}"),
        (uid, name) => format!("project {name} (uid {uid})"),
    }
}

/// A cluster's project, in the shape the scope line uses: `dev (uid 64f2...)`, or the uid alone
/// when the name could not be read. A uid names no project to a reader.
pub(crate) fn project_label(uid: &str, name: &str) -> String {
    match (uid, name) {
        ("", _) => dash("").to_string(),
        (uid, "") => uid.to_string(),
        (uid, name) => format!("{name} (uid {uid})"),
    }
}

/// `2 profiles`, `1 profile`, or `no profiles`.
/// What a packs heading adds for a registry: the state that all its versions share, and that it
/// has no tags when Palette answered 404 for it.
pub(crate) fn registry_note(r: &crate::ops::PackRegistry) -> Option<String> {
    match (&r.state, r.tags_unavailable) {
        (Some(state), true) => Some(format!("{state}, no tags")),
        (Some(state), false) => Some(state.clone()),
        (None, true) => Some("no tags: the registry answered 404".to_string()),
        (None, false) => None,
    }
}

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

/// True when Palette lists the clusters that use a side: a published profile version has
/// consumers, but a side that records a cluster, or one pack from a registry, has none. A
/// candidate has none either, and for another reason: Palette does not hold it yet, so no request
/// asked about it. `no clusters` would state a count that nothing counted.
pub(crate) fn lists_consumers(s: &Source) -> bool {
    s.consumers.is_some()
}

/// True when the two sides are releases of each other: two profile-shaped sides, or two versions
/// of one pack. A pack against a profile, or one pack against another pack, is a comparison a
/// reader can ask for, and no cluster can make that transition. The pack rows and the impact then
/// describe a move that cannot happen.
pub(crate) fn releases_of_each_other(d: &Diff) -> bool {
    match (&d.a.pack, &d.b.pack) {
        (None, None) => true,
        (Some(a), Some(b)) => a.name == b.name,
        _ => false,
    }
}

/// The note for a comparison whose two sides are not releases of each other.
pub(crate) fn cross_kind_note(d: &Diff) -> Option<String> {
    if releases_of_each_other(d) {
        return None;
    }
    Some(
        "the two sides are not releases of each other, so the added and removed packs, and the \
         impact, describe no change a cluster can make"
            .to_string(),
    )
}

/// True when a side can have profile variables. A registry pack cannot, and no request asks for
/// any, so `variables unchanged` there would state a comparison that nothing compared.
pub(crate) fn lists_variables(s: &Source) -> bool {
    s.pack.is_none()
}

/// True when the reports can state anything about the variables of a comparison.
pub(crate) fn variables_comparable(d: &Diff) -> bool {
    lists_variables(&d.a) || lists_variables(&d.b)
}

/// A note for a side whose variables endpoint Palette did not serve. That side's root is the root
/// of a version with no variables, so it cannot be compared with a side read from a Palette that
/// serves the endpoint. Nothing else in the report tells the two apart.
pub(crate) fn variables_unavailable_note(d: &Diff) -> Option<String> {
    let sides = match (d.a.variables_unavailable, d.b.variables_unavailable) {
        (true, true) => "both sides",
        (true, false) => "side A",
        (false, true) => "side B",
        (false, false) => return None,
    };
    Some(format!(
        "{sides}: Palette did not serve the variables endpoint, so the root is the root of a \
         version with no variables"
    ))
}

/// The two counts, when a request gave one for each side. None when either side has none, so a
/// renderer cannot print a count that nothing counted.
pub(crate) fn consumer_counts(d: &Diff) -> Option<(usize, usize)> {
    Some((d.a.consumers.as_ref()?.count, d.b.consumers.as_ref()?.count))
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
    let consumers = profile.consumers.as_ref()?;
    let count = consumer_count(consumers.count);
    let has = format!("{}@{} has {count}", profile.name, profile.version);
    match &other.cluster {
        // the consumer list of Palette names clusters, and never an edge host
        Some(c) if !c.kind.is_cluster() => Some(has),
        Some(c) if consumers.count > 0 => Some(format!(
            "{has}; {} is {}one of them",
            c.name,
            if consumers.clusters.contains(&c.uid) {
                ""
            } else {
                "not "
            }
        )),
        _ => Some(has),
    }
}

/// What the second reading read, after `second reading: `: `3 of 3 clusters read` from the
/// listing, or, from the cluster search, `the cluster search names 2 other clusters with the
/// profile, 2 read`. The search takes the name of the profile, so it names each cluster that runs
/// a version of it; strata keeps the clusters that run this version. Only the clusters that the
/// consumer list did not name are read.
pub(crate) fn second_reading_counts(read: &crate::ops::ClustersRead) -> String {
    let counts = second_reading_main(read);
    // the virtual clusters of the cluster groups that pin the version (#184)
    match (read.groups, &read.groups_error) {
        (0, None) => counts,
        (0, Some(e)) => format!(
            "{counts}; cluster groups not read ({})",
            entry_error(Some(e)).unwrap_or("an error")
        ),
        (g, _) => format!(
            "{counts}; {g} cluster {} the version, {} other virtual {}, {} read",
            if g == 1 { "group pins" } else { "groups pin" },
            read.members,
            if read.members == 1 {
                "cluster"
            } else {
                "clusters"
            },
            read.members.saturating_sub(read.members_unread)
        ),
    }
}

fn second_reading_main(read: &crate::ops::ClustersRead) -> String {
    // the virtual clusters of the groups are counted apart, read and not read
    let n = (read.read + read.unread).saturating_sub(read.members);
    let read_n = read
        .read
        .saturating_sub(read.members.saturating_sub(read.members_unread));
    match read.resolved {
        Some(crate::ops::Resolved::Search) if n == 0 => {
            "the cluster search names no other cluster with the profile".to_string()
        }
        Some(crate::ops::Resolved::Search) => format!(
            "the cluster search names {} other {} with the profile, {} read",
            n,
            if n == 1 { "cluster" } else { "clusters" },
            read_n
        ),
        _ => format!("{read_n} of {n} clusters read"),
    }
}

/// `edge hosts: 3 read`, or why they could not be read.
pub(crate) fn edge_hosts_line(r: &ConsumersReport) -> String {
    let read = &r.edge_hosts_read;
    if read.skipped {
        return format!("edge hosts: not read ({})", read.reason);
    }
    match read
        .error
        .as_ref()
        .and_then(|e| e.get("message"))
        .and_then(Value::as_str)
    {
        Some(msg) => format!("edge hosts: not read ({msg})"),
        None => format!("edge hosts: {} read", read.read),
    }
}

/// The text with its first letter in upper case.
pub(crate) fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
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
        PairStatus::UnknownUpstream => "NO DIGEST",
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
            "root {} -> {}, only the packUid that Palette mints differs",
            short(&p.root_a),
            short(&p.root_b)
        ),
        PairStatus::RegistryMove => {
            format!(
                "{} re-pinned, same configuration",
                count_of(p.packs_repinned, "pack")
            )
        }
        PairStatus::Different | PairStatus::UnknownUpstream => {
            format!("root {} -> {}", short(&p.root_a), short(&p.root_b))
        }
    }
}

/// `SAME RELEASE: 2 pairs passed`, or `NOT THE SAME RELEASE: 1 of 2 pairs failed`. One pair is
/// a pair, not `1 pairs`: the reports count in the singular for one.
pub(crate) fn chain_verdict(c: &Chain) -> String {
    if c.same {
        format!(
            "SAME RELEASE: {} passed",
            count_of(c.summary.passed, "pair")
        )
    } else {
        format!(
            "NOT THE SAME RELEASE: {} of {} failed",
            c.summary.failed,
            count_of(c.summary.pairs, "pair")
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

/// The verify part of the diff of a record that did not stay unchanged.
fn record_verify(r: &crate::ops::VerifiedRecord) -> Option<&Verify> {
    r.diff.as_ref().and_then(|d| d.verify.as_ref())
}

/// The sentence after a record's status.
fn overwritten_by(r: &crate::ops::VerifiedRecord) -> Option<&str> {
    record_verify(r).and_then(|v| v.overwritten_by.as_deref())
}

/// `uid u is name@1.2.0 on one side and name@1.3.0 on the other: the version was overwritten in
/// place`, one per profile uid the two sides name under different versions. A verify whose verdict
/// already says the recorded version was overwritten gets none.
pub(crate) fn overwritten_in_place_notes(d: &Diff) -> Vec<String> {
    if d.verify
        .as_ref()
        .is_some_and(|v| v.overwritten_by.is_some() || v.renamed_to.is_some())
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
/// packUid that Palette mints for a manifest pack or for a pack from a registry that it does not
/// sync, for a diff whose roots differ in nothing else.
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
        (true, true) => "the roots differ only by the tag name a pack is pinned under and the packUid that Palette mints for a manifest pack or for a pack from a registry that it does not sync: the same content",
        _ => "the roots differ only by the packUid that Palette mints for a manifest pack or for a pack from a registry that it does not sync: the same content",
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
/// The note for the user macros the content uses: `the content uses macro region: the root
/// covers its name and not its value, so an edit of the macro does not show`. None when it uses
/// none.
pub(crate) fn macros_note(d: &Diff) -> Option<String> {
    let (noun, pron) = match d.macros.len() {
        0 => return None,
        1 => ("macro", "its name and not its value"),
        _ => ("macros", "their names and not their values"),
    };
    Some(format!(
        "the content uses {noun} {}: the root covers {pron}, so an edit of a macro does not show",
        d.macros.join(", ")
    ))
}

/// Why a record drifted when only macro values moved: `macro region has another value now`.
pub(crate) fn macros_changed_sentence(names: &[String]) -> String {
    match names {
        [one] => format!("macro {one} has another value now"),
        many => format!("macros {} have other values now", many.join(", ")),
    }
}

/// The note for two cluster records whose macro values differ.
pub(crate) fn macros_changed_note(d: &Diff) -> Option<String> {
    (!d.macros_changed.is_empty()).then(|| {
        format!(
            "{}: the clusters run other content, also where the roots are equal",
            match d.macros_changed.as_slice() {
                [one] => format!("macro {one} has another value on each side"),
                many => format!("macros {} have other values on each side", many.join(", ")),
            }
        )
    })
}

/// Palette's answer to `--repave`, one line for each reason: `repave: PackValuesUpdated
/// kubernetes (kubeadmconfig.postKubeadmCommands)`, or `no repave`. Empty without the option.
pub(crate) fn palette_repave_lines(d: &Diff) -> Vec<String> {
    let Some(r) = &d.palette_repave else {
        return Vec::new();
    };
    if !r.required {
        return vec!["no repave".to_string()];
    }
    if r.reasons.is_empty() {
        return vec!["repave".to_string()];
    }
    r.reasons
        .iter()
        .map(|x| {
            let keys = if x.keys.is_empty() {
                String::new()
            } else {
                format!(" ({})", x.keys.join(", "))
            };
            format!("repave: {} {}{keys}", x.code, dash(&x.pack))
        })
        .collect()
}

/// Palette's answer to `--validate`: `accepts`, or one line for each error, `refuses:
/// PackDependencyNotFound strata-live-pack: <message>`. Empty without the option.
pub(crate) fn palette_validation_lines(d: &Diff) -> Vec<String> {
    let Some(v) = &d.palette_validation else {
        return Vec::new();
    };
    if v.errors.is_empty() {
        return vec!["accepts".to_string()];
    }
    v.errors
        .iter()
        .map(|e| {
            let pack = if e.pack.is_empty() {
                String::new()
            } else {
                format!(" {}", e.pack)
            };
            format!("refuses: {}{pack}: {}", e.code, e.message)
        })
        .collect()
}

/// The note when the impact table and Palette disagree about a repave.
pub(crate) fn palette_repave_note(d: &Diff) -> Option<String> {
    let r = d.palette_repave.as_ref().filter(|r| !r.agrees)?;
    let level = serde_json::to_value(d.summary.max_impact)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    Some(format!(
        "the impact table says {level}, and Palette says {} for this cluster: the table does not \
         describe its upgrade engine",
        if r.required { "repave" } else { "no repave" }
    ))
}

/// The note for the packs with no upstream digest: `pack csi/x has no upstream digest: Palette
/// gives none, so a change of its archive does not show`. None when every pack has one.
pub(crate) fn upstream_unknown_note(d: &Diff) -> Option<String> {
    let (noun, verb) = match d.upstream_unknown.len() {
        0 => return None,
        1 => ("pack", "has"),
        _ => ("packs", "have"),
    };
    Some(format!(
        "{noun} {} {verb} no upstream digest: Palette gives none, so a change of the archive does \
         not show",
        d.upstream_unknown.join(", ")
    ))
}

/// One note for each cluster side that Palette says is behind its profile: `cluster fred is
/// behind: an update of strata-live-addon waits, since <time>`. A side read when nothing waited,
/// or whose notifications were not read, gets none.
pub(crate) fn updates_pending_notes(d: &Diff) -> Vec<String> {
    [&d.a, &d.b]
        .into_iter()
        .filter_map(|side| side.cluster.as_ref())
        .filter(|c| c.updates_pending == Some(true))
        .map(|c| {
            let mut names: Vec<&str> = c
                .pending_updates
                .iter()
                .map(|u| u.profile.as_str())
                .collect();
            names.dedup();
            let since = c
                .pending_updates
                .first()
                .map(|u| format!(", since {}", u.since))
                .unwrap_or_default();
            format!(
                "cluster {} is behind: Palette says an update of {} waits for it{since}",
                c.name,
                names.join(", ")
            )
        })
        .collect()
}

/// One note for each pack that a cluster side runs at another version than its profile
/// declares: `cluster fred runs kubernetes 1.36.1, and its profile declares 1.36.2`.
pub(crate) fn running_behind_notes(d: &Diff) -> Vec<String> {
    d.running_behind
        .iter()
        .map(|r| {
            let side = if r.side == "a" { &d.a } else { &d.b };
            let name = side.cluster.as_ref().map_or("", |c| c.name.as_str());
            format!(
                "cluster {name} runs {} {}, and its profile declares {}",
                r.pack, r.running, r.declared
            )
        })
        .collect()
}

/// The path of a leaf inside a tier: `policy.timezone`, and `running[name=kubernetes].version`
/// for a tier that is a list.
pub(crate) fn tier_path(tier: &str, path: &str) -> String {
    if path.starts_with('[') {
        format!("{tier}{path}")
    } else {
        format!("{tier}.{path}")
    }
}

/// Each tier compared between two cluster records, with a word for its state: `applied same`,
/// `policy changed`. Empty when no tier was compared.
pub(crate) fn tier_states(d: &Diff) -> Vec<(String, bool)> {
    d.cluster_tiers
        .iter()
        .map(|t| (t.tier.clone(), t.same))
        .collect()
}

pub(crate) fn edited_in_place_note(d: &Diff) -> Option<String> {
    // a version that Palette no longer has was deleted, and not edited
    let gone = d.verify.as_ref().is_some_and(|v| v.gone);
    (d.edited_in_place && !gone).then(|| {
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

/// What `verify --who` found: `who: 2 writes in the audit log since the record`, or `who: no
/// audit entry since the record`. `None` without `--who`.
pub(crate) fn who_heading(v: &Verify) -> Option<String> {
    let who = v.who.as_ref()?;
    Some(if who.is_empty() {
        "who: no audit entry since the record".to_string()
    } else {
        format!(
            "who: {} in the audit log since the record",
            count_of(who.len(), "write")
        )
    })
}

/// One write: `2026-09-18T14:02:11.482Z  John Doe (user)  update`, then the kind and the name of
/// the object when the write touched another object than `uid`, the object of the record, then
/// `: <summary>` and `; note: <userMsg>` when Palette gives them.
pub(crate) fn who_entry(e: &crate::diff::AuditEntry, uid: &str) -> String {
    let mut line = format!("{}  {} ({})  {}", e.time, e.actor, e.actor_type, e.action);
    if let Some(r) = e.resource.as_ref().filter(|r| r.uid != uid) {
        line.push(' ');
        line.push_str(&r.kind);
        line.push(' ');
        line.push_str(if r.name.is_empty() { &r.uid } else { &r.name });
    }
    if !e.summary.is_empty() {
        line.push_str(": ");
        line.push_str(&e.summary);
    }
    if !e.user_msg.is_empty() {
        line.push_str("; note: ");
        line.push_str(&e.user_msg);
    }
    line
}

/// `; Palette last changed it <time>`: the end of a `DRIFT` verdict, from `verify.lastModified`.
/// Nothing for another verdict, or when Palette gives no time.
pub(crate) fn last_modified_suffix(v: &Verify) -> String {
    match &v.last_modified {
        Some(at) if v.status == crate::diff::VerifyStatus::Drifted => {
            format!("; Palette last changed it {at}")
        }
        _ => String::new(),
    }
}

/// What the summary line of a drifted record adds with `--who`: the last write, or that the log
/// holds none.
fn who_suffix(r: &crate::ops::VerifiedRecord) -> String {
    let Some(who) = r
        .diff
        .as_ref()
        .and_then(|d| d.verify.as_ref())
        .and_then(|v| v.who.as_ref())
    else {
        return String::new();
    };
    match who.last() {
        Some(e) => format!(
            "; last write: {} ({}), {} at {}",
            e.actor, e.actor_type, e.action, e.time
        ),
        None => "; no audit entry since the record".to_string(),
    }
}

/// The macros of a drifted record whose values moved; empty when none did, or the root moved.
fn record_macros_changed(r: &crate::ops::VerifiedRecord) -> Vec<String> {
    r.diff
        .as_ref()
        .filter(|d| d.same)
        .and_then(|d| d.verify.as_ref())
        .map(|v| v.macros_changed.clone())
        .unwrap_or_default()
}

/// The sentence after a record's status, and the state of its object when it is not the state
/// of one that runs: `; the state is Deleting`.
pub(crate) fn record_detail(r: &crate::ops::VerifiedRecord) -> String {
    let gone = record_verify(r).is_some_and(|v| v.gone);
    match r.state.as_deref() {
        Some(state) if !gone && !normal_state(state) => {
            format!("{}; the state is {state}", record_sentence(r))
        }
        _ => record_sentence(r),
    }
}

fn record_sentence(r: &crate::ops::VerifiedRecord) -> String {
    use crate::ops::RecordStatus;
    let sync = registry_sync_suffix(
        r.diff
            .as_ref()
            .and_then(|d| d.verify.as_ref())
            .and_then(|v| v.registry_sync.as_ref()),
    );
    let detail = match r.status {
        RecordStatus::Unchanged => format!(
            "unchanged since {}{}",
            r.recorded_at,
            values_changed_suffix(record_values_changed(r))
        ),
        RecordStatus::Unverified => format!(
            "same content as recorded {}, but {}",
            r.recorded_at,
            unverified_sentence(
                r.diff
                    .as_ref()
                    .map(|d| d.upstream_unknown.as_slice())
                    .unwrap_or_default()
            )
        ),
        RecordStatus::Drifted if record_verify(r).is_some_and(|v| v.gone) => {
            let (v, d) = (record_verify(r).unwrap(), r.diff.as_ref().unwrap());
            format!(
                "{}{}",
                gone_sentence(v, &d.a, str::to_string),
                who_suffix(r)
            )
        }
        RecordStatus::Drifted if record_profiles_overwritten(r).is_some() => format!(
            "{}{}{}",
            record_profiles_overwritten(r).unwrap_or_default(),
            values_changed_suffix(record_values_changed(r)),
            who_suffix(r)
        ),
        RecordStatus::Drifted if !record_macros_changed(r).is_empty() => format!(
            "has the content recorded {}, but {}{}{}",
            r.recorded_at,
            macros_changed_sentence(&record_macros_changed(r)),
            values_changed_suffix(record_values_changed(r)),
            who_suffix(r)
        ),
        RecordStatus::Drifted if record_verify(r).is_some_and(|v| v.renamed_to.is_some()) => {
            format!(
                "was renamed to {}{}",
                record_verify(r)
                    .and_then(|v| v.renamed_to.as_deref())
                    .unwrap_or_default(),
                who_suffix(r)
            )
        }
        RecordStatus::Drifted if overwritten_by(r).is_some() => format!(
            "now names {}: the version recorded {} was overwritten in place{}{}",
            overwritten_by(r).unwrap_or_default(),
            r.recorded_at,
            values_changed_suffix(record_values_changed(r)),
            who_suffix(r)
        ),
        RecordStatus::Drifted => format!(
            "differs from the copy recorded {}; max impact {}, {} changed{}{}{}",
            r.recorded_at,
            r.max_impact
                .and_then(|l| serde_json::to_value(l).ok())
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default(),
            count_of(r.packs_changed.unwrap_or(0), "pack"),
            // Palette keeps the order of the packs, so two orders are two roots: a reorder alone
            // is drift, and the line says so.
            if r.diff.as_ref().is_some_and(|d| d.order_changed) {
                ", the packs in another order"
            } else {
                ""
            },
            values_changed_suffix(record_values_changed(r)),
            who_suffix(r)
        ),
        RecordStatus::Error => r
            .error
            .as_ref()
            .and_then(|e| e.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("could not be checked")
            .to_string(),
    };
    detail + &sync
}

/// `3 records: 2 unchanged, 1 drifted, 0 errors`, and `1 record` for one.
pub(crate) fn counts_line(s: &VerifySummary) -> String {
    let c = &s.summary;
    let unverified = if c.unverified > 0 {
        format!(", {} unverified", c.unverified)
    } else {
        String::new()
    };
    format!(
        "{}: {} unchanged, {} drifted{unverified}, {}",
        count_of(c.records, "record"),
        c.unchanged,
        c.drifted,
        count_of(c.errors, "error")
    )
}

/// Why a record is unverified: `pack csi/x has no upstream digest, so a change of its archive
/// does not show; --allow-no-digest accepts it`.
pub(crate) fn unverified_sentence(packs: &[String]) -> String {
    let (noun, verb) = if packs.len() == 1 {
        ("pack", "has")
    } else {
        ("packs", "have")
    };
    format!(
        "{noun} {} {verb} no upstream digest, so a change of the archive does not show; \
         --allow-no-digest accepts it",
        packs.join(", ")
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
    if s.is_empty() { "-" } else { s }
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
    if pd.images {
        f.push("IMAGES");
    }
    if pd.charts {
        f.push("CHARTS");
    }
    f.join(" ")
}

/// One line for each image, chart, or param that moved: `~ image <repo>: <old> -> <new>`, `+
/// chart <name> <version>`, `~ param <name>: <old> -> <new>`.
pub(crate) fn artifact_lines(pd: &PackDiff) -> Vec<String> {
    let line = |kind: &str, c: &crate::diff::ArtifactChange| -> String {
        let old = c.old.as_deref().unwrap_or_default();
        let new = c.new.as_deref().unwrap_or_default();
        let at = |v: &str| {
            if kind == "image" {
                format!(":{v}")
            } else {
                format!(" {v}")
            }
        };
        match c.op {
            crate::diff::Op::Changed => format!("~ {kind} {}: {old} -> {new}", c.name),
            crate::diff::Op::Added => format!("+ {kind} {}{}", c.name, at(new)),
            crate::diff::Op::Removed => format!("- {kind} {}{}", c.name, at(old)),
        }
    };
    // a param value is JSON: a string reads as it is, and anything else as compact JSON
    let value = |v: &Option<Value>| match v {
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
        None => String::new(),
    };
    let param = |c: &crate::diff::ParamChange| -> String {
        match c.op {
            crate::diff::Op::Changed => {
                format!("~ param {}: {} -> {}", c.name, value(&c.old), value(&c.new))
            }
            crate::diff::Op::Added => format!("+ param {}: {}", c.name, value(&c.new)),
            crate::diff::Op::Removed => format!("- param {}: {}", c.name, value(&c.old)),
        }
    };
    pd.image_changes
        .iter()
        .map(|c| line("image", c))
        .chain(pd.chart_changes.iter().map(|c| line("chart", c)))
        .chain(pd.param_changes.iter().map(param))
        .collect()
}

/// The line of the images and the charts that changed across the packs: `4 images changed in 2
/// packs`. None when none changed.
pub(crate) fn artifacts_summary(d: &Diff) -> Option<String> {
    let s = &d.summary;
    if s.images_changed == 0 && s.charts_changed == 0 {
        return None;
    }
    let packs = d.packs.iter().filter(|p| p.images || p.charts).count();
    let n = |count: usize, one: &str, many: &str| {
        format!("{count} {}", if count == 1 { one } else { many })
    };
    Some(format!(
        "{}, {} changed in {}",
        n(s.images_changed, "image", "images"),
        n(s.charts_changed, "chart", "charts"),
        n(packs, "pack", "packs")
    ))
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

/// For a pack record, the end of the verdict: when Palette last synced the registry, or that its
/// last sync failed. Palette holds a copy of each registry, and the verdict states that copy.
/// Nothing when Palette gives no sync.
pub(crate) fn registry_sync_suffix(sync: Option<&crate::diff::RegistrySync>) -> String {
    match sync {
        Some(s) if s.status == "Failed" && s.message.is_empty() => {
            "; the last sync of the registry failed".to_string()
        }
        Some(s) if s.status == "Failed" => {
            format!("; the last sync of the registry failed: {}", s.message)
        }
        Some(s) if !s.last_synced_time.is_empty() => {
            format!("; Palette synced the registry {}", s.last_synced_time)
        }
        _ => String::new(),
    }
}

/// `1 variable value changed`, `2 variable values changed`.
pub(crate) fn values_changed(n: usize) -> String {
    format!(
        "{n} variable {} changed",
        if n == 1 { "value" } else { "values" }
    )
}

/// The state that a cluster, an edge host, or a cluster group has when it runs what it holds.
fn normal_state(state: &str) -> bool {
    matches!(state, "Running" | "in-use" | "active")
}

/// `; the cluster is Deleting`: the state of the object of a record, when it is not the state of
/// one that runs what it holds, and the object is not gone. Nothing for a profile record.
pub(crate) fn state_suffix(v: &Verify, recorded: &Source) -> String {
    match (&recorded.cluster, &v.state) {
        (Some(c), Some(state)) if !v.gone && !normal_state(state) => {
            format!("; the {} is {state}", c.kind.noun())
        }
        _ => String::new(),
    }
}

/// What a verdict names when the object of a record is gone: the cluster, the edge host, or the
/// cluster group itself, and not the profile it ran; or the published version.
fn gone_subject(recorded: &Source) -> String {
    match &recorded.cluster {
        Some(c) => format!("{} {}", c.kind.noun(), c.name),
        None => subject(recorded),
    }
}

/// `cluster c no longer exists: deleted <time>` when Palette keeps the document of the deleted
/// object, else `... no longer exists: uid u is gone from Palette since the record of <time>`.
/// `code` puts the uid in the form of the report.
pub(crate) fn gone_sentence(v: &Verify, recorded: &Source, code: fn(&str) -> String) -> String {
    let what = gone_subject(recorded);
    match &v.deleted_at {
        Some(at) => format!("{what} no longer exists: deleted {at}"),
        None => format!(
            "{what} no longer exists: uid {} is gone from Palette since the record of {}",
            code(&v.uid),
            recorded.fetched_at
        ),
    }
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
