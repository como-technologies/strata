//! `strata schema`: print the JSON Schema of a document or of the candidate file.

use std::process::ExitCode;

use strata::error::Result;
use strata::snapshot::Snapshot;
use strata::{diff, ops};

use super::args::SchemaKind;

pub(crate) fn cmd_schema(kind: SchemaKind) -> Result<ExitCode> {
    let schema = match kind {
        SchemaKind::Snapshot => schemars::schema_for!(Snapshot),
        SchemaKind::Diff => schemars::schema_for!(diff::Diff),
        SchemaKind::VerifySummary => schemars::schema_for!(ops::VerifySummary),
        SchemaKind::Chain => schemars::schema_for!(ops::Chain),
        SchemaKind::Consumers => schemars::schema_for!(ops::ConsumersReport),
        SchemaKind::Behind => schemars::schema_for!(ops::BehindReport),
        SchemaKind::Set => schemars::schema_for!(ops::SetReport),
        SchemaKind::Report => schemars::schema_for!(strata::closed_set::unknown::report::Report),
        SchemaKind::List => schemars::schema_for!(ops::Listing),
        SchemaKind::Packs => schemars::schema_for!(ops::PackListing),
        SchemaKind::Candidate => schemars::schema_for!(strata::candidate::Candidate),
    };
    println!("{}", strata::text::schema_json(&schema)?);
    Ok(ExitCode::SUCCESS)
}
