ARG ROVER_VERSION=v0.0.1-alpha.1
ARG ROVER_REPO=thalesgelinger/rover

FROM buildpack-deps:bookworm-curl AS download

ARG ROVER_VERSION
ARG ROVER_REPO
ARG TARGETARCH

RUN set -eu; \
  case "$TARGETARCH" in \
    amd64) rover_arch="x86_64" ;; \
    arm64) rover_arch="aarch64" ;; \
    *) echo "unsupported arch: $TARGETARCH" >&2; exit 1 ;; \
  esac; \
  asset="rover-${ROVER_VERSION}-${rover_arch}-unknown-linux-gnu"; \
  archive="${asset}.tar.gz"; \
  base_url="https://github.com/${ROVER_REPO}/releases/download/${ROVER_VERSION}"; \
  curl -fsSL "${base_url}/${archive}" -o "/tmp/${archive}"; \
  curl -fsSL "${base_url}/SHA256SUMS" -o /tmp/SHA256SUMS; \
  cd /tmp; \
  grep " ${archive}$" SHA256SUMS | sha256sum -c -; \
  tar -xzf "${archive}"; \
  cp "${asset}/rover" /rover; \
  chmod 755 /rover

FROM scratch

ARG ROVER_VERSION
ENV ROVER_VERSION=${ROVER_VERSION}

LABEL org.opencontainers.image.title="Rover" \
      org.opencontainers.image.description="Rover binary image" \
      org.opencontainers.image.url="https://rover.lu" \
      org.opencontainers.image.source="https://github.com/thalesgelinger/rover" \
      org.opencontainers.image.licenses="MIT" \
      org.opencontainers.image.version="${ROVER_VERSION}"

COPY --from=download /rover /rover
