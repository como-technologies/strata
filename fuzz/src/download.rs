//! The target of the read of the download of a profile version: the gzip, the tar, the two lists,
//! and the archive of each pack.
//!
//! The reader is not public. The target reaches it as a program does: through the download of a
//! profile, which a Palette answers with the input as the file.

use strata::api::{MemoryApi, Palette, ProfileDownload};

use crate::routes::download_route;
use crate::support::{UID, answer};

/// The length of a digest of a file of an archive: `sha256:` and 64 hex digits.
const DIGEST_LENGTH: usize = 71;

/// The download that a Palette gives when it answers the download of the profile with `bytes`.
fn read(bytes: &[u8]) -> Option<ProfileDownload> {
    let api = MemoryApi::new().with_file(download_route(UID), bytes.to_vec());
    answer(Palette::new(&api).profile_download(UID))
}

/// Read `bytes` as the download of a profile version. A file that strata reads gives the same
/// download each time. Each file of each archive has a digest, and an archive differs from itself
/// in no file. True when strata read the file.
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
        let Some(archive) = &pack.archive else {
            continue;
        };
        assert!(archive.changed_files(archive).is_empty(), "{name}");
        for (path, digest) in archive.files() {
            assert!(
                digest.starts_with("sha256:") && digest.len() == DIGEST_LENGTH,
                "{name} {path}: {digest}"
            );
        }
    }
    true
}

#[test]
fn a_download_of_a_profile_gives_the_archive_of_each_pack_or_is_refused_with_a_kind() {
    bolero::check!().for_each(|bytes| {
        file(bytes);
    });
}
