//! Access to the Palette API behind one trait, so that every operation can run against canned
//! responses in tests and against HTTP in production.
//!
//! The modules: [`PaletteApi`] is the one GET that everything else is built on; [`HttpApi`] sends
//! it over HTTP, [`Retry`] tries it again, and [`MemoryApi`] answers it from canned responses in
//! tests. [`Palette`] holds the operations over any of them, and [`RawProfile`] and [`RawCluster`]
//! are the responses a snapshot is built from.

mod http;
mod memory;
mod palette;
mod raw;
mod retry;

use serde_json::Value;

use crate::error::Result;

pub use http::HttpApi;
pub use memory::MemoryApi;
pub use palette::Palette;
pub use raw::{RawCluster, RawProfile};
pub use retry::{Retry, RetryPolicy};

/// One GET of a JSON document from Palette. Implementations add authentication and scoping.
pub trait PaletteApi {
    /// GET `path` (for example `/v1/clusterprofiles/{uid}`) and parse the body as JSON.
    fn get_json(&self, path: &str) -> Result<Value>;

    /// The project the requests are scoped to, when they are; snapshots record it.
    fn project_uid(&self) -> Option<&str> {
        None
    }
}

impl<A: PaletteApi + ?Sized> PaletteApi for &A {
    fn get_json(&self, path: &str) -> Result<Value> {
        (**self).get_json(path)
    }

    fn project_uid(&self) -> Option<&str> {
        (**self).project_uid()
    }
}
