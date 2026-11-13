//! Access to the Palette API behind one trait, so that every operation can run against canned
//! responses in tests and against HTTP in production.
//!
//! The modules: [`PaletteApi`] is the one GET that everything else is built on; [`HttpApi`] sends
//! it over HTTP, [`Retry`] tries it again, and [`MemoryApi`] answers it from canned responses in
//! tests. [`Palette`] holds the operations over any of them, and [`RawProfile`] and [`RawCluster`]
//! are the responses a snapshot is built from. [`codes`] holds the codes of the answers of
//! Palette: the refusals of each validate route, and the rules of the create.
//! [`connect_from_env`] builds the client that a command line uses.

mod audit;
pub mod codes;
mod connect;
mod download;
mod http;
pub(crate) mod mask;
mod memory;
mod palette;
mod raw;
pub(crate) mod registry;
mod retry;

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::closed_set::{ClosedSet, impl_closed_set};
use crate::error::{Error, Result};

pub use audit::AuditKind;
pub use connect::{connect_from_env, connect_with, project_uid_named};
pub use download::{DownloadedPack, PackArchive, PackFile, ProfileDownload};
pub use http::{API_KEY_HEADER, HttpApi, PROJECT_UID_HEADER};
pub use memory::MemoryApi;
pub(crate) use palette::{
    CLUSTER_TEMPLATE, DOWNLOAD_INTERVAL_SECONDS, OCI_SUMMARY_TEMPLATE, PACK_LISTING_PAGE, PACKS,
    PROFILE_TEMPLATE, REGISTRY_METADATA, SEARCH_CLUSTER_PROFILES, SEARCH_TAGS, TIER_INSTALL_ORDER,
    is_download_limit, named_entries, profile_versions, search_condition, search_filter,
    search_group,
};
// the unit tests build the route of a download with the one function that builds it
#[cfg(test)]
pub(crate) use palette::download_path;
pub use palette::{
    Palette, VariablesRead, cloud_account_path, cloud_config_path, cluster_events_path,
    cluster_summary_path, subject_path,
};
pub use raw::{RawCluster, RawProfile, ReadErrors, has_draft, profile_variables};
pub(crate) use raw::{recorded_packs, recorded_template, save_registries, saved_registries};
pub use retry::{Retry, RetryPolicy};

/// The method of a request that strata sends to Palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum Method {
    /// A read of a document: [`PaletteApi::get_json`].
    Get,
    /// A question to a route that checks a body: [`PaletteApi::post_json`].
    Post,
}

impl_closed_set!(Method {
    Get => "GET",
    Post => "POST",
});

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One GET of a JSON document from Palette. Implementations add authentication and scoping.
pub trait PaletteApi {
    /// GET `path` (for example `/v1/clusterprofiles/{uid}`) and parse the body as JSON.
    ///
    /// # Errors
    ///
    /// [`Error::Api`] when Palette answers with a status that is not 2xx. [`Error::Transport`] when
    /// the request does not complete, and [`Error::NotJson`] when a 2xx answer is not JSON.
    fn get_json(&self, path: &str) -> Result<Value>;

    /// POST `body` to `path` and parse the answer as JSON. Strata sends a POST only to ask
    /// Palette a question, at a route that checks the body. Such a route changes no setting. At
    /// `validate/repave`, Palette can save the address of the console of a pack of virtual
    /// machines: see [`Palette::validate_repave`]. An API that cannot POST says so.
    ///
    /// # Errors
    ///
    /// The errors of [`PaletteApi::get_json`]. An API that cannot POST gives [`Error::Transport`].
    fn post_json(&self, path: &str, _body: &Value) -> Result<Value> {
        Err(Error::Transport {
            method: Method::Post,
            path: path.to_string(),
            message: "this API sends no POST".to_string(),
        })
    }

    /// GET `path` as a file, and give the bytes of the body. Palette sends a file, such as the
    /// download of a profile version, only to a request that accepts a file.
    ///
    /// # Errors
    ///
    /// [`Error::Api`] and [`Error::Transport`], as for [`PaletteApi::get_json`]. An API that reads
    /// no file gives [`Error::Transport`].
    fn get_file(&self, path: &str) -> Result<Vec<u8>> {
        Err(Error::Transport {
            method: Method::Get,
            path: path.to_string(),
            message: "this API reads no file".to_string(),
        })
    }

    /// The project the requests are scoped to, when they are; snapshots record it.
    fn project_uid(&self) -> Option<&str> {
        None
    }
}

impl<A: PaletteApi + ?Sized> PaletteApi for &A {
    fn get_json(&self, path: &str) -> Result<Value> {
        (**self).get_json(path)
    }

    fn post_json(&self, path: &str, body: &Value) -> Result<Value> {
        (**self).post_json(path, body)
    }

    fn get_file(&self, path: &str) -> Result<Vec<u8>> {
        (**self).get_file(path)
    }

    fn project_uid(&self) -> Option<&str> {
        (**self).project_uid()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_method_prints_as_its_http_name_and_reads_no_other_text() {
        for &method in Method::ALL {
            assert_eq!(method.to_string(), method.as_str());
        }
        assert!(serde_json::from_value::<Method>(Value::from("get")).is_err());
    }
}
