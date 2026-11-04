# The session image of strata: each riff session that does a work item or a verify runs in it.
# It is riff-base, which has claude, git, gh, curl, and the CA certificates. It adds Rust, Go, and
# the tools of the CI recipes in CLAUDE.md: just, mdbook with its preprocessors, mdbook-lint, and
# jq. It also adds sccache, the build cache that riff.toml sets. The image holds no code of strata
# and no secret.
#
# The workflow "Session image" builds the image on each change of this folder. riff.toml pins its
# digest: a change of this file counts only after a change of that pin.
FROM ghcr.io/como-technologies/riff-base@sha256:2799cd2e7c6e23ab4b9b576ad1331e7315044378a490317624c93d3d984ad5b1

# The CI builds with the latest stable Rust. RUST_VERSION is the latest stable Rust at the build of
# the image, so that clippy finds the same lints as the CI. RUST_MSRV is the rust-version of
# Cargo.toml, for the check "the lowest Rust version".
ARG RUST_VERSION=1.99.1
ARG RUST_MSRV=1.88
ARG JUST_VERSION=1.58.0
ARG MDBOOK_VERSION=0.5.4
ARG MDBOOK_MERMAID_VERSION=0.17.1
ARG MDBOOK_GRUVBOX_VERSION=1.2.0
ARG MDBOOK_LINT_VERSION=0.16.2
ARG SCCACHE_VERSION=0.18.0
ARG JQ_VERSION=1.7.1
ARG JQ_SHA256=5942c9b0934e510ee61eb3e30273f1b3fe2590df93933a93d7c58b81d19c8ff5

# The observer in observer/ is in Go. GO_VERSION is the latest stable Go at the build of the image.
ARG GO_VERSION=1.27.3
ARG GO_SHA256=65efbc80fdc1fa2cd2631cad5b481777a68e8725f34e89f50fc6ac714e379b40

# gcc and libc6-dev: the linker of rustc, and the C code of a crate. make and pkg-config: the
# build of a crate with C code.
RUN apt-get update \
 && apt-get install -y --no-install-recommends gcc libc6-dev make pkg-config \
 && rm -rf /var/lib/apt/lists/*

# jq: the scripts read each answer of Palette and each summary of strata with it, and the tests
# run the scripts. The jq of Debian is 1.6. The CI has 1.7.1. From jq 1.7, `jq -e` exits with 4 on
# an empty file, which the tests of the CI scripts need.
RUN curl -fsSL -o /usr/local/bin/jq \
      "https://github.com/jqlang/jq/releases/download/jq-${JQ_VERSION}/jq-linux-amd64" \
 && echo "${JQ_SHA256}  /usr/local/bin/jq" | sha256sum -c - \
 && chmod 0755 /usr/local/bin/jq

# The toolchains are in the image, and a session cannot write them. riff.toml gives each session
# its own CARGO_HOME in a volume.
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH
RUN curl -fsSL https://sh.rustup.rs \
    | sh -s -- -y --no-modify-path --profile minimal \
        --default-toolchain "$RUST_VERSION" \
        --component rustfmt --component clippy \
 && rustup toolchain install --profile minimal "$RUST_MSRV" \
 && chmod -R a+rX "$RUSTUP_HOME" "$CARGO_HOME"

# The tools that have a static release.
RUN set -eu; \
    get() { \
      mkdir -p /tmp/get && curl -fsSL "$2" | tar -xz -C /tmp/get \
      && install -m 0755 "$(find /tmp/get -type f -name "$1" | head -n 1)" /usr/local/bin/ \
      && rm -rf /tmp/get; \
    }; \
    get just "https://github.com/casey/just/releases/download/${JUST_VERSION}/just-${JUST_VERSION}-x86_64-unknown-linux-musl.tar.gz"; \
    get mdbook "https://github.com/rust-lang/mdBook/releases/download/v${MDBOOK_VERSION}/mdbook-v${MDBOOK_VERSION}-x86_64-unknown-linux-musl.tar.gz"; \
    get mdbook-mermaid "https://github.com/badboy/mdbook-mermaid/releases/download/v${MDBOOK_MERMAID_VERSION}/mdbook-mermaid-v${MDBOOK_MERMAID_VERSION}-x86_64-unknown-linux-musl.tar.gz"; \
    get sccache "https://github.com/mozilla/sccache/releases/download/v${SCCACHE_VERSION}/sccache-v${SCCACHE_VERSION}-x86_64-unknown-linux-musl.tar.gz"

# The tools that have no static release come from crates.io. The registry of the build does not
# stay in the image.
RUN cargo install --locked --root /usr/local \
      "mdbook-gruvbox@${MDBOOK_GRUVBOX_VERSION}" "mdbook-lint@${MDBOOK_LINT_VERSION}" \
 && rm -rf "$CARGO_HOME/registry" "$CARGO_HOME/git"

# The Go of the observer. GOTOOLCHAIN=local stops a download of another Go. GOPROXY=off stops a
# download of a module, because the observer uses the standard library only.
ENV PATH=/usr/local/go/bin:$PATH GOTOOLCHAIN=local GOPROXY=off
RUN curl -fsSL -o /tmp/go.tgz "https://go.dev/dl/go${GO_VERSION}.linux-amd64.tar.gz" \
 && echo "${GO_SHA256}  /tmp/go.tgz" | sha256sum -c - \
 && tar -C /usr/local -xzf /tmp/go.tgz \
 && rm /tmp/go.tgz \
 && go version

CMD ["bash"]
