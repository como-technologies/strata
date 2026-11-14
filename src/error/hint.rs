//! The end of the message of [`Error::Api`](super::Error::Api): what the status and the body of
//! the answer tell the reader to do.

use crate::api::codes::{Denial, PROJECT_SCOPE_CODE};
use crate::api::{DOWNLOAD_INTERVAL_SECONDS, is_download_limit, key_proved};
use crate::config::{ENV_FILE, KEY_VARS};
use crate::model::validation::PaletteRefusal;

/// The hint of an answer with `status` and `body` to a request of `path`.
///
/// - A 403 with the code [`PROJECT_SCOPE_CODE`]: the route is of the tenant scope, and the request
///   named a project. The key is right.
/// - A 429 of the download of a profile version: the limit of Palette.
/// - A [`Denial`]: what Palette denied, such as the permission that the role of the key does not
///   give. A 403 adds the hint of the key only when no answer of the run proved the key.
/// - Another 401 or 403: the hint of the key, which is almost always the cause.
///
/// Empty for each other status.
pub(super) fn status_hint(status: u16, path: &str, body: &str) -> String {
    let refusal = PaletteRefusal::from_error_body(body);
    if status == 403
        && refusal
            .as_ref()
            .is_some_and(|r| r.code == PROJECT_SCOPE_CODE)
    {
        return "; the route is of the tenant scope, and the request named a project: ask it with no project"
            .to_string();
    }
    if is_download_limit(status, path) {
        return format!(
            "; Palette takes one download of a profile version in {DOWNLOAD_INTERVAL_SECONDS} seconds: run the command again later"
        );
    }
    let denial = refusal.and_then(|r| Some((Denial::from_text(&r.code)?, r.message)));
    let key = match (status, &denial) {
        (401, _) => true,
        (403, Some(_)) => !key_proved(),
        (403, None) => true,
        _ => false,
    };
    let mut hint = denial
        .map(|(denial, message)| format!("; {}", denial.sentence(&message)))
        .unwrap_or_default();
    if key {
        hint.push_str(&format!(
            "; check {}, {}, or ~/{ENV_FILE}: the key can be wrong, expired, or for another tenant",
            KEY_VARS[0], KEY_VARS[1],
        ));
    }
    hint
}
