//! The class of a failure: how a caller treats it when it decides to try again.

use super::Error;
use crate::api::codes::{PROFILE_IMPORT_MATCH_CODES, REFUSAL_CODES, ValidateRoute};
use crate::validation::PaletteRefusal;

/// The class of a failure, for a caller that decides to try again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FailureClass {
    /// Time can remove it: a transport error, HTTP 5xx, HTTP 429, or a `Retry-After` that is
    /// longer than strata waits.
    Transient,
    /// It stays while the inputs stay.
    ///
    /// - Another HTTP 4xx.
    /// - A 5xx with a code of [`REFUSAL_CODES`] or of a check of a [`ValidateRoute`].
    /// - A 5xx of the import validate route when it matches no pack of the body to a registry.
    /// - A thing that is not there, or a file or a setting that is not valid.
    Terminal,
    /// Strata cannot tell.
    Unknown,
}

impl Error {
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
}

/// True when the body of a Palette error is JSON with a `code` of a refusal by design.
///
/// Such a code is in [`REFUSAL_CODES`], in `PROFILE_IMPORT_MATCH_CODES`, or in a check of a
/// [`ValidateRoute`]. Each [`ValidateRoute`] says which bodies are a refusal of its checks.
fn is_refusal(body: &str) -> bool {
    PaletteRefusal::from_error_body(body).is_some_and(|r| {
        let code = r.code.as_str();
        REFUSAL_CODES.contains(&code) || PROFILE_IMPORT_MATCH_CODES.contains(&code)
    }) || ValidateRoute::ALL
        .iter()
        .any(|route| route.refusal(500, body).is_some())
}
