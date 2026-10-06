//! One error type for the library. The CLI maps it to an exit status and a message; library users
//! can match on the variant.

use std::path::PathBuf;

use serde_json::{Map, Value};

/// Everything that can go wrong in the library.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No API key in the environment or in `~/.palette.env`.
    #[error(
        "{primary} is required to fetch from Palette (or {alternate}, or a line in ~/.palette.env)"
    )]
    MissingCredential {
        /// The preferred variable name.
        primary: &'static str,
        /// The accepted alternate variable name.
        alternate: &'static str,
    },
    /// Palette answered with a non-2xx status. A 401 or a 403 is almost always the key, so the
    /// message names where a key comes from; the body alone names nothing the reader can set.
    #[error("{method} {path} failed with HTTP {status}: {}{}", first_part(body), if *status == 401 || *status == 403 {
        "; check PALETTE_API_KEY, SPECTROCLOUD_APIKEY, or ~/.palette.env: the key can be wrong, expired, or for another tenant"
    } else {
        ""
    })]
    Api {
        /// The request method: `GET`, `POST`, `PUT`, `PATCH`, or `DELETE`.
        method: String,
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
        "GET {path} failed with HTTP {status} and Retry-After {retry_after}s, longer than the {limit}s strata waits (STRATA_MAX_RETRY_AFTER); try again later"
    )]
    RetryAfterTooLong {
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
    #[error("GET {path}: {message}")]
    Transport {
        /// Request path.
        path: String,
        /// What the transport said.
        message: String,
    },
    /// Palette answered 2xx with a body that is not JSON.
    #[error("GET {path}: response is not JSON: {source}")]
    NotJson {
        /// Request path.
        path: String,
        /// The parse error.
        source: serde_json::Error,
    },
    /// `name@version` matched nothing in the current scope. The message points at `strata list`,
    /// because the name is often a pack's rather than a profile's. At tenant scope it says so
    /// first: tenant scope holds only the profiles of the tenant, so a profile of a project is
    /// not there, and the reader needs a project before any other advice helps.
    #[error("no cluster profile named '{name}' with version '{version}' in this scope; {}", profile_not_found_hint(name, *tenant_scope))]
    ProfileNotFound {
        /// Profile name.
        name: String,
        /// Profile version.
        version: String,
        /// True when no project was set, so the scope holds only the profiles of the tenant.
        tenant_scope: bool,
    },
    /// `name@version` matched two or more versions in the current scope: a profile of the tenant
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
    #[error("cluster '{cluster}' has no applied profile named '{profile}'; applied: {}", applied.join(", "))]
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
    #[error("Palette refuses '{reference}' with {} {}; strata wrote no record", errors.len(), if errors.len() == 1 { "error" } else { "errors" })]
    PaletteRefuses {
        /// The ref or the candidate file, as given.
        reference: String,
        /// Each error that Palette gave, sorted.
        errors: Vec<crate::validation::PaletteRefusal>,
    },
    /// A candidate file does not parse or breaks a rule of the format.
    #[error("'{}': {message}", path.display())]
    CandidateInvalid {
        /// The candidate file.
        path: PathBuf,
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
    #[error("no registry named '{registry}' is visible; registries: {}", available.join(", "))]
    RegistryNotFound {
        /// The registry name asked for.
        registry: String,
        /// The registry names that are visible.
        available: Vec<String>,
    },
    /// The file exists but is not a strata snapshot.
    #[error("'{0}' is not a strata snapshot")]
    NotSnapshot(PathBuf),
    /// The file is a strata snapshot of a schema version this strata does not read.
    #[error("'{}' is a {found} record; this strata reads {supported}", path.display())]
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

/// The first 300 characters of a body, for a message. The field keeps the whole body.
fn first_part(body: &str) -> String {
    if body.chars().count() <= 300 {
        body.to_string()
    } else {
        format!("{}…", body.chars().take(300).collect::<String>())
    }
}

/// The codes of the refusals that Palette answers with HTTP 500. Palette gives these errors the
/// response type of a server fault, so a refusal by design arrives as a 5xx. A retry gets the
/// same refusal.
pub const REFUSAL_CODES: [&str; 7] = [
    "DeletionResourceInUseError",
    "ResOperationForbidden",
    "OperationForbidden",
    "ClusterProfileCreationValidationFailure",
    "ClusterProfileUpdateValidationFailure",
    "ClusterProfileInvalidPackState",
    "InvalidLimit",
];

/// The codes of the refusals of the whole request that `POST
/// /v1/spectroclusters/{uid}/validate/packs` answers with HTTP 500: the checks of the YAML, of
/// the metadata of the packs, of the integration packs, and of their OIDC identity provider.
/// `compare --validate` reads them as the answer of Palette. A retry gets the same refusal.
pub const PACK_REFUSAL_CODES: [&str; 11] = [
    "PackValuesYamlInvalid",
    "PackManifestYamlInvalid",
    "ClusterDashboardPacksMutuallyExclusive",
    "InvalidName",
    "DuplicatePacksForbidden",
    "DuplicatePackManifestsForbidden",
    "DuplicateClusterPacksForbidden",
    "MultiPackVersionsForbidden",
    "PackSpectroIntegrationNotSupported",
    "ClusterProfileOidcNotConfigured",
    "DisabledPacksForbidden",
];

/// The codes of the refusals of the whole request that `POST /v1/clusterprofiles/validate/packs`
/// and `POST /v1/clusterprofiles/{uid}/validate/packs` answer with HTTP 500: the six checks that
/// Palette does before it checks each pack. `snapshot --validate` reads them as the answer of
/// Palette. A retry gets the same refusal.
pub const PROFILE_PACK_REFUSAL_CODES: [&str; 6] = [
    "ClusterDashboardPacksMutuallyExclusive",
    "PackValuesYamlInvalid",
    "PackManifestYamlInvalid",
    "DisabledPacksForbidden",
    "SubResourceLimitValidationFailure",
    "ClusterProfilePacksInConflict",
];

/// The codes of the refusals that `GET /v1/clusterprofiles/validate/name` answers with HTTP 500:
/// an empty name, and a version that Palette does not parse. `snapshot --validate` reads them as
/// the answer of Palette. A retry gets the same refusal.
pub(crate) const PROFILE_NAME_REFUSAL_CODES: [&str; 2] =
    ["ResourceNameEmpty", "VersionParseFailure"];

/// The code of the refusal of a profile name and version that the project already holds. The
/// name route and the import validate route answer it with HTTP 409.
pub(crate) const PROFILE_EXISTS_CODE: &str = "ClusterProfileAlreadyExists";

/// The codes of the refusals of the whole request that `POST /v1/clusterprofiles/import/validate`
/// answers with HTTP 500. The route checks the limits, the cloud type, the metadata of the packs,
/// and the YAML. It also checks the integration packs, the name, the version, and each pack.
/// `snapshot --validate` reads them as the answer of Palette. A retry gets the same refusal.
pub const PROFILE_IMPORT_REFUSAL_CODES: [&str; 15] = [
    "ResourceLimitValidationFailure",
    "SubResourceLimitValidationFailure",
    "UnSupportedCloudType",
    "ClusterDashboardPacksMutuallyExclusive",
    "InvalidName",
    "DuplicatePacksForbidden",
    "DuplicatePackManifestsForbidden",
    "MultiPackVersionsForbidden",
    "PackValuesYamlInvalid",
    "PackManifestYamlInvalid",
    "PackSpectroIntegrationNotSupported",
    "ClusterProfileOidcNotConfigured",
    "ResourceNameEmpty",
    "VersionParseFailure",
    "ClusterProfileCreationValidationFailure",
];

/// The code of the refusal of a macro that a pack uses and that Palette does not define. The
/// import validate route answers it with HTTP 404.
pub(crate) const PACK_MACRO_UNDEFINED_CODE: &str = "PackMacroUndefined";

/// The codes of the errors that the import validate route gives with HTTP 500 when it cannot
/// match a pack of the body to a registry. The route then checks nothing, so strata reads them as
/// errors and not as an answer. A retry gets the same error.
pub(crate) const PROFILE_IMPORT_MATCH_CODES: [&str; 4] = [
    "ClusterProfileImportRegistriesNotFound",
    "ClusterProfileImportOciRegistriesNotFound",
    "ClusterProfileImportPrivateHelmRegistriesNotFound",
    "ClusterProfileImportPackNotFound",
];

/// The code of the one refusal of `validate/packs` that Palette answers with HTTP 403 and the
/// response type `Forbidden`, and not with 500. Palette gives it when a pack that an integration
/// pack needs read-only has values other than its defaults. `compare --validate` reads it as the
/// answer of Palette.
pub const PACK_FORBIDDEN_CODE: &str = "PackValuesOverrideForbidden";

/// The `code` of the body of a Palette error, when the body is JSON with one.
pub fn error_code(body: &str) -> Option<String> {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.get("code").and_then(Value::as_str).map(str::to_owned))
}

/// True when the body of a Palette error is JSON with a `code` in [`REFUSAL_CODES`],
/// [`PACK_REFUSAL_CODES`], [`PROFILE_PACK_REFUSAL_CODES`], [`PROFILE_NAME_REFUSAL_CODES`],
/// [`PROFILE_IMPORT_REFUSAL_CODES`], or [`PROFILE_IMPORT_MATCH_CODES`].
fn is_refusal(body: &str) -> bool {
    error_code(body).is_some_and(|code| {
        let code = code.as_str();
        REFUSAL_CODES.contains(&code)
            || PACK_REFUSAL_CODES.contains(&code)
            || PROFILE_PACK_REFUSAL_CODES.contains(&code)
            || PROFILE_NAME_REFUSAL_CODES.contains(&code)
            || PROFILE_IMPORT_REFUSAL_CODES.contains(&code)
            || PROFILE_IMPORT_MATCH_CODES.contains(&code)
    })
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
        "this is tenant scope, which holds only the profiles of the tenant: set PALETTE_PROJECT, \
         or give --project-name, and `strata list projects` shows the projects this key can see"
            .to_string()
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
    match quoted.as_slice() {
        [] => String::new(),
        [one] => format!("; Palette does not sync the registry {one}, and {UNSYNCED_LISTS}"),
        [first @ .., last] => format!(
            "; Palette does not sync the registries {} and {last}, and {UNSYNCED_LISTS}",
            first.join(", ")
        ),
    }
}

/// Put the registries that Palette does not sync into the document of an error, when there are
/// any.
fn put_unsynced(m: &mut serde_json::Map<String, Value>, unsynced: &[String]) {
    if !unsynced.is_empty() {
        m.insert("unsynced".into(), Value::from(unsynced.to_vec()));
    }
}

impl Error {
    /// The process exit status the CLI uses for this error. Every error is 2 today; the method
    /// exists so a future distinction (for example, 3 for "cannot reach Palette") lands in one place.
    pub fn exit_code(&self) -> u8 {
        2
    }

    /// How a caller treats the failure when it decides to try again. [`crate::api::Retry`]
    /// decides with the same rule.
    pub fn class(&self) -> FailureClass {
        match self {
            Error::Transport { .. } | Error::RetryAfterTooLong { .. } => FailureClass::Transient,
            // A refusal by design that Palette answers with a 5xx stays while the inputs stay.
            Error::Api { status, body, .. } if (500..600).contains(status) && is_refusal(body) => {
                FailureClass::Terminal
            }
            Error::Api { status, .. } if *status == 429 || (500..600).contains(status) => {
                FailureClass::Transient
            }
            // A 2xx that is not JSON can be a gateway page or a real fault, and a file or a
            // serializer that fails says nothing about the next try.
            Error::NotJson { .. } | Error::NoUid | Error::Io { .. } | Error::Serialize(_) => {
                FailureClass::Unknown
            }
            // Every other failure stays while the inputs stay: another HTTP status, a thing that
            // is not there, a file or a setting that is not valid.
            _ => FailureClass::Terminal,
        }
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
            Error::ProjectNotFound { .. } => "project-not-found",
            Error::ClusterNotFound { .. } => "cluster-not-found",
            Error::ClusterNameAmbiguous { .. } => "cluster-name-ambiguous",
            Error::EdgeHostNotFound { .. } => "edge-host-not-found",
            Error::EdgeHostNameAmbiguous { .. } => "edge-host-name-ambiguous",
            Error::ClusterGroupNotFound { .. } => "cluster-group-not-found",
            Error::AppProfileNotFound { .. } => "app-profile-not-found",
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

    /// The machine-readable form: `kind`, `message`, and the variant's own fields (`path`,
    /// `status`, `retryAfter`, `limit`, `name`, `version`, `schema`, `variable`, `alternate`, `value`, `cluster`,
    /// `profile`, `applied`, `reference`, `tag`, `registry`, `state`, `available`, `errors`) when it has them.
    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("kind".into(), Value::from(self.kind()));
        m.insert("message".into(), Value::from(self.to_string()));
        let path = |m: &mut Map<String, Value>, p: &std::path::Path| {
            m.insert("path".into(), Value::from(p.display().to_string()));
        };
        match self {
            Error::MissingCredential { primary, alternate } => {
                m.insert("variable".into(), Value::from(*primary));
                m.insert("alternate".into(), Value::from(*alternate));
            }
            Error::RecordNotASource { path: p } => {
                m.insert("path".into(), Value::from(p.as_str()));
            }
            Error::PaletteRefuses { reference, errors } => {
                m.insert("reference".into(), Value::from(reference.as_str()));
                m.insert(
                    "errors".into(),
                    serde_json::to_value(errors).unwrap_or_default(),
                );
            }
            // the message carries the whole of it, and no field adds to that
            Error::Usage { .. } => {}
            Error::Api {
                method,
                path: p,
                status,
                ..
            } => {
                m.insert("method".into(), Value::from(method.as_str()));
                m.insert("path".into(), Value::from(p.as_str()));
                m.insert("status".into(), Value::from(*status));
            }
            Error::RetryAfterTooLong {
                path: p,
                status,
                retry_after,
                limit,
            } => {
                m.insert("path".into(), Value::from(p.as_str()));
                m.insert("status".into(), Value::from(*status));
                m.insert("retryAfter".into(), Value::from(*retry_after));
                m.insert("limit".into(), Value::from(*limit));
            }
            Error::Pattern { pattern, .. } => {
                m.insert("pattern".into(), Value::from(pattern.as_str()));
            }
            Error::InvalidSetting {
                variable, value, ..
            } => {
                m.insert("variable".into(), Value::from(*variable));
                m.insert("value".into(), Value::from(value.as_str()));
            }
            Error::Transport { path: p, .. } | Error::NotJson { path: p, .. } => {
                m.insert("path".into(), Value::from(p.as_str()));
            }
            Error::ClusterProfileNotFound {
                cluster,
                profile,
                applied,
            } => {
                m.insert("cluster".into(), Value::from(cluster.as_str()));
                m.insert("profile".into(), Value::from(profile.as_str()));
                m.insert("applied".into(), Value::from(applied.clone()));
            }
            Error::CandidateInvalid { path: p, .. } => path(&mut m, p),
            Error::PackNotFound {
                name,
                version,
                tag,
                registry,
                unsynced,
            } => {
                m.insert("name".into(), Value::from(name.as_str()));
                m.insert("version".into(), Value::from(version.as_str()));
                if let Some(t) = tag {
                    m.insert("tag".into(), Value::from(t.as_str()));
                }
                m.insert("registry".into(), Value::from(registry.as_str()));
                put_unsynced(&mut m, unsynced);
            }
            Error::PackUnavailable {
                name,
                version,
                registry,
                state,
            } => {
                m.insert("name".into(), Value::from(name.as_str()));
                m.insert("version".into(), Value::from(version.as_str()));
                m.insert("registry".into(), Value::from(registry.as_str()));
                m.insert("state".into(), Value::from(state.as_str()));
            }
            // no version and no registry was asked for, so the document names neither
            Error::PackNameNotFound { name, unsynced } => {
                m.insert("name".into(), Value::from(name.as_str()));
                put_unsynced(&mut m, unsynced);
            }
            Error::PackInSeveralRegistries {
                name,
                version,
                registries,
            } => {
                m.insert("name".into(), Value::from(name.as_str()));
                m.insert("version".into(), Value::from(version.as_str()));
                m.insert("registries".into(), Value::from(registries.clone()));
            }
            Error::RegistryNotFound {
                registry,
                available,
            } => {
                m.insert("registry".into(), Value::from(registry.as_str()));
                m.insert("available".into(), Value::from(available.clone()));
            }
            Error::ProfileRefRequired { reference } => {
                m.insert("reference".into(), Value::from(reference.as_str()));
            }
            Error::NoDraft { name, version, uid } => {
                m.insert("name".into(), Value::from(name.as_str()));
                m.insert("version".into(), Value::from(version.as_str()));
                m.insert("uid".into(), Value::from(uid.as_str()));
            }
            Error::ProfileNotFound {
                name,
                version,
                tenant_scope,
            } => {
                m.insert("name".into(), Value::from(name.as_str()));
                m.insert("version".into(), Value::from(version.as_str()));
                m.insert("tenantScope".into(), Value::from(*tenant_scope));
            }
            Error::ProfileNameAmbiguous {
                name,
                version,
                profiles,
            } => {
                m.insert("name".into(), Value::from(name.as_str()));
                m.insert("version".into(), Value::from(version.as_str()));
                m.insert(
                    "profiles".into(),
                    serde_json::to_value(profiles).unwrap_or_default(),
                );
            }
            Error::NotSnapshot(p) | Error::EmptyFile(p) => path(&mut m, p),
            Error::UnsupportedSchema {
                path: p,
                found,
                supported,
            } => {
                path(&mut m, p);
                m.insert("found".into(), Value::from(found.as_str()));
                m.insert("supported".into(), Value::from(*supported));
            }
            Error::SnapshotSchema {
                path: p, schema, ..
            } => {
                path(&mut m, p);
                m.insert("schema".into(), Value::from(*schema));
            }
            Error::Io { path: p, .. } | Error::FileNotJson { path: p, .. } => path(&mut m, p),
            Error::ProjectNotFound { name, available } => {
                m.insert("name".into(), Value::from(name.as_str()));
                m.insert("available".into(), Value::from(available.clone()));
            }
            Error::ClusterNotFound { cluster, available } => {
                m.insert("cluster".into(), Value::from(cluster.as_str()));
                m.insert("available".into(), Value::from(available.clone()));
            }
            Error::ClusterNameAmbiguous { name, uids }
            | Error::EdgeHostNameAmbiguous { name, uids }
            | Error::ClusterGroupNameAmbiguous { name, uids }
            | Error::FilterNameAmbiguous { name, uids } => {
                m.insert("name".into(), Value::from(name.as_str()));
                m.insert("uids".into(), Value::from(uids.clone()));
            }
            Error::EdgeHostNotFound {
                edge_host,
                available,
            } => {
                m.insert("edgeHost".into(), Value::from(edge_host.as_str()));
                m.insert("available".into(), Value::from(available.clone()));
            }
            Error::FilterNotFound { filter, available } => {
                m.insert("filter".into(), Value::from(filter.as_str()));
                m.insert("available".into(), Value::from(available.clone()));
            }
            Error::AppProfileNotFound {
                name,
                version,
                available,
            } => {
                m.insert("name".into(), Value::from(name.as_str()));
                m.insert("version".into(), Value::from(version.as_str()));
                m.insert("available".into(), Value::from(available.clone()));
            }
            Error::ClusterGroupNotFound {
                cluster_group,
                available,
            } => {
                m.insert("clusterGroup".into(), Value::from(cluster_group.as_str()));
                m.insert("available".into(), Value::from(available.clone()));
            }
            Error::NoUid | Error::Serialize(_) => {}
        }
        Value::Object(m)
    }
}

/// Result alias for the library.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    fn bad_json() -> serde_json::Error {
        serde_json::from_str::<serde_json::Value>("{").unwrap_err()
    }

    /// One value of every variant. The `match` in `kind` is exhaustive, so the compiler keeps the
    /// mapping total; this test keeps it distinct and keeps `to_json` in step with it.
    fn one_of_each() -> Vec<Error> {
        vec![
            Error::MissingCredential {
                primary: "PALETTE_API_KEY",
                alternate: "SPECTROCLOUD_APIKEY",
            },
            Error::Api {
                method: "GET".into(),
                path: "/v1/x".into(),
                status: 503,
                body: "busy".into(),
                retry_after: Some(2),
            },
            Error::InvalidSetting {
                variable: "STRATA_RETRIES",
                value: "lots".into(),
                expected: "a whole number",
            },
            Error::RetryAfterTooLong {
                path: "/v1/y".into(),
                status: 429,
                retry_after: 900,
                limit: 300,
            },
            Error::Pattern {
                pattern: "releases/**/*.json".into(),
                message: "matched no files".into(),
            },
            Error::Transport {
                path: "/v1/x".into(),
                message: "refused".into(),
            },
            Error::NotJson {
                path: "/v1/x".into(),
                source: bad_json(),
            },
            Error::ProfileNotFound {
                name: "aws-stack".into(),
                version: "9.9.9".into(),
                tenant_scope: false,
            },
            Error::ProfileNameAmbiguous {
                name: "aws-stack".into(),
                version: "1.1.1".into(),
                profiles: vec![
                    crate::scope::ScopedUid {
                        uid: "64e0000000000000000000c3".into(),
                        scope: Some(crate::scope::ProfileScope::Project),
                    },
                    crate::scope::ScopedUid {
                        uid: "64e4000000000000000000c3".into(),
                        scope: Some(crate::scope::ProfileScope::Tenant),
                    },
                ],
            },
            Error::PackNameNotFound {
                name: "no-such-pack".into(),
                unsynced: vec!["charts-auth".into()],
            },
            Error::ClusterProfileNotFound {
                cluster: "c1".into(),
                profile: "nope".into(),
                applied: vec!["infra".into(), "addons".into()],
            },
            Error::ProfileRefRequired {
                reference: "cluster:c1".into(),
            },
            Error::NoDraft {
                name: "aws-stack".into(),
                version: "1.1.1".into(),
                uid: "64e0000000000000000000c3".into(),
            },
            Error::PaletteRefuses {
                reference: "aws-stack.yaml".into(),
                errors: vec![crate::validation::PaletteRefusal {
                    pack: "kubernetes".into(),
                    code: "DependencyPackWithMinVersionRequired".into(),
                    message: "Pack 'cni-cilium-oss' is required with minimum compatible version \
                              '1.21.0'"
                        .into(),
                }],
            },
            Error::CandidateInvalid {
                path: "profile.yaml".into(),
                message: "a candidate needs at least one layer".into(),
            },
            Error::PackNotFound {
                name: "kubernetes".into(),
                version: "9.9.9".into(),
                tag: Some("2.x".into()),
                registry: "Public Repo".into(),
                unsynced: Vec::new(),
            },
            Error::PackUnavailable {
                name: "nginx".into(),
                version: "1.15.1".into(),
                registry: "Public Repo".into(),
                state: "disabled".into(),
            },
            Error::PackInSeveralRegistries {
                name: "csi-aws-ebs".into(),
                version: "1.65.0".into(),
                registries: vec!["Public Repo".into(), "Mirror".into()],
            },
            Error::RegistryNotFound {
                registry: "Nope".into(),
                available: vec!["Public Repo".into()],
            },
            Error::ProjectNotFound {
                name: "staging".into(),
                available: vec!["lab".into(), "prod".into()],
            },
            Error::ClusterNotFound {
                cluster: "nope".into(),
                available: vec![],
            },
            Error::ClusterNameAmbiguous {
                name: "twin".into(),
                uids: vec!["c7".into(), "c8".into()],
            },
            Error::EdgeHostNotFound {
                edge_host: "nope".into(),
                available: vec!["edge-1".into()],
            },
            Error::EdgeHostNameAmbiguous {
                name: "twin".into(),
                uids: vec!["e7".into(), "e8".into()],
            },
            Error::ClusterGroupNotFound {
                cluster_group: "nope".into(),
                available: vec!["group-1".into()],
            },
            Error::FilterNotFound {
                filter: "prod".into(),
                available: vec!["dev".into()],
            },
            Error::FilterNameAmbiguous {
                name: "twin".into(),
                uids: vec!["f7".into(), "f8".into()],
            },
            Error::AppProfileNotFound {
                name: "shop".into(),
                version: "9.9.9".into(),
                available: vec!["shop@1.0.0".into()],
            },
            Error::ClusterGroupNameAmbiguous {
                name: "twin".into(),
                uids: vec!["g7".into(), "g8".into()],
            },
            Error::UnsupportedSchema {
                path: "newer.json".into(),
                found: "strata-snapshot/2".into(),
                supported: "strata-snapshot/1",
            },
            Error::NotSnapshot("profile.json".into()),
            Error::SnapshotSchema {
                path: "old.json".into(),
                schema: "strata-snapshot/1",
                source: bad_json(),
            },
            Error::NoUid,
            Error::Io {
                path: "rel.json".into(),
                source: std::io::Error::other("denied"),
            },
            Error::EmptyFile("variables.json".into()),
            Error::FileNotJson {
                path: "profile.json".into(),
                source: bad_json(),
            },
            Error::Serialize(bad_json()),
        ]
    }

    #[test]
    fn a_pack_that_is_not_found_names_the_registries_that_palette_does_not_sync() {
        let e = Error::PackNameNotFound {
            name: "podinfo".into(),
            unsynced: vec!["charts-auth".into(), "zarf".into()],
        };
        assert_eq!(
            e.to_string(),
            format!(
                "no registry that this key can see offers a pack named 'podinfo'; Palette does \
                 not sync the registries 'charts-auth' and 'zarf', and {UNSYNCED_LISTS}. `strata \
                 list --pack podinfo` finds the profiles that carry one"
            )
        );
        assert_eq!(
            e.to_json()["unsynced"],
            serde_json::json!(["charts-auth", "zarf"])
        );
        // no version was asked for, so the message names none
        let e = Error::PackNotFound {
            name: "podinfo".into(),
            version: String::new(),
            tag: None,
            registry: "charts-auth".into(),
            unsynced: vec!["charts-auth".into()],
        };
        assert_eq!(
            e.to_string(),
            format!(
                "no pack 'podinfo' in registry 'charts-auth'; Palette does not sync the registry \
                 'charts-auth', and {UNSYNCED_LISTS}"
            )
        );
        let e = Error::PackNotFound {
            name: "podinfo".into(),
            version: "1.0.0".into(),
            tag: Some("1.x".into()),
            registry: "charts".into(),
            unsynced: Vec::new(),
        };
        assert_eq!(
            e.to_string(),
            "no pack 'podinfo' at version '1.0.0' with tag '1.x' in registry 'charts'"
        );
        assert!(e.to_json().get("unsynced").is_none());
    }

    #[test]
    fn every_variant_has_a_distinct_kind() {
        let all = one_of_each();
        let mut kinds: Vec<&str> = all.iter().map(Error::kind).collect();
        kinds.sort_unstable();
        kinds.dedup();
        assert_eq!(kinds.len(), all.len(), "kinds: {kinds:?}");
        for k in &kinds {
            assert!(
                k.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "kind {k} is not kebab-case"
            );
        }
    }

    #[test]
    fn to_json_carries_kind_message_and_the_variant_fields() {
        for e in one_of_each() {
            let j = e.to_json();
            assert_eq!(j["kind"], e.kind());
            assert_eq!(j["message"], e.to_string());
        }
        let by_kind = |k: &str| {
            one_of_each()
                .into_iter()
                .find(|e| e.kind() == k)
                .unwrap()
                .to_json()
        };
        assert_eq!(by_kind("api")["status"], 503);
        assert_eq!(by_kind("api")["path"], "/v1/x");
        assert_eq!(by_kind("missing-credential")["variable"], "PALETTE_API_KEY");
        assert_eq!(
            by_kind("missing-credential")["alternate"],
            "SPECTROCLOUD_APIKEY"
        );
        assert_eq!(by_kind("not-snapshot")["path"], "profile.json");
        assert_eq!(by_kind("profile-not-found")["name"], "aws-stack");
        let ambiguous = by_kind("profile-name-ambiguous");
        assert_eq!(ambiguous["version"], "1.1.1");
        assert_eq!(
            ambiguous["profiles"],
            serde_json::json!([
                {"uid": "64e0000000000000000000c3", "scope": "project"},
                {"uid": "64e4000000000000000000c3", "scope": "tenant"},
            ])
        );
        assert!(
            ambiguous["message"].as_str().unwrap().contains(
                "'aws-stack' with version '1.1.1': 64e0000000000000000000c3 (project), \
                 64e4000000000000000000c3 (tenant); give the uid"
            ),
            "{ambiguous}"
        );
        assert_eq!(by_kind("snapshot-schema")["schema"], "strata-snapshot/1");
        assert_eq!(by_kind("invalid-setting")["variable"], "STRATA_RETRIES");
        assert_eq!(by_kind("invalid-setting")["value"], "lots");
        assert_eq!(by_kind("retry-after-too-long")["path"], "/v1/y");
        assert_eq!(by_kind("retry-after-too-long")["status"], 429);
        assert_eq!(by_kind("retry-after-too-long")["retryAfter"], 900);
        assert_eq!(by_kind("retry-after-too-long")["limit"], 300);
        assert_eq!(by_kind("project-not-found")["name"], "staging");
        assert_eq!(by_kind("project-not-found")["available"][1], "prod");
        assert_eq!(by_kind("cluster-not-found")["cluster"], "nope");
        assert!(
            by_kind("cluster-not-found")["message"]
                .as_str()
                .unwrap()
                .ends_with("clusters: none")
        );
        assert_eq!(by_kind("cluster-name-ambiguous")["uids"][0], "c7");
        assert_eq!(by_kind("pattern")["pattern"], "releases/**/*.json");
        let cp = by_kind("cluster-profile-not-found");
        assert_eq!(
            (cp["cluster"].clone(), cp["profile"].clone()),
            ("c1".into(), "nope".into())
        );
        assert_eq!(cp["applied"], serde_json::json!(["infra", "addons"]));
        assert_eq!(by_kind("profile-ref-required")["reference"], "cluster:c1");
        assert_eq!(by_kind("candidate-invalid")["path"], "profile.yaml");
        assert_eq!(by_kind("unsupported-schema")["found"], "strata-snapshot/2");
        let pnf = by_kind("pack-not-found");
        assert_eq!(
            (pnf["name"].clone(), pnf["tag"].clone()),
            ("kubernetes".into(), "2.x".into())
        );
        assert_eq!(
            by_kind("registry-not-found")["available"],
            serde_json::json!(["Public Repo"])
        );
        assert_eq!(
            by_kind("pack-in-several-registries")["registries"],
            serde_json::json!(["Public Repo", "Mirror"])
        );
    }
}

/// The class of a failure, for a caller that decides to try again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FailureClass {
    /// Time can remove it: a transport error, HTTP 5xx, HTTP 429, or a `Retry-After` that is
    /// longer than strata waits.
    Transient,
    /// It stays while the inputs stay: another HTTP 4xx, a 5xx with a code in
    /// [`REFUSAL_CODES`], [`PACK_REFUSAL_CODES`], or [`PROFILE_PACK_REFUSAL_CODES`], a thing
    /// that is not there, a file or a setting that is not valid.
    Terminal,
    /// Strata cannot tell.
    Unknown,
}
