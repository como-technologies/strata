# The image of the observer: the signed strata binary of a release, and the observer of the same commit.
FROM --platform=$BUILDPLATFORM golang:1.27.3@sha256:2920bb124f1a2e12a1d39f0cab756ae0068441ee1da7655dec7f5e374944095b AS build
WORKDIR /src
COPY observer/ .
ARG VERSION
ARG TARGETARCH
RUN CGO_ENABLED=0 GOOS=linux GOARCH=$TARGETARCH go build -trimpath -ldflags "-X main.version=$VERSION" -o /strata-observer .

FROM gcr.io/distroless/static-debian12:nonroot@sha256:afa5c872c891853ca7fcf1f12c3edb23f7eeef36189728842dd51042ff57f7ab
ARG TARGETARCH
COPY dist/strata-$TARGETARCH /usr/local/bin/strata
COPY --from=build /strata-observer /usr/local/bin/strata-observer
ENTRYPOINT ["/usr/local/bin/strata-observer"]
