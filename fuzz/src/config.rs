//! The target of the settings of a connection to Palette.
//!
//! A person gives the settings in the environment and in `~/.palette.env`, as lines of
//! `NAME=value`. `Config::from_lookup` is public, and it reads them through a lookup. The target
//! builds the lookup from the lines of its input. The reader of the file `~/.palette.env` is not
//! public. So the target reaches it only as a lookup. The lines of the input are the lines of the
//! file.

use std::collections::BTreeMap;

use strata::config::{Config, KEY_VARS, PROJECT_VARS};

use crate::support::{answer, utf8};

/// The settings of `text`: the lines `NAME=value`. A line with no `=` gives no setting, and a
/// later line replaces an earlier line of the same name.
fn settings(text: &str) -> BTreeMap<&str, &str> {
    text.lines()
        .filter_map(|line| line.split_once('='))
        .collect()
}

/// Make sure that `config`, which the lookup of `settings` gave, holds what the settings say and
/// leaks no key.
#[track_caller]
fn check_config(config: &Config, settings: &BTreeMap<&str, &str>) {
    let key = KEY_VARS.iter().find_map(|name| settings.get(name));
    assert_eq!(
        Some(config.api_key.as_str()),
        key.copied(),
        "the key is the first of the variables of the key"
    );
    assert!(
        !config.base_url.ends_with('/'),
        "the address {:?} ends with a slash",
        config.base_url
    );
    assert!(
        format!("{config:?}").contains("api_key: \"<redacted>\""),
        "the debug form of a config shows the key"
    );
    let uid = PROJECT_VARS.iter().find_map(|name| settings.get(name));
    assert_eq!(
        config.project_uid.as_deref(),
        uid.copied().filter(|uid| !uid.is_empty()),
        "the project uid is the first uid that is not empty"
    );
    assert!(
        config.project_name.is_none() || config.project_uid.is_none(),
        "a name and a uid name the project: the uid decides"
    );
    assert!(
        config.project_name.as_deref() != Some(""),
        "an empty name is no name"
    );
}

/// Read the text of `bytes` as lines of settings. Return true when the settings give a connection.
pub(crate) fn lookup(bytes: &[u8]) -> bool {
    let Some(settings) = utf8(bytes).map(settings) else {
        return false;
    };
    let Some(config) = answer(Config::from_lookup(|name| {
        settings.get(name).map(|value| value.to_string())
    })) else {
        return false;
    };
    check_config(&config, &settings);
    true
}

#[test]
fn a_lookup_of_settings_gives_a_config_that_hides_its_key_or_is_refused_with_a_kind() {
    bolero::check!().for_each(|bytes| {
        lookup(bytes);
    });
}
