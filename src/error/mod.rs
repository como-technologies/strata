//! One error type for the library. The CLI maps it to an exit status and a message; library users
//! can match on the variant.

use std::path::PathBuf;

use crate::api::Method;
use crate::api::codes::CreateRule;
use crate::config::{KEY_VARS, MAX_RETRY_AFTER_VAR, PROJECT_NAME_VAR};
use crate::text::{and_list, cut};

mod class;
mod json;

pub(crate) use json::ErrorDocument;

pub use class::FailureClass;

/// Everything that can go wrong in the library.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No API key in the environment or in `~/.palette.env`.
    #[error(
        "{primary} is required to fetch from Palette (or {alternate}, or a line in ~/{})",
        crate::config::ENV_FILE
    )]
    MissingCredential {
        /// The preferred variable name.
        primary: &'static str,
        /// The accepted alternate variable name.
        alternate: &'static str,
    },
    /// Palette answered with a non-2xx status. A 401 or a 403 is almost always the key, so the
    /// message names where a key comes from; the body alone names nothing the reader can set.
    #[error("{method} {path} failed with HTTP {status}: {}{}", cut(body, BODY_IN_MESSAGE), key_hint(*status))]
    Api {
        /// The request method.
        method: Method,
        /// Request path.
        path: String,
        /// HTTP status code.
        status: u16,
        /// The response body, on one line. The message shows its first part; a caller that reads
        /// the answer of a validate route parses the whole of it.
        body: String,
        /// The `Retry-After` header in whole seconds, when Palette sent one.
        retry_after: Option<u64>,
    },
    /// Palette asked strata to wait longer than it will before retrying.
    #[error(
        "{method} {path} failed with HTTP {status} and Retry-After {retry_after}s, longer than the {limit}s strata waits ({}); try again later",
        MAX_RETRY_AFTER_VAR
    )]
    RetryAfterTooLong {
        /// The request method.
        method: Method,
        /// Request path.
        path: String,
        /// HTTP status code.
        status: u16,
        /// The wait Palette asked for, in whole seconds.
        retry_after: u64,
        /// The longest wait strata honors, in whole seconds.
        limit: u64,
    },
    /// A file pattern on the command line is invalid or matched nothing.
    #[error("'{pattern}' {message}")]
    Pattern {
        /// The pattern as given.
        pattern: String,
        /// What is wrong with it.
        message: String,
    },
    /// A setting from the environment has a value strata cannot use.
    #[error("{variable}: expected {expected}, got '{value}'")]
    InvalidSetting {
        /// The variable name.
        variable: &'static str,
        /// The value as given.
        value: String,
        /// What would have been accepted.
        expected: &'static str,
    },
    /// The request did not complete.
    #[error("{method} {path}: {message}")]
    Transport {
        /// The request method.
        method: Method,
        /// Request path.
        path: String,
        /// What the transport said.
        message: String,
    },
    /// Palette answered 2xx with a body that is not JSON.
    #[error("{method} {path}: response is not JSON: {source}")]
    NotJson {
        /// The request method.
        method: Method,
        /// Request path.
        path: String,
        /// The parse error.
        source: serde_json::Error,
    },
    /// `name@version` matched nothing in the current scope. The message points at `strata list`,
    /// because the name is often a pack's rather than a profile's. At tenant scope it says so
    /// first: tenant scope holds only the profiles of the tenant, so a profile of a project is
    /// not there. The reader needs a project before any other advice helps.
    #[error("no cluster profile named '{name}' with version '{version}' in this scope; {}", profile_not_found_hint(name, *tenant_scope))]
    ProfileNotFound {
        /// Profile name.
        name: String,
        /// Profile version.
        version: String,
        /// True when no project was set, so the scope holds only the profiles of the tenant.
        tenant_scope: bool,
    },
    /// `name@version` matched two or more versions in the current scope. A profile of the tenant
    /// and a profile of the project can have one name and one version. The message names each
    /// uid with its scope, so that the reader can give the uid in place of the name.
    #[error(
        "several profiles in this scope are named '{name}' with version '{version}': {}; give the uid in place of the name",
        profiles.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
    )]
    ProfileNameAmbiguous {
        /// Profile name.
        name: String,
        /// The version that matched: the version of the ref, or the highest version for `latest`.
        version: String,
        /// Each version with that name and version, by uid, with its scope.
        profiles: Vec<crate::scope::ScopedUid>,
    },
    /// `cluster:<uid>/<profile>` named a profile the cluster does not run.
    #[error(
        "cluster '{cluster}' has no applied profile named '{profile}'; applied: {}",
        list_or_none(applied)
    )]
    ClusterProfileNotFound {
        /// Cluster uid.
        cluster: String,
        /// The profile name asked for.
        profile: String,
        /// The names of the profiles applied to the cluster, in order.
        applied: Vec<String>,
    },
    /// A project name matches no project the key can see.
    #[error(
        "no project named '{name}' that this key can see; projects: {}",
        list_or_none(available)
    )]
    ProjectNotFound {
        /// The name asked for.
        name: String,
        /// The names of the projects the key can see, in order.
        available: Vec<String>,
    },
    /// `cluster:<x>` is neither a cluster uid nor the name of a cluster in the scope.
    #[error(
        "no cluster with the uid or name '{cluster}' in this scope; clusters: {}",
        list_or_none(available)
    )]
    ClusterNotFound {
        /// The uid or name as given.
        cluster: String,
        /// The names of the clusters in the scope, in order.
        available: Vec<String>,
    },
    /// `cluster:<name>` matches more than one cluster in the scope.
    #[error("several clusters in this scope are named '{name}': {}; use cluster:<uid>", uids.join(", "))]
    ClusterNameAmbiguous {
        /// The name as given.
        name: String,
        /// The uids of the clusters with that name, in order.
        uids: Vec<String>,
    },
    /// `edgehost:<x>` is neither an edge host uid nor the name of an edge host in the scope.
    #[error(
        "no edge host with the uid or name '{edge_host}' in this scope; edge hosts: {}",
        list_or_none(available)
    )]
    EdgeHostNotFound {
        /// The uid or name as given.
        edge_host: String,
        /// The names of the edge hosts in the scope, in order.
        available: Vec<String>,
    },
    /// `edgehost:<name>` matches more than one edge host in the scope.
    #[error("several edge hosts in this scope are named '{name}': {}; use edgehost:<uid>", uids.join(", "))]
    EdgeHostNameAmbiguous {
        /// The name as given.
        name: String,
        /// The uids of the edge hosts with that name, in order.
        uids: Vec<String>,
    },
    /// A saved tag filter that no filter of the tenant has as its name or uid.
    #[error(
        "no saved tag filter with the name or uid '{filter}'; filters: {}",
        list_or_none(available)
    )]
    FilterNotFound {
        /// The name or uid as given.
        filter: String,
        /// The names of the saved tag filters, in order.
        available: Vec<String>,
    },
    /// A name that two or more saved tag filters have.
    #[error("several saved tag filters are named '{name}': {}; use filter:<uid>", uids.join(", "))]
    FilterNameAmbiguous {
        /// The name as given.
        name: String,
        /// The uids of the filters with that name.
        uids: Vec<String>,
    },
    /// `clustergroup:<x>` is neither a cluster group uid nor the name of a cluster group in the
    /// scope.
    #[error(
        "no cluster group with the uid or name '{cluster_group}' in this scope; cluster groups: {}",
        list_or_none(available)
    )]
    ClusterGroupNotFound {
        /// The uid or name as given.
        cluster_group: String,
        /// The names of the cluster groups in the scope, in order.
        available: Vec<String>,
    },
    /// `clustergroup:<name>` matches more than one cluster group in the scope.
    #[error("several cluster groups in this scope are named '{name}': {}; use clustergroup:<uid>", uids.join(", "))]
    ClusterGroupNameAmbiguous {
        /// The name as given.
        name: String,
        /// The uids of the cluster groups with that name, in order.
        uids: Vec<String>,
    },
    /// `appprofile:<name>@<version>` matched no app profile in the scope.
    #[error(
        "no app profile named '{name}' with version '{version}' in this scope; app profiles: {}",
        list_or_none(available)
    )]
    AppProfileNotFound {
        /// The name as given.
        name: String,
        /// The version as given.
        version: String,
        /// Each app profile of the scope, as `name@version`, in order.
        available: Vec<String>,
    },
    /// `appprofile:<name>@<version>` matched two or more app profiles in the scope. Palette
    /// refuses a second app profile with one name and one version only in one scope.
    #[error(
        "several app profiles in this scope are named '{name}' with version '{version}': {}; use appprofile:<uid>",
        uids.join(", ")
    )]
    AppProfileNameAmbiguous {
        /// The name as given.
        name: String,
        /// The version that matched: the version as given, or the highest version for `latest`.
        version: String,
        /// The uids of the app profiles with that name and version, in order.
        uids: Vec<String>,
    },
    /// A command that works on a profile was given a cluster, an edge host, a cluster group, an
    /// app profile, a pack, or a record of one.
    #[error(
        "'{reference}' is not a profile version; this command takes a profile uid, <name>@<version>, or a profile record"
    )]
    ProfileRefRequired {
        /// The ref as given.
        reference: String,
    },
    /// `<ref>@draft` named a profile version that has no draft: Palette holds only its published
    /// template.
    #[error(
        "profile {name} version {version} (uid {uid}) has no draft: Palette holds only its published template, which `strata snapshot {uid}` reads"
    )]
    NoDraft {
        /// The profile name.
        name: String,
        /// The profile version.
        version: String,
        /// The profile uid.
        uid: String,
    },
    /// The command was called in a way that cannot work, and no request was made.
    #[error("{message}")]
    Usage {
        /// What the caller must do instead.
        message: String,
    },
    /// `snapshot` was given a record file. Every other command reads one; `snapshot` writes one.
    #[error(
        "'{path}' is a record; snapshot writes a record, and does not read one. To check it against Palette, run: strata verify {path}"
    )]
    RecordNotASource {
        /// The path as given.
        path: String,
    },
    /// `snapshot --validate` asked Palette whether it takes the version, and Palette refused it:
    /// its name, its version, or its packs. Strata wrote no record. The CLI prints each error
    /// before this message.
    #[error("Palette refuses '{reference}' with {} {}; strata wrote no record", errors.len(), crate::text::plural(errors.len(), "error", "errors"))]
    PaletteRefuses {
        /// The ref or the candidate file, as given.
        reference: String,
        /// Each error that Palette gave, sorted.
        errors: Vec<crate::validation::PaletteRefusal>,
    },
    /// A candidate file does not parse, breaks a rule of the format, or breaks a rule that only
    /// the create of Palette checks. For such a rule, the message ends with the code of Palette
    /// and with why strata checks it.
    #[error("'{}': {message}{}", path.display(), rule_sentence(*rule))]
    CandidateInvalid {
        /// The candidate file.
        path: PathBuf,
        /// The rule of the create that the candidate breaks, when the problem is one.
        /// [`to_json`](Error::to_json) gives its code as `code`.
        rule: Option<CreateRule>,
        /// What is wrong.
        message: String,
    },
    /// A candidate layer names a pack version, or a tag, that its registry does not have.
    #[error("no pack '{name}'{}{} in registry '{registry}'{}", at_version(version), with_tag(tag.as_deref()), unsynced_sentence(unsynced))]
    PackNotFound {
        /// Pack name.
        name: String,
        /// Pack version. Empty when the ref and the flags give no version, as with
        /// `packs --registry`.
        version: String,
        /// The tag asked for, when one was.
        tag: Option<String>,
        /// The registry: its name, or the text that the candidate or the ref gave.
        registry: String,
        /// The registries that Palette does not sync, among the ones that strata asked. Palette
        /// lists a pack of such a registry only while a profile pins the pack.
        unsynced: Vec<String>,
    },
    /// The registry lists a pack version that Spectro Cloud disabled or deleted. Palette serves no
    /// such pack, and refuses a profile that pins one.
    #[error(
        "pack '{name}' at version '{version}' is {state} in registry '{registry}': Palette does not serve it, and refuses a profile that pins it; `strata packs {name}` lists the versions and their states"
    )]
    PackUnavailable {
        /// Pack name.
        name: String,
        /// Pack version.
        version: String,
        /// The registry, as the ref or the candidate names it.
        registry: String,
        /// The state that the registry listing gives: `disabled` or `deleted`.
        state: String,
    },
    /// `packs <name>` found the name in no visible registry. It asked for every version, so no
    /// version and no registry was ever in play, and `PackNotFound` would state both as blanks.
    #[error(
        "no registry that this key can see offers a pack named '{name}'{}. `strata list --pack {name}` finds the profiles that carry one",
        unsynced_sentence(unsynced)
    )]
    PackNameNotFound {
        /// Pack name.
        name: String,
        /// The visible registries that Palette does not sync. Palette lists a pack of such a
        /// registry only while a profile pins the pack.
        unsynced: Vec<String>,
    },
    /// A pack version is in more than one visible registry, and the ref named none.
    #[error("pack '{name}' at version '{version}' is in several registries: {}; name one, as in pack:<registry>/{name}@{version}", registries.join(", "))]
    PackInSeveralRegistries {
        /// Pack name.
        name: String,
        /// Pack version.
        version: String,
        /// The registries that have it, by name.
        registries: Vec<String>,
    },
    /// A candidate names a registry that is not visible with this key.
    #[error(
        "no registry named '{registry}' is visible; registries: {}",
        list_or_none(available)
    )]
    RegistryNotFound {
        /// The registry name asked for.
        registry: String,
        /// The registry names that are visible.
        available: Vec<String>,
    },
    /// A name that two or more visible registries have. A registry of the system can have the
    /// name of a registry of the tenant.
    #[error("several registries are named '{name}': {}; give the uid of one in place of the name", uids.join(", "))]
    RegistryNameAmbiguous {
        /// The name as given.
        name: String,
        /// The uids of the registries with that name, in order.
        uids: Vec<String>,
    },
    /// The file exists but is not a strata snapshot.
    #[error("'{0}' is not a strata snapshot")]
    NotSnapshot(PathBuf),
    /// The file is a strata snapshot of a schema version this strata does not read.
    #[error("'{}' is a {found} document; this strata reads {supported}", path.display())]
    UnsupportedSchema {
        /// File path.
        path: PathBuf,
        /// The schema id in the file.
        found: String,
        /// The schema id this strata reads.
        supported: &'static str,
    },
    /// The file claims the snapshot schema but does not match it.
    #[error("'{path}' does not match {schema}: {source}")]
    SnapshotSchema {
        /// File path.
        path: PathBuf,
        /// The schema id expected.
        schema: &'static str,
        /// The deserialization error.
        source: serde_json::Error,
    },
    /// A snapshot without `source.uid` cannot be verified.
    #[error("snapshot has no source.uid to verify against")]
    NoUid,
    /// A file could not be read or written.
    #[error("{path}: {source}")]
    Io {
        /// File path.
        path: PathBuf,
        /// The I/O error.
        source: std::io::Error,
    },
    /// A raw API response file is empty.
    #[error("{0} is empty")]
    EmptyFile(PathBuf),
    /// A raw API response file is not JSON.
    #[error("{path} is not JSON: {source}")]
    FileNotJson {
        /// File path.
        path: PathBuf,
        /// The parse error.
        source: serde_json::Error,
    },
    /// A document could not be serialized.
    #[error("serializing: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// The hint of a 401 or a 403: the key is almost always the cause, so the message names where a
/// key comes from. Empty for each other status.
fn key_hint(status: u16) -> String {
    if status == 401 || status == 403 {
        format!(
            "; check {}, {}, or ~/{}: the key can be wrong, expired, or for another tenant",
            KEY_VARS[0],
            KEY_VARS[1],
            crate::config::ENV_FILE
        )
    } else {
        String::new()
    }
}

/// The most characters of a body that the message of [`Error::Api`] shows. The field keeps the
/// whole body.
const BODY_IN_MESSAGE: usize = 300;

/// What strata says after a rule that only the create of Palette checks: why strata checks it.
const CREATE_GAP: &str = "Palette checks this only at the create, and no route that takes a \
                          candidate answers it before the create. Strata removes this check when \
                          a route answers it";

/// The end of the message of a `CandidateInvalid` error for a rule of the create: the code of
/// Palette, and why strata checks it. Empty for a problem of the format.
fn rule_sentence(rule: Option<CreateRule>) -> String {
    rule.map_or_else(String::new, |rule| {
        format!(" ({}). {CREATE_GAP}", rule.code())
    })
}

/// The reason that a record keeps for an answer of Palette that is an error:
/// `HTTP <status>: <body>`. Palette ends a body with a newline, so the body comes with no blank
/// at its ends.
pub(crate) fn unread_reason(status: u16, body: &str) -> String {
    format!("HTTP {status}: {}", body.trim())
}

/// `answer`, or `None` when Palette answered HTTP 404: it does not have the object. Each other
/// error returns.
pub(crate) fn found<T>(answer: Result<T>) -> Result<Option<T>> {
    match answer {
        Ok(v) => Ok(Some(v)),
        Err(Error::Api { status: 404, .. }) => Ok(None),
        Err(e) => Err(e),
    }
}

/// `answer`, or `None` when Palette has no such route: see [`Error::is_missing_route`]. Each
/// other error returns, also a 401, a 403, or a 429: the key or the rate, and not the route.
pub(crate) fn served<T>(answer: Result<T>) -> Result<Option<T>> {
    match answer {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.is_missing_route() => Ok(None),
        Err(e) => Err(e),
    }
}

/// `a, b, c`, or `none` for an empty list.
fn list_or_none(items: &[String]) -> String {
    if items.is_empty() {
        "none".to_string()
    } else {
        items.join(", ")
    }
}

/// What to try after a profile name matched nothing. At tenant scope the listing advice is a dead
/// end for a profile of a project: tenant scope holds only the profiles of the tenant, so the
/// reader needs a project first.
fn profile_not_found_hint(name: &str, tenant_scope: bool) -> String {
    if tenant_scope {
        format!(
            "this is tenant scope, which holds only the profiles of the tenant: set {PROJECT_NAME_VAR}, \
             or give --project-name, and `strata list projects` shows the projects this key can see"
        )
    } else {
        format!(
            "`strata list` shows the profiles in it, and `strata list --pack {name}` the profiles \
             with a pack of that name"
        )
    }
}

/// ` at version '1.2.3'`, or nothing when the ref and the flags give no version.
fn at_version(version: &str) -> String {
    if version.is_empty() {
        String::new()
    } else {
        format!(" at version '{version}'")
    }
}

/// ` with tag '2.x'`, or nothing when the ref and the flags give no tag.
fn with_tag(tag: Option<&str>) -> String {
    tag.map(|t| format!(" with tag '{t}'")).unwrap_or_default()
}

/// Why a registry that Palette does not sync has no pack, in the message of
/// [`Error::PackNotFound`] and [`Error::PackNameNotFound`]. Palette makes the pack of such a
/// registry when it creates the profile, so its listing has the pack for that profile only.
pub const UNSYNCED_LISTS: &str =
    "lists a pack of such a registry only while a profile pins the pack";

/// Why the registries of `registries` have no pack of a name, for the message of an error:
/// `; Palette does not sync the registry 'charts-auth', and lists a pack of such a registry only
/// while a profile pins the pack`. Nothing when there is no such registry.
fn unsynced_sentence(registries: &[String]) -> String {
    let quoted: Vec<String> = registries.iter().map(|r| format!("'{r}'")).collect();
    let noun = match quoted.len() {
        0 => return String::new(),
        1 => "registry",
        _ => "registries",
    };
    format!(
        "; Palette does not sync the {noun} {}, and {UNSYNCED_LISTS}",
        and_list(&quoted)
    )
}

impl Error {
    /// The process exit status the CLI uses for this error. Every error is 2 today; the method
    /// exists so a future distinction (for example, 3 for "cannot reach Palette") lands in one place.
    pub fn exit_code(&self) -> u8 {
        2
    }

    /// True for an answer of Palette that says it has no such route: HTTP 404 or 405. An older
    /// Palette answers so for a route that a later one added.
    pub fn is_missing_route(&self) -> bool {
        matches!(
            self,
            Error::Api {
                status: 404 | 405,
                ..
            }
        )
    }

    /// A stable kebab-case name for the variant, for programs that read errors. The `match` is
    /// exhaustive, so a new variant cannot be added without naming it here.
    pub fn kind(&self) -> &'static str {
        match self {
            Error::MissingCredential { .. } => "missing-credential",
            Error::Api { .. } => "api",
            Error::RetryAfterTooLong { .. } => "retry-after-too-long",
            Error::InvalidSetting { .. } => "invalid-setting",
            Error::Pattern { .. } => "pattern",
            Error::Transport { .. } => "transport",
            Error::NotJson { .. } => "not-json",
            Error::ProfileNotFound { .. } => "profile-not-found",
            Error::ProfileNameAmbiguous { .. } => "profile-name-ambiguous",
            Error::ClusterProfileNotFound { .. } => "cluster-profile-not-found",
            Error::ProfileRefRequired { .. } => "profile-ref-required",
            Error::NoDraft { .. } => "no-draft",
            Error::RecordNotASource { .. } => "record-not-a-source",
            Error::PaletteRefuses { .. } => "palette-refuses",
            Error::Usage { .. } => "usage",
            Error::CandidateInvalid { .. } => "candidate-invalid",
            Error::PackNotFound { .. } => "pack-not-found",
            Error::PackNameNotFound { .. } => "pack-name-not-found",
            Error::PackUnavailable { .. } => "pack-unavailable",
            Error::PackInSeveralRegistries { .. } => "pack-in-several-registries",
            Error::RegistryNotFound { .. } => "registry-not-found",
            Error::RegistryNameAmbiguous { .. } => "registry-name-ambiguous",
            Error::ProjectNotFound { .. } => "project-not-found",
            Error::ClusterNotFound { .. } => "cluster-not-found",
            Error::ClusterNameAmbiguous { .. } => "cluster-name-ambiguous",
            Error::EdgeHostNotFound { .. } => "edge-host-not-found",
            Error::EdgeHostNameAmbiguous { .. } => "edge-host-name-ambiguous",
            Error::ClusterGroupNotFound { .. } => "cluster-group-not-found",
            Error::AppProfileNotFound { .. } => "app-profile-not-found",
            Error::AppProfileNameAmbiguous { .. } => "app-profile-name-ambiguous",
            Error::FilterNotFound { .. } => "filter-not-found",
            Error::FilterNameAmbiguous { .. } => "filter-name-ambiguous",
            Error::ClusterGroupNameAmbiguous { .. } => "cluster-group-name-ambiguous",
            Error::NotSnapshot(_) => "not-snapshot",
            Error::UnsupportedSchema { .. } => "unsupported-schema",
            Error::SnapshotSchema { .. } => "snapshot-schema",
            Error::NoUid => "no-uid",
            Error::Io { .. } => "io",
            Error::EmptyFile(_) => "empty-file",
            Error::FileNotJson { .. } => "file-not-json",
            Error::Serialize(_) => "serialize",
        }
    }
}

/// Result alias for the library.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests;
