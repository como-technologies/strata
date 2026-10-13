//! `strata preflight`: the run of the `strata-preflight` binary.

use std::path::PathBuf;
use std::process::ExitCode;

use strata::error::{Error, Result};

use super::args::PreflightArgs;

/// The binary that `strata preflight` runs. It is a package of its own, `strata-preflight`, so the
/// strata crate stays free of its dependencies; a release ships both binaries side by side.
const PREFLIGHT_BIN: &str = "strata-preflight";

/// `strata preflight ...` runs the `strata-preflight` binary with the same arguments: the one
/// beside this binary first, then the one on the PATH. Its exit status is the exit status.
pub(crate) fn cmd_preflight(a: &PreflightArgs) -> Result<ExitCode> {
    let beside = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(PREFLIGHT_BIN)))
        .filter(|p| p.is_file());
    let program = beside.unwrap_or_else(|| PathBuf::from(PREFLIGHT_BIN));
    let status = std::process::Command::new(&program)
        .args(&a.args)
        .status()
        .map_err(|e| Error::Usage {
            message: format!(
                "cannot run {PREFLIGHT_BIN} ({e}): install it beside strata, or on the PATH; \
                 the installer of a release puts both in one directory"
            ),
        })?;
    Ok(match status.code() {
        Some(code) => ExitCode::from(u8::try_from(code).unwrap_or(2)),
        None => ExitCode::from(2),
    })
}
