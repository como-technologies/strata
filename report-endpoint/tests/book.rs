//! The page of the book about the endpoint states what the code does: the settings that the
//! service reads, and the limits that it keeps.

use strata_report_endpoint::server::{
    MOST_AT_ONCE, NEW_ISSUES_PER_HOUR, REQUESTS_PER_HOUR, WRITES_PER_HOUR,
};

mod common;

use common::read;

fn page() -> String {
    read("../docs/src/development/report-endpoint.md")
}

/// The text of the section `heading` of `page`, up to the next section.
fn section<'a>(page: &'a str, heading: &str) -> &'a str {
    let start = page
        .find(&format!("\n## {heading}\n"))
        .unwrap_or_else(|| panic!("the page has no section {heading:?}"));
    let rest = &page[start + 1..];
    let end = rest[3..].find("\n## ").map_or(rest.len(), |at| at + 3);
    &rest[..end]
}

/// The names of the variables that `settings.rs` reads: each quoted word of capital letters and
/// underscores before the tests. A `CARGO_` variable is read when the crate builds, so it is not a
/// setting of the service.
fn variables_of_the_code() -> Vec<String> {
    let source = read("src/settings.rs");
    let code = source.split("#[cfg(test)]").next().unwrap();
    code.split('"')
        .skip(1)
        .step_by(2)
        .filter(|word| word.len() > 2 && word.chars().all(|c| c.is_ascii_uppercase() || c == '_'))
        .filter(|word| !word.starts_with("CARGO_"))
        .map(str::to_string)
        .collect()
}

#[test]
fn no_two_sections_of_the_page_have_the_same_heading() {
    let page = page();
    let mut headings: Vec<&str> = page
        .lines()
        .filter(|line| line.starts_with("## "))
        .collect();
    let total = headings.len();
    headings.sort_unstable();
    headings.dedup();
    assert_eq!(headings.len(), total, "a heading repeats on the page");
}

#[test]
fn the_settings_section_lists_each_variable_that_the_service_reads_and_says_when_the_header_counts()
{
    let page = page();
    let settings = section(&page, "The settings of the service");
    let names = variables_of_the_code();
    assert!(names.len() >= 7, "{names:?}");
    for name in &names {
        assert!(
            settings.contains(&format!("`{name}`")),
            "{name} is not in the section"
        );
    }
    // the service reads the header only with the setting, and the page must not say that it always does
    assert!(settings.contains("`TRUST_FORWARDED_FOR`"));
    assert!(
        settings.contains("Without `TRUST_FORWARDED_FOR=1`"),
        "the section says what the service does without the setting"
    );
    assert!(
        !page.contains("The service reads the sender from the last entry of `X-Forwarded-For`, which the front end of"),
        "a second text says that the service always reads the header"
    );
}

#[test]
fn the_page_states_the_limits_and_the_most_requests_at_once_of_the_code() {
    let page = page();
    let limits = section(&page, "The limits");
    for (name, row) in [
        (
            "requests of one sender",
            format!("| Requests of one sender | {REQUESTS_PER_HOUR} in an hour |"),
        ),
        (
            "new issues",
            format!("| New issues | {NEW_ISSUES_PER_HOUR} in an hour |"),
        ),
        (
            "writes",
            format!("| Writes: new issues and changes | {WRITES_PER_HOUR} in an hour |"),
        ),
    ] {
        assert!(limits.contains(&row), "{name}: the section lacks {row:?}");
    }
    let threads = section(&page, "The locks and the threads");
    assert!(
        threads.contains(&format!("handles {MOST_AT_ONCE} requests at a time")),
        "{threads}"
    );
    let request = section(&page, "The checks of a request");
    assert!(
        request.contains(&format!(
            "| 4 | The sender made {REQUESTS_PER_HOUR} requests or fewer in the last hour."
        )),
        "{request}"
    );
}
