ARG ROVER_VERSION=v0.0.1-alpha.1
ARG BIN_IMAGE=ghcr.io/thalesgelinger/rover:bin-${ROVER_VERSION}

FROM ${BIN_IMAGE} AS bin

FROM buildpack-deps:bookworm-curl AS tini

ARG TINI_VERSION=0.19.0
ARG TARGETARCH

RUN curl -fsSL "https://github.com/krallin/tini/releases/download/v${TINI_VERSION}/tini-${TARGETARCH}" -o "/tini-${TARGETARCH}" \
  && curl -fsSL "https://github.com/krallin/tini/releases/download/v${TINI_VERSION}/tini-${TARGETARCH}.sha256sum" -o "/tini-${TARGETARCH}.sha256sum" \
  && cd / \
  && sha256sum -c "tini-${TARGETARCH}.sha256sum" \
  && mv "/tini-${TARGETARCH}" /tini \
  && rm "/tini-${TARGETARCH}.sha256sum" \
  && chmod 755 /tini

FROM debian:bookworm-slim

RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates curl \
  && rm -rf /var/lib/apt/lists/* \
  && useradd --uid 1993 --user-group rover \
  && mkdir /rover-home \
  && chown rover:rover /rover-home

ENV ROVER_HOME=/rover-home
ENV PATH=/usr/local/bin:${PATH}

ARG ROVER_VERSION
ENV ROVER_VERSION=${ROVER_VERSION}

LABEL org.opencontainers.image.title="Rover" \
      org.opencontainers.image.description="Rover Docker image (Debian)" \
      org.opencontainers.image.url="https://rover.lu" \
      org.opencontainers.image.source="https://github.com/thalesgelinger/rover" \
      org.opencontainers.image.licenses="MIT" \
      org.opencontainers.image.version="${ROVER_VERSION}"

COPY --from=bin /rover /usr/local/bin/rover
COPY --from=tini /tini /tini
COPY docker/entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod 755 /usr/local/bin/docker-entrypoint.sh

WORKDIR /app
ENTRYPOINT ["/tini", "--", "docker-entrypoint.sh"]
CMD ["--help"]
