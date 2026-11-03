//! The regression cases of the targets: the inputs that made a target fail, kept in `crashes/`.

/// A crash that a fuzzer found, or a bug that a review found, kept as a regression case. A case
/// has the target, the name of the file in `crashes/`, and the input. Each case is a bug that has
/// a fix and a unit test in `src/` or in `preflight/src/`.
pub(super) fn cases() -> Vec<(&'static str, &'static str, Vec<u8>)> {
    let draft = "mask::a_draft_pack_keeps_no_secret_in_clear_and_nothing_else_changes";
    let palette =
        "palette::an_answer_of_palette_of_any_shape_gives_a_record_or_an_error_with_a_kind";
    let a_ref = "reference::the_text_of_a_ref_parses_as_a_ref_that_prints_the_same_text";
    let jq = "preflight::a_jq_program_compiles_and_runs_or_is_refused_with_a_kind";
    let world = "preflight::a_world_file_gives_the_document_of_a_run_or_is_refused_with_a_kind";
    // a program of 3000 levels: jaq reads each level with a call, and the stack ends first
    let deep = format!("{}1{}", "(".repeat(3000), ")".repeat(3000));
    // the same, with a keyword or a comment for each level, and no bracket that the guard counts
    let levels = |open: &str, middle: &str, close: &str| {
        format!("{}{middle}{}", open.repeat(3000), close.repeat(3000)).into_bytes()
    };
    vec![
        // The mask of a draft cut the values of a user at a byte that is not a `char` boundary.
        // This happened for text that has a multi-byte letter where a variable of the profile has
        // a dot.
        (
            draft,
            "wave-3-mask-slices-a-char",
            "adminPassword: \"{{ .spectroévar.x }}\"\nsync:\n  password: \"{{ .spectroévar.y }}\"\n"
                .into(),
        ),
        // The write of a field into a document of Palette panicked for a `spec` that is a number.
        (palette, "a-document-that-is-a-number", b"5".to_vec()),
        (
            palette,
            "a-spec-that-is-a-number",
            br#"{"metadata": {"uid": "c1"}, "spec": 5, "packs": [{"metadata": {"name": "pack"}, "spec": 5}]}"#
                .to_vec(),
        ),
        // A listing named its next page by the token of the page itself, and the walk never ended.
        (
            palette,
            "a-listing-that-names-its-own-page-as-the-next",
            br#"{"items": [{"metadata": {"name": "a", "uid": "u"}}], "listmeta": {"continue": "x"}}"#
                .to_vec(),
        ),
        // A ref that prints as the text of another ref: no registry, and a `/` in the name.
        (a_ref, "pack-ref-with-no-registry-and-a-slash", b"pack://x@1".to_vec()),
        (a_ref, "pack-ref-with-a-slash-in-the-name", b"pack:/xergroup:a/b@1".to_vec()),
        // A cluster ref with a slash after `@pending` was the uid `c1@pending`, and it printed as the
        // text of the update that waits for `c1`.
        (a_ref, "cluster-ref-with-a-slash-after-pending", b"cluster:c1@pending/".to_vec()),
        // A tag ref with no label was a set of clusters, and it printed as `tag:`, which is a uid.
        (a_ref, "tag-ref-with-no-label", b"tag:+".to_vec()),
        (jq, "a-program-of-3000-levels", deep.into_bytes()),
        (jq, "a-program-of-3000-tries", levels("try ", "[]", "")),
        (jq, "a-program-of-3000-ands", levels("true and ", "true", "")),
        (jq, "a-program-of-3000-ifs", levels("if . then ", "1", " else 2 end")),
        (jq, "a-program-of-3000-defs", levels("def f: ", ".", "; f")),
        (jq, "a-program-of-3000-comments", levels("(# )\n", "1", ")")),
        // The validate body of an existing cluster wrote its name into a `metadata` that is a number,
        // and the index of `serde_json` panicked.
        (
            world,
            "a-cluster-uid-and-a-metadata-that-is-a-number",
            br#"{"clusterUid": "c1", "entity": {"metadata": 5, "spec": {}}}"#.to_vec(),
        ),
    ]
}
