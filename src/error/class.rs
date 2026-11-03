//! The class of a failure: how a caller treats it when it decides to try again.

use super::Error;
use crate::api::codes::is_refusal_code;
use crate::model::validation::PaletteRefusal;

/// The class of a failure, for a caller that decides to try again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FailureClass {
    /// Time can remove it: a transport error, HTTP 5xx, HTTP 429, or a `Retry-After` that is
    /// longer than strata waits.
    Transient,
    /// It stays while the inputs stay.
    ///
    /// - Another HTTP 4xx.
    /// - A 5xx with the code of a refusal of Palette, or of a check of a validate route.
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
            // A 2xx that is not JSON can be a gateway page or a real fault. A file or a serializer
            // that fails says nothing about the next try. A send of the report can fail for the
            // network of the user or for the endpoint, and the error does not tell which.
            Error::NotJson { .. }
            | Error::NoUid
            | Error::Io { .. }
            | Error::Serialize(_)
            | Error::ReportSend { .. } => FailureClass::Unknown,
            // Every other failure stays while the inputs stay. They are another HTTP status, a
            // thing that is not there, and a file or a setting that is not valid.
            _ => FailureClass::Terminal,
        }
    }
}

/// True when the body of a Palette error is JSON with a `code` of a refusal by design.
///
/// [`is_refusal_code`] says which codes these are. They are the refusals of the other routes. They
/// are also the errors of the import validate route that match no pack to a registry, and the
/// checks of each validate route.
fn is_refusal(body: &str) -> bool {
    PaletteRefusal::from_error_body(body).is_some_and(|r| is_refusal_code(&r.code))
}
