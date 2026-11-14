//! The target of the read of the download of a profile version: the gzip, the tar, the two lists,
//! and the archive of each pack.
//!
//! The reader is not public. The target reaches it as a program does: through the download of a
//! profile, which a Palette answers with the input as the file.

use strata::api::{MemoryApi, PackArchive, PackFile, Palette, ProfileDownload};

use crate::routes::download_route;
use crate::support::{UID, answer};

/// The length of a digest of a file of an archive: `sha256:` and 64 hex digits.
const DIGEST_LENGTH: usize = 71;

/// The most characters of the reason of a pack file that is no archive. The reason holds the size
/// of the file, a few words, and 120 characters at most of the file or of the error.
const REASON_LENGTH: usize = 160;

/// The start of the reason of a pack file that the list names and the download does not hold, as
/// [`PackFile::NotReadable`] states it. The path of the file follows it.
const NOT_HELD: &str = "the download does not hold ";

/// The download that a Palette gives when it answers the download of the profile with `bytes`.
fn read(bytes: &[u8]) -> Option<ProfileDownload> {
    let api = MemoryApi::new().with_file(download_route(UID), bytes.to_vec());
    answer(Palette::new(&api).profile_download(UID))
}

/// Read `bytes` as the download of a profile version. A file that strata reads gives the same
/// download each time. Each file of each archive has a digest, and an archive differs from itself
/// in no file. The reason of a pack file starts with the size of the file, or names a file that the
/// download does not hold. It is short, and holds no control character. True when strata read the
/// file.
pub(crate) fn file(bytes: &[u8]) -> bool {
    let Some(download) = read(bytes) else {
        return false;
    };
    assert_eq!(
        read(bytes).as_ref(),
        Some(&download),
        "one file gives two downloads"
    );
    for (name, pack) in download.packs() {
        match &pack.file {
            PackFile::Archive(archive) => archive_digests(name, archive),
            PackFile::NotReadable(reason) => reason_form(name, reason),
            PackFile::NoArchive => {}
        }
    }
    true
}

/// Each file of `archive`, of the pack `name`, has a digest, and the archive differs from itself in
/// no file.
fn archive_digests(name: &str, archive: &PackArchive) {
    assert!(archive.changed_files(archive).is_empty(), "{name}");
    for (path, digest) in archive.files() {
        assert!(
            digest.starts_with("sha256:") && digest.len() == DIGEST_LENGTH,
            "{name} {path}: {digest}"
        );
    }
}

/// The reason of the pack `name` starts with the size of its file, or with [`NOT_HELD`]. It holds
/// [`REASON_LENGTH`] characters at most, and holds no control character.
fn reason_form(name: &str, reason: &str) {
    let size = reason.split_once(' ').map_or("", |(size, _)| size);
    assert!(
        size.parse::<usize>().is_ok() || reason.starts_with(NOT_HELD),
        "{name}: {reason:?}"
    );
    assert!(
        reason.chars().count() <= REASON_LENGTH,
        "{name}: {reason:?}"
    );
    assert!(!reason.chars().any(char::is_control), "{name}: {reason:?}");
}

#[test]
fn a_download_of_a_profile_gives_the_archive_of_each_pack_or_is_refused_with_a_kind() {
    bolero::check!().for_each(|bytes| {
        file(bytes);
    });
}
