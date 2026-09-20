//! Split `docs/backlog.md` into one issue for each section.
//!
//! `scripts/dev/open-backlog-issues.sh` reads the output: one line for each section, as
//! `title<TAB>labels<TAB>body_file`. The body is written to a temporary file, because it holds
//! newlines and `gh issue create --body-file` takes a path.

use std::io::Write;
use std::path::Path;

/// Write one `title<TAB>labels<TAB>body_file` line for each `## ` section of `backlog`.
pub fn split(backlog: &Path, temp_dir: &Path) -> std::io::Result<()> {
    let text = std::fs::read_to_string(backlog)?;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for (index, section) in sections(&text).into_iter().enumerate() {
        let path = temp_dir.join(format!("backlog-{index}.md"));
        std::fs::write(&path, &section.body)?;
        writeln!(
            out,
            "{}\t{}\t{}",
            section.title,
            section.labels,
            path.display()
        )?;
    }
    Ok(())
}

struct Section {
    title: String,
    labels: String,
    body: String,
}

/// Each `## ` heading starts a section. A `labels:` line anywhere in it names the labels and
/// leaves the body; the file writes `### ` for the headings inside a section, so that no heading
/// inside one is mistaken for the start of the next, and the body gets them back as `## `.
fn sections(text: &str) -> Vec<Section> {
    let mut out = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        let Some(title) = line.strip_prefix("## ") else {
            continue;
        };
        let mut labels = String::new();
        let mut body = String::new();
        while let Some(next) = lines.peek() {
            if next.starts_with("## ") {
                break;
            }
            let next = lines.next().expect("peeked");
            if let Some(rest) = next.strip_prefix("labels:") {
                labels = rest.trim().to_string();
                continue;
            }
            match next.strip_prefix("### ") {
                Some(heading) => body.push_str(&format!("## {heading}\n")),
                None => {
                    body.push_str(next);
                    body.push('\n');
                }
            }
        }
        out.push(Section {
            title: title.trim().to_string(),
            labels,
            body: body.trim().to_string() + "\n",
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const BACKLOG: &str = "\
# Backlog

Some words before any section.

## A first thing
labels: feature, ux

The body of the first thing.

### A heading inside it

More words.

## A second thing

No labels on this one.
";

    #[test]
    fn each_heading_at_two_hashes_is_one_section() {
        let got = sections(BACKLOG);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].title, "A first thing");
        assert_eq!(got[1].title, "A second thing");
    }

    #[test]
    fn a_labels_line_names_the_labels_and_leaves_the_body() {
        let got = sections(BACKLOG);
        assert_eq!(got[0].labels, "feature, ux");
        assert_eq!(got[1].labels, "");
        assert!(!got[0].body.contains("labels:"));
    }

    /// The file writes `### ` inside a section so that only a real section starts with `## `.
    /// An issue body has no such constraint, so each one moves up.
    #[test]
    fn a_heading_inside_a_section_moves_up_one_level() {
        let got = sections(BACKLOG);
        assert!(got[0].body.contains("## A heading inside it"));
        assert!(!got[0].body.contains("### "));
    }

    #[test]
    fn the_words_before_the_first_section_are_in_no_section() {
        let got = sections(BACKLOG);
        assert!(!got[0].body.contains("Some words before"));
    }
}
