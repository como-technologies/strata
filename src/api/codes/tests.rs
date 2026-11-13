//! The tests of the codes of Palette: the tables, the routes, the classes, and the denials.

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
const CHECKS: [Check; 16] = [
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
    BODY_VARIABLES,
    VPC_CIDRS,
    POOL_SUBNETS,
];

#[test]
fn each_code_of_palette_is_in_one_table() {
    let other_status = [
        PROFILE_EXISTS_CODE,
        PACK_MACRO_UNDEFINED_CODE,
        PACK_FORBIDDEN_CODE,
    ];
    let denials: Vec<&str> = Denial::ALL.iter().map(|d| d.as_str()).collect();
    let tables = CHECKS.iter().copied().chain([
        &REFUSAL_CODES[..],
        &denials[..],
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
fn a_code_of_a_refusal_at_a_5xx_is_a_refusal_code_and_the_class_of_its_error_is_terminal() {
    let api = |status: u16, code: &str| Error::Api {
        method: Method::Post,
        path: "/v1/clouds/aws/properties/validate".into(),
        status,
        body: format!(r#"{{"code":"{code}","message":"refused"}}"#),
        retry_after: None,
    };
    let mut listed: Vec<&str> = REFUSAL_CODES.to_vec();
    listed.extend(Denial::ALL.iter().map(|d| d.as_str()));
    listed.extend(PROFILE_IMPORT_MATCH_CODES);
    listed.extend(ValidateRoute::ALL.iter().flat_map(|r| r.refusal_codes()));
    // the property route of AWS: a Pod Identity account, and a key pair that the region lacks
    listed.extend(["FeatureNotSupported", "AwsKeyPairNotFound"]);
    for code in listed {
        assert!(is_refusal_code(code), "{code}");
        assert_eq!(api(500, code).class(), FailureClass::Terminal, "{code}");
        assert_eq!(api(503, code).class(), FailureClass::Terminal, "{code}");
    }
    // a code that a route gives with another status is no refusal at a 5xx. An error of a pack
    // in a 200 answer is no refusal at a 5xx. A rule of the create and a fault of the server
    // are no refusal at a 5xx
    let other = [
        PROFILE_EXISTS_CODE,
        PACK_MACRO_UNDEFINED_CODE,
        PACK_FORBIDDEN_CODE,
        UPGRADE_NOT_SUPPORTED_CODE,
        "InternalError",
        "610",
        "",
    ]
    .into_iter()
    .chain(RULES.map(CreateRule::code));
    for code in other {
        assert!(!is_refusal_code(code), "{code}");
        assert_eq!(api(500, code).class(), FailureClass::Transient, "{code}");
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
fn only_the_cluster_validate_route_refuses_a_profile_of_another_cloud_type_a_variable_a_cidr_or_a_pool_with_no_subnet()
 {
    for code in [PROFILE_CLOUD, BODY_VARIABLES, VPC_CIDRS, POOL_SUBNETS].concat() {
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
    assert!(!ValidateRoute::ClusterCreate.refuses_answer(500, &json!({"code": "InternalError"})));
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

#[test]
fn a_denial_names_the_permission_of_its_message_and_a_denial_of_the_project_names_none() {
    let route = "filter.list operation is not permitted, required permissions 'filter.list' in the \
                 scope(s) tenant";
    let object = "Operation 'cluster.get' is forbidden. Verify the user has 'cluster.get' \
                  permission";
    assert_eq!(Denial::Route.permission(route), Some("filter.list"));
    assert_eq!(Denial::Object.permission(object), Some("cluster.get"));
    assert_eq!(
        Denial::Route.sentence(route),
        "the role of the key does not give the permission 'filter.list'"
    );
    // a message with no quotes, or with empty quotes, names no permission
    for message in [
        "access denied",
        "the permission '' is missing",
        "one ' only",
    ] {
        assert_eq!(Denial::Route.permission(message), None, "{message}");
    }
    assert_eq!(
        Denial::Object.sentence("forbidden"),
        "the role of the key does not give the permission of the request"
    );
    // the quotes of a denial of the project hold the project, and no permission
    let project = "Project 'dev' access denied";
    assert_eq!(Denial::Project.permission(project), None);
    assert_eq!(
        Denial::Project.sentence(project),
        "no role of the key in the project gives the access of the request"
    );
    assert_eq!(
        Denial::NoRole.permission("No roles associated to user 'barney'"),
        None
    );
}
