//! The readers of a TOML file, such as a `Cargo.toml` or `riff.toml`: the lines of a table, and the
//! entries `key = "value"`.
//!
//! `tests/common/mod.rs` re-exports this file for the tests of strata. xtask and the tests of
//! strata-report-endpoint include it with `#[path]`. So each package reads a TOML file with one
//! reader.

/// The lines of the table `table` of the TOML text `text`, such as `[lints.clippy]`: each line
/// after its header, up to the next header. The lines keep their comments and their indent.
pub fn toml_table<'a>(text: &'a str, table: &str) -> Vec<&'a str> {
    let mut lines = text.lines().skip_while(|line| line.trim() != table);
    assert!(lines.next().is_some(), "the TOML text has no table {table}");
    lines
        .take_while(|line| !line.trim_start().starts_with('['))
        .collect()
}

/// The entries `key = "value"` among `lines`, such as the lines of a table of a TOML file. A line
/// with another form of value, such as a number or a list, is not an entry here.
pub fn toml_entries<'a>(lines: impl IntoIterator<Item = &'a str>) -> Vec<(&'a str, &'a str)> {
    lines
        .into_iter()
        .filter_map(|line| {
            let (key, value) = line.split_once(" = ")?;
            Some((key, value.strip_prefix('"')?.strip_suffix('"')?))
        })
        .collect()
}

/// The value of the first entry `key = "value"` among `lines`.
pub fn toml_value<'a>(lines: impl IntoIterator<Item = &'a str>, key: &str) -> Option<&'a str> {
    toml_entries(lines)
        .into_iter()
        .find_map(|(name, value)| (name == key).then_some(value))
}
