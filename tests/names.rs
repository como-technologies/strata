//! The neutral names of a set of clusters (#702). The flags, the code, the routes, the page, and the book say "a set
//! of clusters" and "the cluster where the observer runs". "hub" and "spoke" occur only in an example: a code span, a
//! code block, a file of `docs/src/output/`, or a file of `examples/`.

use std::path::Path;

use crate::common::{outside_code_blocks, repo_file, walk, without_code};

/// The words that only an example holds, in lower case, with their plurals.
const EXAMPLE_WORDS: [&str; 4] = ["hub", "hubs", "spoke", "spokes"];

/// The word before "hub" in the name of a registry. "Docker Hub" is no word of a set of clusters.
const REGISTRY_WORD: &str = "docker";

/// Each line of `text` that says one of [`EXAMPLE_WORDS`] as a whole word, in any case, with its number from 1.
fn example_words(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let words: Vec<String> = line
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(str::to_lowercase)
            .collect();
        for (j, w) in words.iter().enumerate() {
            let registry = j > 0 && words[j - 1] == REGISTRY_WORD;
            if EXAMPLE_WORDS.contains(&w.as_str()) && !registry {
                out.push((i + 1, line.trim().to_string()));
            }
        }
    }
    out
}

/// The text of each place that names a set of clusters: each page of the book outside its examples, and the code of the
/// flags, of the routes, and of the page of the observer.
fn places() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut out = Vec::new();
    for path in walk(&root.join("docs/src")) {
        let rel = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .to_string();
        if rel.starts_with("docs/src/output/") || path.extension().is_none_or(|e| e != "md") {
            continue;
        }
        out.push((
            rel.clone(),
            without_code(&outside_code_blocks(&repo_file(&rel))),
        ));
    }
    for path in walk(&root.join("observer/internal/web/assets")) {
        let rel = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .to_string();
        if path
            .extension()
            .is_some_and(|e| e == "js" || e == "html" || e == "css")
        {
            out.push((rel.clone(), repo_file(&rel)));
        }
    }
    for rel in [
        "observer/main.go",
        "observer/internal/api/api.go",
        "src/cli/args.rs",
    ] {
        out.push((rel.to_string(), repo_file(rel)));
    }
    out
}

#[test]
fn no_flag_route_text_of_the_page_or_page_of_the_book_says_hub_or_spoke_outside_an_example() {
    let mut faults = Vec::new();
    for (file, text) in places() {
        for (line, words) in example_words(&text) {
            faults.push(format!("{file}:{line}: {words}"));
        }
    }
    assert!(
        faults.is_empty(),
        "say \"a set of clusters\" and \"the cluster where the observer runs\": {faults:#?}"
    );
}

#[test]
fn the_check_of_the_names_finds_a_whole_word_in_each_case_and_not_the_name_of_a_registry() {
    let found = |text: &str| example_words(text).len();
    assert_eq!(found("the hub runs the observer"), 1);
    assert_eq!(found("Two SPOKES, and one Hub."), 2);
    assert_eq!(found("a spoke-cluster"), 1);
    assert_eq!(found("a host other than Docker Hub"), 0);
    assert_eq!(found("nginxHub and hubble and spoken"), 0);
    // a code span and a code block of the book are examples
    let page = "Run `observer --clusters tag:role=hub`.\n\n```text\nhub\n```\n";
    assert_eq!(found(&without_code(&outside_code_blocks(page))), 0);
}
