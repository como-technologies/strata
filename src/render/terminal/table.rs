//! The tables of the terminal: the width of each column, and the cells that match a query.
//!
//! A cell can hold a text of Palette, such as a name. So the width, the padding, and the paint of
//! a cell read the cell as [`terminal_text`] shows it.

use crate::term::color::Paint;
use crate::text::terminal_text;

/// The width of each column: its widest cell. The last cell of each row does not count, as strata
/// never pads it.
pub(super) fn column_widths(rows: &[Vec<String>]) -> Vec<usize> {
    let mut widths = Vec::new();
    for row in rows {
        for (i, cell) in row.iter().enumerate().take(row.len().saturating_sub(1)) {
            if widths.len() <= i {
                widths.resize(i + 1, 0);
            }
            widths[i] = widths[i].max(terminal_text(cell).chars().count());
        }
    }
    widths
}

/// The pack name with the part that `--pack` matched in bold yellow, the way a search tool marks
/// a match. The cell is already padded, so the escape codes do not move the column. Text that is
/// not ASCII is left alone: lowercasing can change its length, and the match would slice wrongly.
pub(super) fn matched(p: &Paint, cell: &str, text: Option<&str>) -> String {
    let Some(text) = text.filter(|t| !t.is_empty()) else {
        return cell.to_string();
    };
    if !cell.is_ascii() || !text.is_ascii() {
        return cell.to_string();
    }
    match cell.to_lowercase().find(&text.to_lowercase()) {
        Some(at) => format!(
            "{}{}{}",
            &cell[..at],
            p.byellow(&cell[at..at + text.len()]),
            &cell[at + text.len()..]
        ),
        None => cell.to_string(),
    }
}

/// A padded cell with `paint` applied to its text. The padding stays outside the paint, so the
/// text reads as the value that the report holds.
pub(super) fn painted_text(cell: &str, paint: impl Fn(&str) -> String) -> String {
    let text = cell.trim_end();
    format!("{}{}", paint(text), &cell[text.len()..])
}

/// One table line: cells padded to their column and two spaces apart, the last cell unpadded so
/// no line ends in spaces. `style` colors a padded cell by its column. Each cell is
/// [`terminal_text`], so a control character of a cell shows as a visible escape.
pub(super) fn table_line(
    indent: &str,
    row: &[String],
    widths: &[usize],
    style: impl Fn(usize, &str) -> String,
) -> String {
    let last = row.len().saturating_sub(1);
    let cells: Vec<String> = row
        .iter()
        .enumerate()
        .map(|(i, cell)| {
            let cell = terminal_text(cell);
            if i < last {
                let width = widths.get(i).copied().unwrap_or(0);
                style(i, &format!("{cell:<width$}"))
            } else {
                style(i, &cell)
            }
        })
        .collect();
    format!("{indent}{}\n", cells.join("  "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::term::color::{ColorMode, Stream};

    #[test]
    fn the_part_of_a_pack_name_that_matched_is_marked() {
        let on = Paint::resolve(ColorMode::Always, Stream::Stdout);
        assert_eq!(
            matched(&on, "cni-cilium-oss", Some("cilium")),
            format!("cni-{}-oss", on.byellow("cilium")),
            "the match is marked, and the rest of the name is not"
        );
        // the search ignores case, and the name keeps its own
        assert_eq!(
            matched(&on, "cni-Cilium-oss", Some("cilium")),
            format!("cni-{}-oss", on.byellow("Cilium"))
        );
        // the padding of the column stays in the cell
        assert_eq!(
            matched(&on, "csi-aws-ebs   ", Some("aws")),
            format!("csi-{}-ebs   ", on.byellow("aws"))
        );

        // nothing to mark, nothing to do
        assert_eq!(matched(&on, "kubernetes", Some("cilium")), "kubernetes");
        assert_eq!(matched(&on, "kubernetes", None), "kubernetes");
        assert_eq!(matched(&on, "kubernetes", Some("")), "kubernetes");
        // no color: the plain name, so a pipe and a file read the same
        let off = Paint::off();
        assert_eq!(
            matched(&off, "cni-cilium-oss", Some("cilium")),
            "cni-cilium-oss"
        );
        // text that is not ASCII is left alone, because lowercasing can change its length
        assert_eq!(matched(&on, "pack-ß", Some("ß")), "pack-ß");
        assert_eq!(matched(&on, "cilium", Some("CILIUM")), on.byellow("cilium"));
    }

    #[test]
    fn a_cell_of_a_table_shows_each_control_character_as_a_visible_escape_in_its_column() {
        let rows = vec![
            vec!["a\u{1b}[2Kb".to_string(), "x\ny".to_string()],
            vec!["longer name".to_string(), "z".to_string()],
        ];
        let widths = column_widths(&rows);
        // the escape `\u{1b}` is six characters wide, so the first cell is eleven wide
        assert_eq!(widths, [11]);
        let lines: Vec<String> = rows
            .iter()
            .map(|row| table_line("  ", row, &widths, |_, cell| cell.to_string()))
            .collect();
        assert_eq!(
            lines,
            [
                r"  a\u{1b}[2Kb  x\ny".to_string() + "\n",
                "  longer name  z\n".to_string()
            ]
        );
    }
}
