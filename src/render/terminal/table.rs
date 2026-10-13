//! The tables of the terminal: the width of each column, and the cells that match a query.

use crate::term::color::Paint;

/// The width of each column: its widest cell, leaving out the last cell of each row, which is
/// never padded.
pub(super) fn column_widths(rows: &[Vec<String>]) -> Vec<usize> {
    let mut widths = Vec::new();
    for row in rows {
        for (i, cell) in row.iter().enumerate().take(row.len().saturating_sub(1)) {
            if widths.len() <= i {
                widths.resize(i + 1, 0);
            }
            widths[i] = widths[i].max(cell.chars().count());
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

/// One table line: cells padded to their column and two spaces apart, the last cell unpadded so
/// no line ends in spaces. `style` colors a padded cell by its column.
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
            if i < last {
                let width = widths.get(i).copied().unwrap_or(0);
                style(i, &format!("{cell:<width$}"))
            } else {
                style(i, cell)
            }
        })
        .collect();
    format!("{indent}{}\n", cells.join("  "))
}
