//! The role bindings of a cluster, `GET /v1/spectroclusters/{uid}/config/rbacs`, as Palette answers
//! them to the user of the key.
//!
//! The file of the route holds each binding with all its subjects. In a binding of the system, an
//! item with `spec.isSystem` true, Palette answers only the subject that is the user of the key.
//! The routes of the tests build that answer in `rbacs_answer`, so the tests and the stand-in give
//! one answer. The code of Palette shows it; no live run shows it yet.
//!
//! The file [`KEY_USERS`] at the root of the tenant names the user of each key by its email. A key
//! that the file does not name is the key of a user that no binding names.

use std::io;
use std::path::Path;

use serde_json::Value;

use super::Request;
use crate::common_palette::rbacs_answer;
use crate::tenant::{KEY_USERS, is_rbacs_route};

/// The answer to `request` from `body`, the file of its route in the tenant at `root`. The route
/// of the role bindings of a cluster gets the answer to the user of its key. Each other route, and
/// a file that holds no JSON, gets `body` as it is.
pub(super) fn answered(root: &Path, request: &Request, body: Vec<u8>) -> io::Result<Vec<u8>> {
    let path = request
        .target
        .split_once('?')
        .map_or(request.target.as_str(), |(path, _)| path);
    if !is_rbacs_route(path.trim_start_matches('/')) {
        return Ok(body);
    }
    let Ok(stored) = serde_json::from_slice::<Value>(&body) else {
        return Ok(body);
    };
    // a Palette user always has an email, and no subject has an empty name. So the empty name is a
    // user that no binding names.
    let user = key_user(root, request.key.as_deref())?.unwrap_or_default();
    Ok(rbacs_answer(stored, &user).to_string().into_bytes())
}

/// The email of the user of `key`, from the file [`KEY_USERS`] at `root`. `None` when the request
/// has no key, when the tenant has no such file, or when the file does not name the key.
fn key_user(root: &Path, key: Option<&str>) -> io::Result<Option<String>> {
    let file = root.join(KEY_USERS);
    if !file.is_file() {
        return Ok(None);
    }
    let users: Value = serde_json::from_slice(&std::fs::read(&file)?)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(key
        .and_then(|key| users.get(key))
        .and_then(Value::as_str)
        .map(str::to_string))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::Value;

    use crate::common_palette::system_rbacs;
    use crate::palette::{Request, served};
    use crate::tenant::{Tenant, rbacs_route};

    /// The bindings of each item of `answer`, as the role and the names of the subjects of each.
    fn bindings(answer: &Value) -> Vec<Vec<(String, Vec<String>)>> {
        let text = |v: &Value| v.as_str().unwrap().to_string();
        answer["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| {
                let bindings = item["spec"]["bindings"].as_array().unwrap();
                bindings
                    .iter()
                    .map(|b| {
                        let subjects = b["subjects"].as_array().unwrap();
                        let names = subjects.iter().map(|s| text(&s["name"])).collect();
                        (text(&b["role"]["name"]), names)
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn the_role_bindings_of_the_system_hold_only_the_subject_that_is_the_user_of_the_key() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();
        let file = dir.join(format!("p1/{}.json", rbacs_route("c1")));
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, system_rbacs().to_string()).unwrap();
        Tenant::new(dir, dir).key_users(&[
            ("k-fred", "fred@example.com"),
            ("k-wilma", "wilma@example.com"),
        ]);
        let answer = |target: &str, key: Option<&str>| {
            let request = Request {
                target: target.to_string(),
                project: Some("p1".to_string()),
                key: key.map(str::to_string),
                accept: None,
                body: Vec::new(),
            };
            let (status, body) = served(dir, &request).unwrap();
            assert_eq!(status, 200, "{target}");
            serde_json::from_slice::<Value>(&body).unwrap()
        };
        let route = format!("/{}", rbacs_route("c1"));
        let group = vec![("view".to_string(), vec!["platform".to_string()])];
        let one = |role: &str, user: &str| vec![(role.to_string(), vec![user.to_string()])];

        // each user reads the binding of the system that names it, and the group in full
        let fred = answer(&route, Some("k-fred"));
        assert_eq!(
            bindings(&fred),
            [one("cluster-admin", "fred@example.com"), group.clone()]
        );
        let wilma = answer(&route, Some("k-wilma"));
        assert_eq!(
            bindings(&wilma),
            [one("view", "wilma@example.com"), group.clone()]
        );
        // a key that the file does not name, and a request with no key, read no binding of the
        // system
        for key in [Some("k-other"), None] {
            assert_eq!(bindings(&answer(&route, key)), [vec![], group.clone()]);
        }
        // the item of the system stays, with its other fields
        assert_eq!(fred["items"][0]["spec"]["isSystem"], true);
        // another route serves its file as it is
        let other = dir.join("p1/v1/spectroclusters/c1/config/namespaces.json");
        fs::write(&other, system_rbacs().to_string()).unwrap();
        assert_eq!(
            answer("/v1/spectroclusters/c1/config/namespaces", Some("k-fred")),
            system_rbacs()
        );
    }
}
