//! The codes of the answers of Palette that strata reads, in one place.
//!
//! A validate route refuses the whole request with an HTTP error and a `code` before it checks
//! each pack. Strata reads such a refusal as an answer, and does not send the request again.
//! [`ValidateRoute`] names each route with the checks that it runs. Each check is one table of
//! codes. Two routes that run one check read one table, so each code is in one table.
//!
//! A rule that only the create of a profile version checks is a gap that strata fills.
//! [`CreateRule`] names each such rule with the code that the create gives.
//!
//! A refusal that says that the key has no access is a [`Denial`]. The key is right, and its role
//! does not give the access, so strata does not send the request again.
//!
//! Each comment states what Palette does, and not where its code does it.

use serde_json::Value;

use crate::api::Method;
use crate::closed_set::impl_texts;
use crate::model::validation::PaletteRefusal;

/// The codes of the refusals that Palette answers with HTTP 500 on a route that [`ValidateRoute`]
/// does not name.
///
/// The routes are the delete of an object, an operation on an object, and the update of a profile.
/// Two more are a listing and the property route of AWS. Palette gives these errors the response
/// type of a server fault, so an intended refusal arrives as a 5xx. A retry gets the same refusal.
///
/// The property route of AWS refuses a Pod Identity account with `FeatureNotSupported`, and a key
/// pair that the region lacks with `AwsKeyPairNotFound`. A live run shows the second answer. No
/// live run shows the first.
pub(crate) const REFUSAL_CODES: [&str; 7] = [
    "DeletionResourceInUseError",
    "ResOperationForbidden",
    "ClusterProfileUpdateValidationFailure",
    "ClusterProfileInvalidPackState",
    "InvalidLimit",
    "AwsKeyPairNotFound",
    "FeatureNotSupported",
];

/// The code of the refusal of one object to a key whose role does not give the permission.
///
/// Palette answers it with HTTP 500, and the message names the permission. A read of the role
/// bindings, the namespaces, or the repave of a cluster gets it when the role does not give
/// `cluster.get`. The code of Palette shows it; no live run shows it yet for these three routes.
pub const OPERATION_FORBIDDEN_CODE: &str = "OperationForbidden";

/// A refusal of Palette that says that the key has no access to the request.
///
/// Palette found the user of the key, and the roles of that user do not give the access. So the
/// key itself is right, and a retry gets the same refusal. Each variant is one code of Palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Denial {
    /// The route as a whole, such as the audit log, with HTTP 403. The message names the
    /// permission that the route needs, as in `required permissions 'audit.list'`.
    Route,
    /// One object, such as a profile, a cluster, or a registry, with HTTP 500. The message names
    /// the permission, as in `Operation 'cluster.get' is forbidden`.
    Object,
    /// No role of the user in the project of the request gives the access, with HTTP 500. The
    /// user has no role in the project, or a role without the permission, such as `cluster.list`
    /// for the cluster listing. The message names the project, as in `Project 'dev' access denied`.
    Project,
    /// The user has no role at all, with HTTP 500.
    NoRole,
}

impl_texts!(pub Denial {
    Route => "AccessDenied",
    Object => OPERATION_FORBIDDEN_CODE,
    Project => "ResourceAccessDenied",
    NoRole => "NoRolesAssociated",
});

impl Denial {
    /// The HTTP status that Palette gives the denial: 403 for [`Denial::Route`]. Palette gives each
    /// other denial the response type of a server fault, HTTP 500.
    pub fn http_status(self) -> u16 {
        match self {
            Denial::Route => 403,
            Denial::Object | Denial::Project | Denial::NoRole => 500,
        }
    }

    /// The permission that `message`, the message of Palette, names: its first text in single
    /// quotes. `None` for a denial that names no permission, and for a message with no quotes.
    pub fn permission(self, message: &str) -> Option<&str> {
        if !matches!(self, Denial::Route | Denial::Object) {
            return None;
        }
        let (_, rest) = message.split_once('\'')?;
        let (permission, _) = rest.split_once('\'')?;
        Some(permission).filter(|p| !p.is_empty())
    }

    /// What the denial says about the key, for the message of an error. An example is `the role
    /// of the key does not give the permission 'filter.list'`. `message` is the message of Palette.
    pub fn sentence(self, message: &str) -> String {
        match (self, self.permission(message)) {
            (Denial::Route | Denial::Object, Some(permission)) => {
                format!("the role of the key does not give the permission '{permission}'")
            }
            (Denial::Route | Denial::Object, None) => {
                "the role of the key does not give the permission of the request".to_string()
            }
            (Denial::Project, _) => {
                "no role of the key in the project gives the access of the request".to_string()
            }
            (Denial::NoRole, _) => "the user of the key has no role".to_string(),
        }
    }
}

/// The code of the refusal of a profile name and version that the project already holds.
///
/// The name route and the import validate route answer it with HTTP 409.
pub const PROFILE_EXISTS_CODE: &str = "ClusterProfileAlreadyExists";

/// The code of the refusal of a macro that a pack uses and that Palette does not define.
///
/// The import validate route answers it with HTTP 404.
pub const PACK_MACRO_UNDEFINED_CODE: &str = "PackMacroUndefined";

/// The code of the one refusal of the cluster packs route with HTTP 403, and not with 500.
///
/// Its response type is `Forbidden`. Palette gives it when a pack that an
/// integration pack needs read-only has values other than its defaults. The cluster validate route
/// gives it too, with HTTP 403: see [`ValidateRoute::ClusterCreate`].
pub const PACK_FORBIDDEN_CODE: &str = "PackValuesOverrideForbidden";

/// The codes of the other refusals of the body that the cluster validate route answers with HTTP
/// 403, and not with 500.
///
/// Their response type is `Forbidden`, as for [`PACK_FORBIDDEN_CODE`]. The body has two system
/// profiles (`MultipleSystemProfilesNotSupported`), or a pack in the state `deleted`
/// (`ClusterOperationNotAllowedInvalidPackState`). Or Palette runs in FIPS mode, and the cloud does
/// not support it (`ClusterFipsNotSupported`). The code of Palette shows it; no live run shows it
/// yet.
pub const CLUSTER_CREATE_FORBIDDEN_CODES: [&str; 3] = [
    "MultipleSystemProfilesNotSupported",
    "ClusterOperationNotAllowedInvalidPackState",
    "ClusterFipsNotSupported",
];

/// The codes of the refusals of a create by the cluster validate route when the tenant is at a
/// limit, with HTTP 403.
///
/// A trial tenant over its usage is `FreemiumLimitExceeded`, and over its active clusters
/// `FreemiumActiveClusterLimitExceeded`. A tenant of the AWS Marketplace over its usage is
/// `MarketplaceUsageLimitExceeded`. The response type is `Forbidden`, and the key is right. The
/// code of Palette shows it; no live run shows it yet.
pub const CLUSTER_CREATE_LIMIT_CODES: [&str; 3] = [
    "FreemiumLimitExceeded",
    "FreemiumActiveClusterLimitExceeded",
    "MarketplaceUsageLimitExceeded",
];

/// The code of the refusal of a request that names a project on a route of the tenant scope.
///
/// Palette answers HTTP 403, and the message says that the operation is not allowed in the project
/// scope. The saved tag filters are such routes, with each query. The key is right: the request
/// must name no project.
pub const PROJECT_SCOPE_CODE: &str = "ProjectUidIsNotEmpty";

/// The code of the error for a move that a pack does not allow.
///
/// A pack of the body is above the highest version that a cluster can move to from the version it
/// runs. The validate routes of the packs answer it in `packs.results[]` with HTTP 200, with the
/// other errors of a pack. It is not a refusal of the whole request, so no table of a check holds
/// it. `behind` prints it under the bump, and strata decides nothing from it.
pub const UPGRADE_NOT_SUPPORTED_CODE: &str = "DependencyPackUpgradeNotSupported";

/// The code of the refusal of an update that moves the Kubernetes pack of a cluster more than one
/// minor version.
///
/// Palette refuses it with HTTP 400, on each cloud. It refuses it at the update of the profiles of
/// a cluster or of a cluster template. It also refuses it at an update or a publish of a profile
/// version that a cluster runs. No validate route checks the rule, so strata checks it: see
/// [`MinorSkip`](crate::model::update::MinorSkip). The code of Palette shows it; no live run shows
/// it yet.
pub const K8S_UPGRADE_NOT_SUPPORTED_CODE: &str = "K8sUpgradeNotSupported";

/// The codes of the errors that the import validate route gives with HTTP 500 when it matches no
/// pack of the body to a registry.
///
/// The route then checks nothing, so strata reads them as errors and not as an answer. A retry gets
/// the same error.
pub const PROFILE_IMPORT_MATCH_CODES: [&str; 4] = [
    "ClusterProfileImportRegistriesNotFound",
    "ClusterProfileImportOciRegistriesNotFound",
    "ClusterProfileImportPrivateHelmRegistriesNotFound",
    "ClusterProfileImportPackNotFound",
];

/// One check of a validate route: the codes of its refusal, each with HTTP 500.
///
/// The routes that run one check read one table.
type Check = &'static [&'static str];

/// The values and the manifests of each pack are YAML.
///
/// The four routes that take packs run it.
const PACK_YAML: Check = &["PackValuesYamlInvalid", "PackManifestYamlInvalid"];

/// The body holds no more than one dashboard pack.
///
/// The four routes that take packs run it.
const DASHBOARD_PACKS: Check = &["ClusterDashboardPacksMutuallyExclusive"];

/// The metadata of the packs.
///
/// The name of each manifest and of each manifest pack is a DNS-1123 label of 32 characters at
/// most. No pack name is two times in a profile. No manifest name is two times in a pack. No pack
/// has two versions. The cluster packs route, the import validate route, and the cluster validate
/// route run it. The cluster validate route also gives `InvalidName` for a name of the cluster or
/// of a machine pool that it refuses.
const PACK_METADATA: Check = &[
    "InvalidName",
    "DuplicatePacksForbidden",
    "DuplicatePackManifestsForbidden",
    "MultiPackVersionsForbidden",
];

/// No pack name is in two profiles of one workload scope of the cluster.
///
/// The cluster packs route and the cluster validate route run it.
const CLUSTER_PACKS: Check = &["DuplicateClusterPacksForbidden"];

/// The integration packs.
///
/// The cloud supports each of them. The OIDC identity provider that one names is in the Kubernetes
/// pack or in the organization. The cluster packs route, the import validate route, and the cluster
/// validate route run it.
const INTEGRATION_PACKS: Check = &[
    "PackSpectroIntegrationNotSupported",
    "ClusterProfileOidcNotConfigured",
];

/// No pack is disabled.
///
/// The four routes that take packs run it. A live run shows that the import validate route refuses
/// a candidate with a disabled pack.
const DISABLED_PACKS: Check = &["DisabledPacksForbidden"];

/// No more packs and no more manifests than the limit.
///
/// The profile packs routes and the import validate route run it, with a limit of 200 of each.
/// The cluster validate route runs it with the limit of a cluster, over all the profiles of the
/// body.
const SUB_RESOURCE_LIMIT: Check = &["SubResourceLimitValidationFailure"];

/// No other profile of a cluster that runs the version has a pack of the name in the same workload
/// scope.
///
/// Only the route of an update of a version runs it.
const PACKS_IN_CONFLICT: Check = &["ClusterProfilePacksInConflict"];

/// The name is not empty, and the version parses as semver.
///
/// The name route runs it. The import validate route runs it too, so its list of checks holds this
/// table.
const NAME_AND_VERSION: Check = &["ResourceNameEmpty", "VersionParseFailure"];

/// The tenant holds fewer profiles than its limit.
///
/// Only the import validate route runs it.
const PROFILE_LIMIT: Check = &["ResourceLimitValidationFailure"];

/// Palette supports the cloud type.
///
/// Only the import validate route runs it.
const CLOUD_TYPE: Check = &["UnSupportedCloudType"];

/// The variables, and the values and the dependencies of each pack: the check of the create.
///
/// The import validate route runs it too.
const CREATE_VALIDATION: Check = &["ClusterProfileCreationValidationFailure"];

/// Each profile of the body is of the cloud type of the route, or of the cloud type `all`.
///
/// Only the cluster validate route runs it. A live run shows that it refuses a body with an add-on
/// profile of another cloud type, with HTTP 500.
const PROFILE_CLOUD: Check = &["ClusterProfilesNotSupported"];

/// Each variable of the body is a variable of the profile, and its value passes the schema of the
/// variable in the profile.
///
/// Of the validate routes, only the cluster validate route runs it. A label that is not an option
/// of a dropdown variable is `InvalidDropDownVariable`. A value that the format or the pattern of
/// its variable refuses is `InvalidVariable`. Variables for a profile with no variables are
/// `ClusterVariablesNotFound`. A value of an immutable or hidden variable that is not its default
/// value is `ClusterVariableCreateFailure`. The route refuses each one with HTTP 500, as the code
/// of Palette shows.
const BODY_VARIABLES: Check = &[
    "InvalidDropDownVariable",
    INVALID_VARIABLE_CODE,
    "ClusterVariablesNotFound",
    "ClusterVariableCreateFailure",
];

/// Each profile of the body is published, valid, and not deleted, and one of them is an
/// infrastructure profile.
///
/// Only the cluster validate route runs it. A deleted profile is `ClusterProfileDeleted`. A profile
/// whose pack tag the registry deleted is `ClusterProfileIsNotValid`. A draft is
/// `ClusterProfileIsNotPublished`. A body with no infrastructure profile is
/// `MissingClusterInfraProfile`. The route refuses each one with HTTP 500, as the code of Palette
/// shows.
const BODY_PROFILES: Check = &[
    "ClusterProfileDeleted",
    "ClusterProfileIsNotValid",
    "ClusterProfileIsNotPublished",
    "MissingClusterInfraProfile",
];

/// The macros of a variable in the values of each pack name a variable.
///
/// The cluster validate route runs it. A macro `{{.spectro.var}}` with no name is
/// `InvalidPackVariable`, with HTTP 500. The code of Palette shows it; no live run shows it yet.
const PACK_VARIABLES: Check = &["InvalidPackVariable"];

/// When Palette runs in FIPS mode, each pack is FIPS compliant.
///
/// Of the validate routes, only the cluster validate route runs it. A pack that is not compliant
/// is `PackForbidden`, with HTTP 500. The code of Palette shows it; no live run shows it yet.
const FIPS_PACKS: Check = &["PackForbidden"];

/// The machine pools: a pool of the control plane, and the zones of the worker pools of AWS.
///
/// Only the cluster validate route runs it. For AWS, Azure, MAAS, vSphere, and Edge, a body with
/// no pool of the control plane is `ControlPlanePoolRequired`. For AWS, a worker pool with a zone
/// that the control plane does not have is `AwsWorkerPoolAzsValidate`. With no VPC, a worker pool
/// with no zone is `AwsDynamicWorkerPoolAzsRequired`. The route refuses each one with HTTP 500.
/// The code of Palette shows it; no live run shows it yet.
const CREATE_POOLS: Check = &[
    "ControlPlanePoolRequired",
    "AwsWorkerPoolAzsValidate",
    "AwsDynamicWorkerPoolAzsRequired",
];

/// The region and the instance type of each machine pool are known to Palette.
///
/// Only the cluster validate route runs it. An empty region is `NoRegionSpecified`. An instance
/// type that Palette does not list for the region is `InstanceTypeNotFound`. For GCP and GKE, it is
/// `CustomInstanceTypeNotFound` when no custom type of the region matches it either. The route
/// refuses each one with HTTP 500. The code of Palette shows it; no live run shows it yet.
const POOL_MACHINES: Check = &[
    "NoRegionSpecified",
    "InstanceTypeNotFound",
    "CustomInstanceTypeNotFound",
];

/// The labels and the SSH keys that the cloud reads.
///
/// Only the cluster validate route runs it. For Azure, a label key `microsoft`, `azure`, or
/// `windows` is `ReservedTags`. For GCP, a label key that GCP refuses is `GcpTagValidationFailure`.
/// For vSphere and Edge, an SSH key that is not a public key is `SshKeyValidationFailure`. The
/// route refuses each one with HTTP 500. The code of Palette shows it; no live run shows it yet.
const CLOUD_FIELDS: Check = &[
    "ReservedTags",
    "GcpTagValidationFailure",
    "SshKeyValidationFailure",
];

/// The code of the refusal of a value of a variable.
///
/// The cluster validate route gives it for a value of the body that the format or the pattern of
/// its variable refuses. The update of the profiles of a cluster gives it with HTTP 500 for a
/// required variable with no value and no default value. No validate route checks that, so strata
/// names such a variable: see [`UpdateChecks`](crate::model::update::UpdateChecks). The code of
/// Palette shows it; no live run shows it yet.
pub const INVALID_VARIABLE_CODE: &str = "InvalidVariable";

/// For AWS with a VPC, the pod CIDR and the service CIDR of the Kubernetes pack are CIDRs. Neither
/// one overlaps the CIDR of the VPC.
///
/// Only the cluster validate route runs it. A value that is not a CIDR is `CidrParseError`, and a
/// CIDR that overlaps is `K8SCidrOverlap`. The route refuses each one with HTTP 500. The code of
/// Palette shows it; no live run shows it yet.
const VPC_CIDRS: Check = &["K8SCidrOverlap", "CidrParseError"];

/// For AWS with a VPC, each machine pool names a subnet.
///
/// Only the cluster validate route runs it. A pool with no subnet is `RequiredParam`, with the
/// message `Parameter 'Subnets' is required`. A live run shows that the route refuses it with HTTP
/// 500. The code of Palette shows that the route gives the code for no other cause.
const POOL_SUBNETS: Check = &["RequiredParam"];

/// A validate route of Palette.
///
/// Each refuses the whole request with an HTTP error and a code before it checks each pack. Strata
/// reads such a refusal as an answer, and does not send the request again. Palette gives each
/// refusal the response type of a server fault, HTTP 500. A few refusals set another status: 403,
/// 404, 409, or 422.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidateRoute {
    /// `POST /v1/spectroclusters/{uid}/validate/packs`: the packs of the profiles on a cluster.
    ///
    /// `compare --validate` sends it.
    ClusterPacks,
    /// `POST /v1/clusterprofiles/validate/packs`, the packs of a candidate, and `POST
    /// /v1/clusterprofiles/{uid}/validate/packs`, the packs of a version as an update of itself.
    ///
    /// `snapshot --validate` sends them.
    ProfilePacks,
    /// `GET /v1/clusterprofiles/validate/name`: the name and the version of a candidate.
    ///
    /// `snapshot --validate` sends it.
    ProfileName,
    /// `POST /v1/clusterprofiles/import/validate`: a candidate as an import.
    ///
    /// `snapshot --validate` sends it.
    ProfileImport,
    /// `POST /v1/spectroclusters/{cloudType}/validate`: the body of a create.
    ///
    /// `strata preflight` sends it, as the rule `palette-validate`.
    ClusterCreate,
}

impl ValidateRoute {
    /// Each route.
    pub const ALL: [ValidateRoute; 5] = [
        ValidateRoute::ClusterPacks,
        ValidateRoute::ProfilePacks,
        ValidateRoute::ProfileName,
        ValidateRoute::ProfileImport,
        ValidateRoute::ClusterCreate,
    ];

    /// The checks of the route that refuse the whole request with HTTP 500, each one table of
    /// codes.
    ///
    /// The import validate route runs the check of the name route and the check of the create too,
    /// so its list holds their tables.
    const fn checks(self) -> &'static [Check] {
        match self {
            ValidateRoute::ClusterPacks => &[
                PACK_YAML,
                DASHBOARD_PACKS,
                PACK_METADATA,
                CLUSTER_PACKS,
                INTEGRATION_PACKS,
                DISABLED_PACKS,
            ],
            ValidateRoute::ProfilePacks => &[
                DASHBOARD_PACKS,
                PACK_YAML,
                DISABLED_PACKS,
                SUB_RESOURCE_LIMIT,
                PACKS_IN_CONFLICT,
            ],
            ValidateRoute::ProfileName => &[NAME_AND_VERSION],
            ValidateRoute::ProfileImport => &[
                PROFILE_LIMIT,
                SUB_RESOURCE_LIMIT,
                CLOUD_TYPE,
                DASHBOARD_PACKS,
                PACK_METADATA,
                PACK_YAML,
                INTEGRATION_PACKS,
                DISABLED_PACKS,
                NAME_AND_VERSION,
                CREATE_VALIDATION,
            ],
            ValidateRoute::ClusterCreate => &[
                CREATE_POOLS,
                POOL_SUBNETS,
                PACK_METADATA,
                BODY_PROFILES,
                PROFILE_CLOUD,
                DASHBOARD_PACKS,
                CLUSTER_PACKS,
                PACK_YAML,
                PACK_VARIABLES,
                INTEGRATION_PACKS,
                DISABLED_PACKS,
                SUB_RESOURCE_LIMIT,
                FIPS_PACKS,
                BODY_VARIABLES,
                POOL_MACHINES,
                CLOUD_FIELDS,
                VPC_CIDRS,
            ],
        }
    }

    /// The codes of the refusals of the whole request that the route answers with HTTP 500.
    pub fn refusal_codes(self) -> impl Iterator<Item = &'static str> {
        self.checks().iter().flat_map(|check| check.iter().copied())
    }

    /// Whether the route refuses the whole request with `status` and `code`.
    ///
    /// A code of a check of the route comes with HTTP 500. These refusals set another type:
    ///
    /// - The cluster packs route gives [`PACK_FORBIDDEN_CODE`] with HTTP 403.
    /// - The cluster validate route gives [`PACK_FORBIDDEN_CODE`], the codes of
    ///   [`CLUSTER_CREATE_FORBIDDEN_CODES`], and the codes of [`CLUSTER_CREATE_LIMIT_CODES`] with
    ///   HTTP 403. Another 403 of the route, such as the refusal of the role of the key, is no
    ///   refusal of the request.
    /// - The name route and the import validate route give [`PROFILE_EXISTS_CODE`] with HTTP 409.
    /// - The import validate route gives [`PACK_MACRO_UNDEFINED_CODE`] with HTTP 404.
    /// - The API schema checks the body of the profile routes first. It refuses a list with two
    ///   equal items with HTTP 422 and a number as the code.
    pub(crate) fn refuses(self, status: u16, code: &str) -> bool {
        match (self, status) {
            (_, 500) => self.refusal_codes().any(|c| c == code),
            (ValidateRoute::ClusterPacks, 403) => code == PACK_FORBIDDEN_CODE,
            (ValidateRoute::ClusterCreate, 403) => {
                code == PACK_FORBIDDEN_CODE
                    || CLUSTER_CREATE_FORBIDDEN_CODES.contains(&code)
                    || CLUSTER_CREATE_LIMIT_CODES.contains(&code)
            }
            (ValidateRoute::ProfileName | ValidateRoute::ProfileImport, 409) => {
                code == PROFILE_EXISTS_CODE
            }
            (ValidateRoute::ProfileImport, 404) => code == PACK_MACRO_UNDEFINED_CODE,
            (ValidateRoute::ProfilePacks | ValidateRoute::ProfileImport, 422) => true,
            _ => false,
        }
    }

    /// The refusal of the whole request in `body`, when the route refuses it with the HTTP error
    /// `status`.
    ///
    /// `body` is the body of the Palette error. The route refuses it when the `code` of that body
    /// is a code that the route refuses with `status`. A body that is not JSON with a `code` is no
    /// refusal.
    pub(crate) fn refusal(self, status: u16, body: &str) -> Option<PaletteRefusal> {
        PaletteRefusal::from_error_body(body).filter(|r| self.refuses(status, &r.code))
    }

    /// Whether the route refuses the whole request with the HTTP error `status` and `error`.
    ///
    /// `error` is the body of the Palette error, parsed. A body with no `code` is no refusal.
    pub fn refuses_answer(self, status: u16, error: &Value) -> bool {
        PaletteRefusal::from_error(error).is_some_and(|r| self.refuses(status, &r.code))
    }

    /// The validate route that the request `method` `path` goes to, or `None` when it goes to
    /// none. `path` can carry a query.
    ///
    /// The routes are the ones of [`ValidateRoute`]. The segment `{uid}` or `{cloudType}` is any
    /// text.
    pub fn of(method: Method, path: &str) -> Option<ValidateRoute> {
        let path = path.split_once('?').map_or(path, |(path, _)| path);
        let segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        match (method, segments.as_slice()) {
            (Method::Post, ["v1", "spectroclusters", _, "validate", "packs"]) => {
                Some(ValidateRoute::ClusterPacks)
            }
            (
                Method::Post,
                ["v1", "clusterprofiles", "validate", "packs"]
                | ["v1", "clusterprofiles", _, "validate", "packs"],
            ) => Some(ValidateRoute::ProfilePacks),
            (Method::Get, ["v1", "clusterprofiles", "validate", "name"]) => {
                Some(ValidateRoute::ProfileName)
            }
            (Method::Post, ["v1", "clusterprofiles", "import", "validate"]) => {
                Some(ValidateRoute::ProfileImport)
            }
            (Method::Post, ["v1", "spectroclusters", _, "validate"]) => {
                Some(ValidateRoute::ClusterCreate)
            }
            _ => None,
        }
    }

    /// Whether the request `method` `path` goes to a validate route that refuses the whole
    /// request with the HTTP error `status` and `error`: [`ValidateRoute::of`], then
    /// [`ValidateRoute::refuses_answer`].
    pub fn refuses_request(method: Method, path: &str, status: u16, error: &Value) -> bool {
        ValidateRoute::of(method, path).is_some_and(|route| route.refuses_answer(status, error))
    }
}

/// Whether `code` is the code of a refusal that Palette answers with HTTP 500, and that a retry
/// does not change.
///
/// The codes are of four kinds. The first two are the refusals of the other routes and each
/// [`Denial`]. The third kind holds the errors of the import validate route that match no pack to
/// a registry. The checks of each [`ValidateRoute`] are the fourth.
/// [`Error::class`](crate::Error::class) gives the class `Terminal` to a 5xx with such a code. A
/// rule of `strata-preflight` that names a code in `refusal_codes` must name one of them, so that
/// the check and the retry read one list.
pub fn is_refusal_code(code: &str) -> bool {
    REFUSAL_CODES.contains(&code)
        || Denial::from_text(code).is_some()
        || PROFILE_IMPORT_MATCH_CODES.contains(&code)
        || ValidateRoute::ALL
            .iter()
            .any(|route| route.refusal_codes().any(|c| c == code))
}

/// A rule of Palette that only the create of a profile version checks.
///
/// No route that takes a candidate answers it before the create, so strata checks it before the
/// create. A candidate that breaks it is
/// [`Error::CandidateInvalid`](crate::Error::CandidateInvalid) with the rule. The book lists each
/// rule in "The gaps that strata fills".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateRule {
    /// The type of the profile takes the layer of each pack.
    TypeLayer,
    /// A `cluster` or `infra` profile has its core layers.
    CoreLayers,
    /// Each pack supports the cloud of the profile.
    Cloud,
    /// The tag of a pack that Palette makes for a registry that it does not sync is a version.
    PackTag,
}

impl CreateRule {
    /// The code of the error that Palette gives at the create.
    pub fn code(self) -> &'static str {
        match self {
            CreateRule::TypeLayer => "ClusterProfilePackNotAllowed",
            CreateRule::CoreLayers => "ErrPackCoreLayersMissing",
            CreateRule::Cloud => "PackNotSupported",
            CreateRule::PackTag => "InvalidPackVersion",
        }
    }
}

#[cfg(test)]
mod tests;
