//! The target of the candidate file: the parse, and the check before any request.

use strata::candidate;

use crate::support::{answer, scratch, write};

/// Load `bytes` as the candidate file `candidate.yaml`. Return true when the file passes the
/// check.
///
/// A file that passes is a candidate that strata writes the same way: the YAML of the candidate
/// loads as the same candidate. The file names no values file or manifest file that exists. So a
/// layer with a `valuesFile` is an error of the read. The target checks the layers that hold their
/// content.
pub(crate) fn file(bytes: &[u8]) -> bool {
    let dir = scratch();
    write(&dir, "candidate.yaml", bytes);
    let Some(loaded) = answer(candidate::load(&dir.join("candidate.yaml"))) else {
        return false;
    };
    let text = answer(candidate::to_yaml(&loaded.candidate))
        .expect("a candidate that loaded is written as YAML");
    write(&dir, "again.yaml", text.as_bytes());
    let again = answer(candidate::load(&dir.join("again.yaml")))
        .unwrap_or_else(|| panic!("the YAML of a candidate that loaded does not load:\n{text}"));
    assert_eq!(
        again.candidate, loaded.candidate,
        "a candidate and its YAML are two candidates:\n{text}"
    );
    true
}

#[test]
fn a_candidate_file_loads_as_the_candidate_it_writes_or_is_refused_with_a_kind() {
    bolero::check!().for_each(|bytes| {
        file(bytes);
    });
}
