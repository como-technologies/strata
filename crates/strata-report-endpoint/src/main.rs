//! The service of the endpoint of the report. Cloud Run starts it: see `deploy/report/`.

use std::process::ExitCode;
use std::sync::Arc;

use strata_report_endpoint::server::{MOST_AT_ONCE, Service, serve};
use strata_report_endpoint::settings::Settings;
use strata_report_endpoint::tracker::GitHub;

fn main() -> ExitCode {
    let settings = match Settings::from_lookup(|name| std::env::var(name).ok()) {
        Ok(settings) => settings,
        Err(message) => {
            eprintln!("strata-report-endpoint: {message}");
            return ExitCode::from(2);
        }
    };
    let tracker = GitHub::new(
        &settings.api,
        &settings.repository,
        &settings.token,
        &settings.label,
        settings.milestone.as_deref(),
    );
    let server = match tiny_http::Server::http(("0.0.0.0", settings.port)) {
        Ok(server) => Arc::new(server),
        Err(e) => {
            eprintln!(
                "strata-report-endpoint: cannot listen on port {}: {e}",
                settings.port
            );
            return ExitCode::from(2);
        }
    };
    eprintln!(
        "strata-report-endpoint: listening on port {}, for {}",
        settings.port, settings.repository
    );
    serve(
        &server,
        &Arc::new(Service::new(tracker)),
        settings.trust_forwarded,
        MOST_AT_ONCE,
    );
    ExitCode::SUCCESS
}
