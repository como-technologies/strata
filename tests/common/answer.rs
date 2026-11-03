//! The checks that `tests/fuzz.rs` and the package `fuzz/` share: an error is an answer, and the
//! text of a ref parses as the same ref.
//!
//! `tests/common/mod.rs` re-exports this file for the tests, and the library of `fuzz/` includes
//! it with `#[path]`, so that one rule holds for both.

use std::fmt::Display;

use serde_json::Value;
use strata::reference::Ref;

/// An error that a check reads, of strata or of strata-preflight.
pub trait Refusal: Display {
    /// The kind of the error.
    fn kind(&self) -> &'static str;

    /// The JSON form of the error.
    fn to_json(&self) -> Value;

    /// The exit code of the error, or `None` when the program gives no code for it.
    fn exit_code(&self) -> Option<u8>;
}

impl Refusal for strata::Error {
    fn kind(&self) -> &'static str {
        strata::Error::kind(self)
    }

    fn to_json(&self) -> Value {
        strata::Error::to_json(self)
    }

    fn exit_code(&self) -> Option<u8> {
        Some(strata::Error::exit_code(self))
    }
}

/// What makes `error` no answer, or `None` when it is one.
///
/// An error is an answer when it has a kind, a JSON form with that kind, and the exit code `2` when
/// it has a code: the person can read it. A panic is not an answer.
pub fn answer_fault<E: Refusal>(error: &E) -> Option<String> {
    if error.kind().is_empty() {
        return Some(format!("an error has a kind: {error}"));
    }
    if let Some(code) = error.exit_code().filter(|&code| code != 2) {
        return Some(format!(
            "an error exits with 2, and not with {code}: {error}"
        ));
    }
    (error.to_json()["kind"] != error.kind())
        .then(|| format!("the JSON form names the kind: {error}"))
}

/// The ref that `text` parses as, when it names no file; or what is wrong with the ref.
///
/// The text that the ref prints parses as the same ref. So a ref in a record, a message, or a
/// script names the same thing each time that a person or strata reads it. A check of the printed
/// text alone is not enough: `cluster:c1@pending/` is the uid `c1@pending`, which prints as the
/// text of the update that waits for `c1`.
///
/// # Errors
///
/// The sentence that names the text, when the ref prints as nothing for a text that is not empty,
/// or prints as the text of another ref.
pub fn ref_fault(text: &str) -> Result<Ref, String> {
    let r = Ref::parse_with(text, |_| false);
    let said = r.to_string();
    if said.is_empty() && !text.is_empty() {
        return Err(format!("{text:?} printed as nothing"));
    }
    let again = Ref::parse_with(&said, |_| false);
    if again != r {
        return Err(format!(
            "{text:?} is {r:?}, and it printed as {said:?}, which parses as another ref"
        ));
    }
    Ok(r)
}
