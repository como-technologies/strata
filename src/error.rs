//! One error type for the library. The CLI maps it to an exit status and a message; library users
//! can match on the variant.

use std::path::PathBuf;

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
    /// Palette answered with a non-2xx status.
    #[error("GET {path} failed with HTTP {status}: {body}")]
    Api {
        /// Request path.
        path: String,
        /// HTTP status code.
        status: u16,
        /// The first part of the response body, one line.
        body: String,
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
    /// `name@version` matched nothing in the current scope.
    #[error("no cluster profile named '{name}' with version '{version}' in this scope")]
    ProfileNotFound {
        /// Profile name.
        name: String,
        /// Profile version.
        version: String,
    },
    /// The file exists but is not a strata snapshot.
    #[error("'{0}' is not a strata snapshot")]
    NotSnapshot(PathBuf),
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

impl Error {
    /// The process exit status the CLI uses for this error. Every error is 2 today; the method
    /// exists so a future distinction (for example, 3 for "cannot reach Palette") lands in one place.
    pub fn exit_code(&self) -> u8 {
        2
    }
}

/// Result alias for the library.
pub type Result<T> = std::result::Result<T, Error>;
