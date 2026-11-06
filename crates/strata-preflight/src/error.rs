//! One error type for the crate. The command maps it to exit status `2`: strata-preflight could
//! not answer. A failed check is not an error; it is a result, and the report carries it.

use std::path::PathBuf;

/// Everything that stops a run before it has an answer.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A rule file does not parse, or breaks a rule of the format.
    #[error("{}: {message}", path.display())]
    Catalog {
        /// The rule file.
        path: PathBuf,
        /// What is wrong.
        message: String,
    },
    /// A jq program of a rule does not compile, or fails on its input.
    #[error("rule {rule}: {message}")]
    Jq {
        /// The rule whose program failed.
        rule: String,
        /// What jaq said.
        message: String,
    },
    /// The input is not a cluster entity strata-preflight can read.
    #[error("{message}")]
    Input {
        /// What is wrong, and what to give instead.
        message: String,
    },
    /// The command was called in a way that cannot work.
    #[error("{message}")]
    Usage {
        /// What the caller must do instead.
        message: String,
    },
    /// A file could not be read or written.
    #[error("{}: {source}", path.display())]
    Io {
        /// The file.
        path: PathBuf,
        /// The I/O error.
        source: std::io::Error,
    },
    /// A ledger of runs does not parse.
    #[error("{}: {message}", path.display())]
    Ledger {
        /// The ledger file.
        path: PathBuf,
        /// What is wrong.
        message: String,
    },
    /// A call to Palette failed in a way no check can answer: no key, a transport error, or a
    /// 5xx. A 4xx is an answer, and a check reads it.
    #[error(transparent)]
    Palette(#[from] strata::Error),
    /// A document could not be serialized.
    #[error("serializing: {0}")]
    Serialize(#[from] serde_json::Error),
}

impl Error {
    /// A stable kebab-case name for the variant, for programs that read errors.
    pub fn kind(&self) -> &'static str {
        match self {
            Error::Catalog { .. } => "catalog",
            Error::Jq { .. } => "jq",
            Error::Input { .. } => "input",
            Error::Usage { .. } => "usage",
            Error::Io { .. } => "io",
            Error::Ledger { .. } => "ledger",
            Error::Palette(e) => e.kind(),
            Error::Serialize(_) => "serialize",
        }
    }

    /// The machine-readable form: `kind` and `message`.
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Error::Palette(e) => e.to_json(),
            _ => serde_json::json!({ "kind": self.kind(), "message": self.to_string() }),
        }
    }
}

/// Result alias for the crate.
pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn io(path: &std::path::Path, source: std::io::Error) -> Error {
    Error::Io {
        path: path.to_path_buf(),
        source,
    }
}
