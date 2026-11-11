# The image of the observer: the signed strata binary of a release, and the observer of the same commit.
FROM --platform=$BUILDPLATFORM golang:1.27.3@sha256:e519d3b0a5b285e5aa53ab9eef02bc6bbb9cb04608389b743a136a9ce75f0d1e AS build
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
