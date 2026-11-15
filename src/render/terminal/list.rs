//! The tables of `list` for the terminal: the profile versions with their packs, the clusters
//! with their profiles, the projects, the saved tag filters, and the registries.

use super::table::{column_widths, matched, painted_text, table_line};
use crate::ops::{
    ListedCluster, ListedPack, ListedProfile, ListedProject, ListedRegistry, Listing, SavedFilter,
};
use crate::reference::name_version;
use crate::render::consumers::consumer_count;
use crate::render::list::{
    applied_ref, cluster_count, list_count, listed_pack_ref, profile_marks, project_label,
    registry_count, registry_kind_cell, tag_note, unregistered_pack, virtual_line, virtual_note,
};
use crate::render::{Kind, count_of, entry_error, paint_state};
use crate::term::color::Paint;
use crate::text::dash;

/// The table of the profile versions of a listing, with its count and the packs of each one.
pub(super) fn list_profiles(out: &mut String, p: &Paint, l: &Listing, profiles: &[ListedProfile]) {
    out.push_str(&format!(
        "\n{}\n",
        list_count(profiles.len(), "profile", l.pack.as_deref())
    ));
    // the marks of a version end its row, so their column moves with the columns before
    let (rows, marked): (Vec<Vec<String>>, Vec<Option<usize>>) = profiles
        .iter()
        .map(|pr| {
            let mut row = vec![
                pr.reference.clone(),
                pr.uid.clone(),
                dash(&pr.cloud_type).to_string(),
            ];
            if let Some(msg) = entry_error(pr.error.as_ref()) {
                row.push(format!("({msg})"));
            } else if pr.packs.is_some() {
                row.push(crate::render::list::profile_type_cell(pr).to_string());
                row.push(consumer_count(pr.consumers.unwrap_or(0)));
            }
            let marks = profile_marks(pr, l);
            let marked = marks.as_ref().map(|_| row.len());
            row.extend(marks);
            (row, marked)
        })
        .unzip();
    let widths = column_widths(&rows);
    for ((pr, row), marked) in profiles.iter().zip(&rows).zip(&marked) {
        let failed = pr.error.is_some();
        out.push_str(&table_line("  ", row, &widths, |i, cell| match i {
            0 => painted_text(cell, |text| Kind::Profile.paint(p, text)),
            1 => p.dim(cell),
            3 if failed => p.dim(cell),
            i if Some(i) == *marked => p.yellow(cell),
            _ => cell.to_string(),
        }));
        let packs: Vec<PackCell> = pr.packs.iter().flatten().map(PackCell::of).collect();
        let rows: Vec<Vec<String>> = packs.iter().map(PackCell::row).collect();
        let pack_widths = column_widths(&rows);
        for (pack, row) in packs.iter().zip(&rows) {
            out.push_str(&table_line("    ", row, &pack_widths, |i, cell| match i {
                0 => p.dim(cell),
                1 => painted_text(cell, |text| pack.paint(p, text, l.pack.as_deref())),
                _ => cell.to_string(),
            }));
        }
    }
}

/// The row of a pack under its profile version: the layer, the ref of the pack, and the tag
/// when the profile pins another name. A pack with no registry names its kind in words.
struct PackCell<'a> {
    pack: &'a ListedPack,
    /// The text of the pack: its ref, as a person types it in a shell, such as
    /// `'pack:Public Repo/kubernetes@1.36.2'`.
    text: String,
    /// Where the name of the pack starts in `text`, so that a match of `--pack` marks only the
    /// name. `None` when the text holds the name in another form, such as an escaped quote.
    name_at: Option<usize>,
}

impl<'a> PackCell<'a> {
    fn of(pack: &'a ListedPack) -> Self {
        // the name and the version end the ref, and they come after the word `pack` in the words
        // of a pack with no registry
        let (text, named) = match listed_pack_ref(pack) {
            Some(r) => (r.shell_word(), name_version(&pack.name, &pack.version)),
            None => (
                unregistered_pack(pack),
                format!("{} {}", pack.name, pack.version),
            ),
        };
        let name_at = text.rfind(&named);
        PackCell {
            pack,
            text,
            name_at,
        }
    }

    fn row(&self) -> Vec<String> {
        let mut row = vec![self.pack.layer.clone(), self.text.clone()];
        // a pack with no tag of its own ends at its ref, with no space after it
        row.extend(tag_note(self.pack));
        row
    }

    /// `text`, the cell of the pack, in the color of a pack, with the part of the name that
    /// `query` matched marked: see [`matched`].
    fn paint(&self, p: &Paint, text: &str, query: Option<&str>) -> String {
        let name = &self.pack.name;
        let marked = self
            .name_at
            .and_then(|at| {
                let (before, rest) = text.split_at_checked(at)?;
                let after = rest.strip_prefix(name.as_str())?;
                Some(format!("{before}{}{after}", matched(p, name, query)))
            })
            .unwrap_or_else(|| text.to_string());
        Kind::Pack.paint(p, &marked)
    }
}

/// The table of the clusters of a listing, with its count, the lines of its query, and the
/// profiles of each cluster.
pub(super) fn list_clusters(out: &mut String, p: &Paint, l: &Listing, clusters: &[ListedCluster]) {
    out.push_str(&format!(
        "\n{}\n",
        cluster_count(clusters.len(), l.cluster.as_deref())
    ));
    for line in crate::render::list::query_line(l)
        .into_iter()
        .chain(virtual_line(l))
    {
        out.push_str(&format!("{}\n", p.dim(&line)));
    }
    let rows: Vec<Vec<String>> = clusters.iter().map(|c| cluster_row(l, c)).collect();
    let widths = column_widths(&rows);
    for (c, row) in clusters.iter().zip(&rows) {
        out.push_str(&table_line("  ", row, &widths, |i, cell| match i {
            0 => painted_text(cell, |text| Kind::Cluster.paint(p, text)),
            1 => matched(p, cell, l.cluster.as_deref()),
            2 => painted_text(cell, |state| paint_state(p, state)),
            _ => cell.to_string(),
        }));
        let applied = applied_rows(c);
        let applied_widths = column_widths(&applied);
        for row in &applied {
            out.push_str(&table_line(
                "    ",
                row,
                &applied_widths,
                |i, cell| match i {
                    0 => painted_text(cell, |text| Kind::Profile.paint(p, text)),
                    1 => p.dim(cell),
                    3 => p.yellow(cell),
                    _ => cell.to_string(),
                },
            ));
        }
    }
}

/// The cells of the row of a cluster.
fn cluster_row(l: &Listing, c: &ListedCluster) -> Vec<String> {
    let mut row = vec![
        c.reference.clone(),
        dash(&c.name).to_string(),
        dash(&c.state).to_string(),
        dash(&c.cloud_type).to_string(),
    ];
    if c.project_uid != l.project_uid {
        row.push(format!(
            "project {}",
            project_label(&c.project_uid, &c.project_name)
        ));
    }
    // a query is a question about labels, so its rows show them
    if l.query.is_some() {
        row.push(crate::render::list::labels_text(&c.labels));
    }
    row.extend(virtual_note(c));
    if let Some(msg) = entry_error(c.error.as_ref()) {
        row.push(format!("({msg})"));
    }
    if let Some(note) = crate::render::list::imported_note(c) {
        row.push(note.to_string());
    }
    row
}

/// The rows of the profiles that a cluster runs, under the row of the cluster.
fn applied_rows(c: &ListedCluster) -> Vec<Vec<String>> {
    c.profiles
        .iter()
        .flatten()
        .map(|a| {
            let mut row = vec![
                applied_ref(a),
                dash(&a.uid).to_string(),
                dash(&a.kind).to_string(),
            ];
            if c.pending_updates.iter().any(|u| u.profile_uid == a.uid) {
                row.push("behind: an update waits".to_string());
            }
            row
        })
        .collect()
}

/// The table of the projects of a listing, with its count.
pub(super) fn list_projects(out: &mut String, p: &Paint, projects: &[ListedProject]) {
    out.push_str(&format!("\n{}\n", count_of(projects.len(), "project")));
    let rows: Vec<Vec<String>> = projects
        .iter()
        .map(|pr| vec![dash(&pr.name).to_string(), pr.uid.clone()])
        .collect();
    let widths = column_widths(&rows);
    for row in &rows {
        out.push_str(&table_line("  ", row, &widths, |i, cell| match i {
            0 => p.bold(cell),
            _ => p.dim(cell),
        }));
    }
}

/// The table of the saved tag filters of a listing, with its count.
pub(super) fn list_filters(out: &mut String, p: &Paint, filters: &[SavedFilter]) {
    out.push_str(&format!(
        "\n{}\n",
        count_of(filters.len(), "saved tag filter")
    ));
    let rows: Vec<Vec<String>> = filters
        .iter()
        .map(|f| vec![f.reference(), f.uid.clone(), f.expression()])
        .collect();
    let widths = column_widths(&rows);
    for row in &rows {
        out.push_str(&table_line("  ", row, &widths, |i, cell| match i {
            0 => p.bold(cell),
            1 => p.dim(cell),
            _ => cell.to_string(),
        }));
    }
}

/// The table of the registries of a listing, with its count.
pub(super) fn list_registries(out: &mut String, p: &Paint, registries: &[ListedRegistry]) {
    out.push_str(&format!("\n{}\n", registry_count(registries.len())));
    let rows: Vec<Vec<String>> = registries
        .iter()
        .map(|r| {
            vec![
                dash(&r.name).to_string(),
                r.uid.clone(),
                registry_kind_cell(r).to_string(),
            ]
        })
        .collect();
    let widths = column_widths(&rows);
    for row in &rows {
        out.push_str(&table_line("  ", row, &widths, |i, cell| match i {
            0 => painted_text(cell, |text| Kind::Registry.paint(p, text)),
            1 => p.dim(cell),
            _ => cell.to_string(),
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::term::color::{ColorMode, Stream};

    fn pack(name: &str, version: &str, tag: &str, registry: &str, uid: &str) -> ListedPack {
        ListedPack {
            layer: "addon".to_string(),
            name: name.to_string(),
            tag: tag.to_string(),
            version: version.to_string(),
            registry_uid: uid.to_string(),
            registry: registry.to_string(),
        }
    }

    #[test]
    fn a_pack_row_gives_the_ref_of_the_pack_and_marks_a_match_of_its_name_alone() {
        let packs = [
            pack("csi-aws-ebs", "1.65.0", "1.65.0", "Public Repo", "r1"),
            pack("ubuntu-aws", "24.04", "LTS__24.x", "Public Repo", "r1"),
            // a registry with a `/` in its name is its uid in a ref
            pack("repo-tools", "1.0.0", "1.0.0", "Repo/Mirror", "r2"),
            pack("namespace-labels", "1.0.0", "1.0.0", "", ""),
        ];
        let cells: Vec<PackCell> = packs.iter().map(PackCell::of).collect();
        let rows: Vec<Vec<String>> = cells.iter().map(PackCell::row).collect();
        assert_eq!(
            rows,
            [
                vec!["addon", "'pack:Public Repo/csi-aws-ebs@1.65.0'"],
                vec![
                    "addon",
                    "'pack:Public Repo/ubuntu-aws@24.04'",
                    "(tag LTS__24.x)"
                ],
                vec!["addon", "pack:r2/repo-tools@1.0.0"],
                vec!["addon", "pack namespace-labels 1.0.0 (no registry)"],
            ]
        );
        // each ref of a row pastes back as written: a shell gives strata the ref of the pack
        for (cell, p) in cells.iter().zip(&packs).take(3) {
            let unquoted = cell.text.trim_matches('\'');
            let parsed = crate::reference::Ref::parse_with(unquoted, |_| false);
            assert_eq!(parsed.shell_word(), cell.text);
            assert!(
                matches!(parsed, crate::reference::Ref::Pack { ref name, .. } if *name == p.name)
            );
        }

        // the match of `--pack` is in the name, and not in the registry, in the color of a pack
        let on = Paint::resolve(ColorMode::Always, Stream::Stdout);
        let query = Some("repo");
        assert_eq!(
            cells[0].paint(&on, &cells[0].text, query),
            on.cyan(&cells[0].text)
        );
        assert_eq!(
            cells[2].paint(&on, &cells[2].text, query),
            on.cyan(&format!("pack:r2/{}-tools@1.0.0", on.byellow("repo")))
        );
        assert_eq!(
            cells[3].paint(&on, &cells[3].text, Some("labels")),
            on.cyan(&format!(
                "pack namespace-{} 1.0.0 (no registry)",
                on.byellow("labels")
            ))
        );
        // with no color, the text of each cell is the text of its row
        for cell in &cells {
            assert_eq!(cell.paint(&Paint::off(), &cell.text, query), cell.text);
        }
    }

    #[test]
    fn the_table_of_clusters_paints_the_state_of_each_row_and_not_its_padding() {
        let on = Paint::resolve(ColorMode::Always, Stream::Stdout);
        let listing = Listing {
            schema: crate::ops::LIST_SCHEMA.to_string(),
            tool: crate::snapshot::Tool::current(),
            generated_at: "2026-11-02T00:00:00Z".to_string(),
            project_uid: String::new(),
            project_name: String::new(),
            pack: None,
            cluster: None,
            profiles: None,
            clusters: None,
            includes_virtual: None,
            projects: None,
            query: None,
            resolved: None,
            query_filter: None,
            filters: None,
            registries: None,
            errors: std::collections::BTreeMap::new(),
        };
        let clusters: Vec<ListedCluster> = ["Running", "Provisioning", "Error"]
            .into_iter()
            .map(|state| {
                serde_json::from_value(serde_json::json!({
                    "ref": crate::reference::Ref::cluster(state).to_string(),
                    "name": state,
                    "uid": state,
                    "state": state,
                    "cloudType": "aws",
                    "projectUid": "",
                }))
                .unwrap()
            })
            .collect();
        let mut out = String::new();
        list_clusters(&mut out, &on, &listing, &clusters);
        for (state, painted) in [
            ("Running", on.green("Running")),
            ("Provisioning", on.yellow("Provisioning")),
            ("Error", on.red("Error")),
        ] {
            // the renderer paints the state whatever the width of its column, and not its padding
            let padding = " ".repeat("Provisioning".len() - state.len());
            let cell = format!("{painted}{padding}  aws");
            assert!(out.contains(&cell), "{state}: {out:?}");
        }
        let mut plain = String::new();
        list_clusters(&mut plain, &Paint::off(), &listing, &clusters);
        assert!(!plain.contains('\x1b'), "{plain:?}");
        assert!(plain.contains("Running       aws"), "{plain:?}");
    }
}
