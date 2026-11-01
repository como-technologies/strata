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
//! Each comment states what Palette does, and not where its code does it.

use serde_json::Value;

use crate::api::Method;
use crate::validation::PaletteRefusal;

/// The codes of the refusals that Palette answers with HTTP 500 on a route that is not a validate
/// route.
///
/// The routes are the delete of an object, an operation on an object, the update of a profile, and
/// a listing. Palette gives these errors the response type of a server fault, so a refusal by
/// design arrives as a 5xx. A retry gets the same refusal.
pub const REFUSAL_CODES: [&str; 6] = [
    "DeletionResourceInUseError",
    "ResOperationForbidden",
    "OperationForbidden",
    "ClusterProfileUpdateValidationFailure",
    "ClusterProfileInvalidPackState",
    "InvalidLimit",
];

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
/// integration pack needs read-only has values other than its defaults.
pub const PACK_FORBIDDEN_CODE: &str = "PackValuesOverrideForbidden";

/// The code of the error for a move that a pack does not allow.
///
/// A pack of the body is above the highest version that a cluster can move to from the version it
/// runs. The validate routes of the packs answer it in `packs.results[]` with HTTP 200, with the
/// other errors of a pack. It is not a refusal of the whole request, so no table of a check holds
/// it. `behind` prints it under the bump, and strata decides nothing from it.
pub const UPGRADE_NOT_SUPPORTED_CODE: &str = "DependencyPackUpgradeNotSupported";

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
/// The three routes that take packs run it.
const PACK_YAML: Check = &["PackValuesYamlInvalid", "PackManifestYamlInvalid"];

/// The body holds no more than one dashboard pack.
///
/// The three routes that take packs run it.
const DASHBOARD_PACKS: Check = &["ClusterDashboardPacksMutuallyExclusive"];

/// The metadata of the packs.
///
/// The name of each manifest and of each manifest pack is a DNS-1123 label of 32 characters at
/// most. No pack name is two times in a profile. No manifest name is two times in a pack. No pack
/// has two versions. The cluster packs route and the import validate route run it.
const PACK_METADATA: Check = &[
    "InvalidName",
    "DuplicatePacksForbidden",
    "DuplicatePackManifestsForbidden",
    "MultiPackVersionsForbidden",
];

/// No pack name is in two profiles of one workload scope of the cluster.
///
/// Only the cluster packs route runs it.
const CLUSTER_PACKS: Check = &["DuplicateClusterPacksForbidden"];

/// The integration packs.
///
/// The cloud supports each of them. The OIDC identity provider that one names is in the Kubernetes
/// pack or in the organization. The cluster packs route and the import validate route run it.
const INTEGRATION_PACKS: Check = &[
    "PackSpectroIntegrationNotSupported",
    "ClusterProfileOidcNotConfigured",
];

/// No pack is disabled.
///
/// The cluster packs route, the profile packs routes, and the import validate route run it. A live
/// run shows that the import validate route refuses a candidate with a disabled pack.
const DISABLED_PACKS: Check = &["DisabledPacksForbidden"];

/// No more than 200 packs, and no more than 200 manifests.
///
/// The profile packs routes and the import validate route run it.
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
            ValidateRoute::ClusterCreate => &[PROFILE_CLOUD],
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
    /// - The name route and the import validate route give [`PROFILE_EXISTS_CODE`] with HTTP 409.
    /// - The import validate route gives [`PACK_MACRO_UNDEFINED_CODE`] with HTTP 404.
    /// - The API schema checks the body of the profile routes first. It refuses a list with two
    ///   equal items with HTTP 422 and a number as the code.
    pub(crate) fn refuses(self, status: u16, code: &str) -> bool {
        match (self, status) {
            (_, 500) => self.refusal_codes().any(|c| c == code),
            (ValidateRoute::ClusterPacks, 403) => code == PACK_FORBIDDEN_CODE,
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
mod tests {
    use super::*;
    use crate::error::{Error, FailureClass};
    use serde_json::json;

    /// Each rule of the create.
    const RULES: [CreateRule; 4] = [
        CreateRule::TypeLayer,
        CreateRule::CoreLayers,
        CreateRule::Cloud,
        CreateRule::PackTag,
    ];

    /// Each table of one check.
    const CHECKS: [Check; 13] = [
        PACK_YAML,
        DASHBOARD_PACKS,
        PACK_METADATA,
        CLUSTER_PACKS,
        INTEGRATION_PACKS,
        DISABLED_PACKS,
        SUB_RESOURCE_LIMIT,
        PACKS_IN_CONFLICT,
        NAME_AND_VERSION,
        PROFILE_LIMIT,
        CLOUD_TYPE,
        CREATE_VALIDATION,
        PROFILE_CLOUD,
    ];

    #[test]
    fn each_code_of_palette_is_in_one_table() {
        let other_status = [
            PROFILE_EXISTS_CODE,
            PACK_MACRO_UNDEFINED_CODE,
            PACK_FORBIDDEN_CODE,
        ];
        let tables = CHECKS.iter().copied().chain([
            &REFUSAL_CODES[..],
            &PROFILE_IMPORT_MATCH_CODES[..],
            &other_status[..],
        ]);
        let mut codes: Vec<&str> = tables.flat_map(|table| table.iter().copied()).collect();
        let count = codes.len();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), count, "a code is in two tables: {codes:?}");
        for rule in RULES {
            assert!(!codes.contains(&rule.code()), "{rule:?}");
        }
        // an error of a pack in a 200 answer is in no table of a refusal of the whole request
        assert!(!codes.contains(&UPGRADE_NOT_SUPPORTED_CODE));
        // each check that a route runs is one of the tables
        for route in ValidateRoute::ALL {
            for check in route.checks() {
                assert!(CHECKS.contains(check), "{route:?}: {check:?}");
            }
        }
    }

    #[test]
    fn each_route_reads_the_codes_of_its_checks_with_http_500() {
        for route in ValidateRoute::ALL {
            for code in route.refusal_codes() {
                assert!(route.refuses(500, code), "{route:?} {code}");
                assert!(!route.refuses(503, code), "{route:?} {code}");
            }
            assert!(!route.refuses(500, "InternalError"), "{route:?}");
            assert!(!route.refuses(500, "610"), "{route:?}");
        }
        // the import validate route runs the check of the name route too
        for code in ValidateRoute::ProfileName.refusal_codes() {
            assert!(ValidateRoute::ProfileImport.refuses(500, code), "{code}");
        }
        // a rule of the create is a gap: no validate route answers its code
        for rule in RULES {
            for route in ValidateRoute::ALL {
                assert!(!route.refuses(500, rule.code()), "{route:?} {rule:?}");
            }
        }
    }

    #[test]
    fn the_routes_that_check_the_packs_of_a_profile_refuse_a_disabled_pack() {
        for code in DISABLED_PACKS {
            for route in [
                ValidateRoute::ClusterPacks,
                ValidateRoute::ProfilePacks,
                ValidateRoute::ProfileImport,
            ] {
                assert!(route.refuses(500, code), "{route:?} {code}");
            }
            assert!(!ValidateRoute::ProfileName.refuses(500, code), "{code}");
        }
    }

    #[test]
    fn only_the_cluster_validate_route_refuses_a_profile_of_another_cloud_type() {
        for code in PROFILE_CLOUD {
            assert!(ValidateRoute::ClusterCreate.refuses(500, code), "{code}");
            assert!(!ValidateRoute::ClusterCreate.refuses(503, code), "{code}");
            for route in [
                ValidateRoute::ClusterPacks,
                ValidateRoute::ProfilePacks,
                ValidateRoute::ProfileName,
                ValidateRoute::ProfileImport,
            ] {
                assert!(!route.refuses(500, code), "{route:?} {code}");
            }
        }
    }

    #[test]
    fn a_route_reads_a_refusal_from_the_body_of_a_palette_error() {
        let body = r#"{"code":"ClusterProfilesNotSupported","message":"Cluster profiles 'p' are not supported with 'aws' environment"}"#;
        let refusal = ValidateRoute::ClusterCreate
            .refusal(500, body)
            .expect("the route refuses its own code with HTTP 500");
        assert_eq!(refusal.code, "ClusterProfilesNotSupported");
        assert_eq!(
            refusal.message,
            "Cluster profiles 'p' are not supported with 'aws' environment"
        );
        assert_eq!(ValidateRoute::ClusterCreate.refusal(502, body), None);
        assert_eq!(ValidateRoute::ProfilePacks.refusal(500, body), None);
        let error: Value = serde_json::from_str(body).unwrap();
        assert!(ValidateRoute::ClusterCreate.refuses_answer(500, &error));
        assert!(!ValidateRoute::ClusterCreate.refuses_answer(502, &error));
        assert!(!ValidateRoute::ProfilePacks.refuses_answer(500, &error));
        assert!(!ValidateRoute::ClusterCreate.refuses_answer(500, &Value::Null));
        assert!(
            !ValidateRoute::ClusterCreate.refuses_answer(500, &json!({"code": "InternalError"}))
        );
    }

    #[test]
    fn a_request_goes_to_the_validate_route_that_its_method_and_its_path_name() {
        let post = Method::Post;
        for (method, path, route) in [
            (
                post,
                "/v1/spectroclusters/u1/validate/packs",
                ValidateRoute::ClusterPacks,
            ),
            (
                post,
                "/v1/clusterprofiles/validate/packs",
                ValidateRoute::ProfilePacks,
            ),
            (
                post,
                "/v1/clusterprofiles/u1/validate/packs",
                ValidateRoute::ProfilePacks,
            ),
            (
                Method::Get,
                "/v1/clusterprofiles/validate/name?name=a&version=1",
                ValidateRoute::ProfileName,
            ),
            (
                post,
                "/v1/clusterprofiles/import/validate",
                ValidateRoute::ProfileImport,
            ),
            (
                post,
                "/v1/spectroclusters/aws/validate",
                ValidateRoute::ClusterCreate,
            ),
            (
                post,
                "/v1/spectroclusters/edge-native/validate",
                ValidateRoute::ClusterCreate,
            ),
        ] {
            assert_eq!(ValidateRoute::of(method, path), Some(route), "{path}");
        }
        for (method, path) in [
            (Method::Get, "/v1/spectroclusters/aws/validate"),
            (post, "/v1/spectroclusters/aws"),
            (post, "/v1/spectroclusters/u1/validate/repave"),
            (post, "/v1/clouds/aws/account/validate"),
            (post, "/v1/spectroclusters/validate"),
            (post, ""),
        ] {
            assert_eq!(ValidateRoute::of(method, path), None, "{path}");
        }
    }

    #[test]
    fn only_a_request_to_the_cluster_validate_route_reads_its_refusal() {
        let error = json!({"code": "ClusterProfilesNotSupported"});
        let post = Method::Post;
        assert!(ValidateRoute::refuses_request(
            post,
            "/v1/spectroclusters/aws/validate",
            500,
            &error
        ));
        // the same body from another route is no refusal of the route that the code belongs to
        assert!(!ValidateRoute::refuses_request(
            post,
            "/v1/clusterprofiles/validate/packs",
            500,
            &error
        ));
        assert!(!ValidateRoute::refuses_request(
            post,
            "/v1/clouds/aws/account/validate",
            500,
            &error
        ));
        assert!(!ValidateRoute::refuses_request(
            Method::Get,
            "/v1/spectroclusters/aws/validate",
            500,
            &error
        ));
        assert!(!ValidateRoute::refuses_request(
            post,
            "/v1/spectroclusters/aws/validate",
            502,
            &error
        ));
    }

    #[test]
    fn a_retry_does_not_send_again_the_refusal_of_the_cluster_validate_route() {
        for code in ValidateRoute::ClusterCreate.refusal_codes() {
            let error = Error::Api {
                method: crate::api::Method::Post,
                path: "/v1/spectroclusters/aws/validate".into(),
                status: 500,
                body: format!(r#"{{"code":"{code}","message":"refused"}}"#),
                retry_after: None,
            };
            assert_eq!(error.class(), FailureClass::Terminal, "{code}");
        }
    }

    #[test]
    fn the_refusals_that_set_another_type_belong_to_their_routes() {
        assert!(ValidateRoute::ClusterPacks.refuses(403, PACK_FORBIDDEN_CODE));
        assert!(!ValidateRoute::ClusterPacks.refuses(403, "Forbidden"));
        assert!(!ValidateRoute::ProfilePacks.refuses(403, PACK_FORBIDDEN_CODE));
        for route in [ValidateRoute::ProfileName, ValidateRoute::ProfileImport] {
            assert!(route.refuses(409, PROFILE_EXISTS_CODE), "{route:?}");
            assert!(!route.refuses(409, "Conflict"), "{route:?}");
        }
        assert!(!ValidateRoute::ClusterPacks.refuses(409, PROFILE_EXISTS_CODE));
        assert!(ValidateRoute::ProfileImport.refuses(404, PACK_MACRO_UNDEFINED_CODE));
        assert!(!ValidateRoute::ProfileName.refuses(404, PACK_MACRO_UNDEFINED_CODE));
        // the API schema refuses a body of the profile routes with HTTP 422 and a number
        for route in [ValidateRoute::ProfilePacks, ValidateRoute::ProfileImport] {
            assert!(route.refuses(422, "610"), "{route:?}");
        }
        for route in [ValidateRoute::ClusterPacks, ValidateRoute::ProfileName] {
            assert!(!route.refuses(422, "610"), "{route:?}");
        }
    }
}
