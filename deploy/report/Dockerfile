# The image of the endpoint of the report: the package strata-report-endpoint, built in the first
# stage and run in the second. Cloud Build builds it from the root of the repository, because the
# package depends on strata by path: see deploy/report/cloudbuild.yaml.
#
# The binary reads the token of GitHub from GITHUB_TOKEN, which Cloud Run fills from a secret when
# the service starts. The image holds no secret.
FROM rust:1.99.1-slim-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p strata-report-endpoint

# distroless/cc: the glibc of the build stage, no shell, and a user that is not root. The binary
# carries its root certificates, so it needs no package of certificates.
FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /src/target/release/strata-report-endpoint /strata-report-endpoint
ENV PORT=8080
EXPOSE 8080
ENTRYPOINT ["/strata-report-endpoint"]
