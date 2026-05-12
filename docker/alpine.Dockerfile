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

FROM gcr.io/distroless/cc-debian12 AS cc

FROM alpine:3.20 AS patcher

COPY --from=bin /rover /rover
COPY --from=tini /tini /tini
RUN apk add --no-cache patchelf \
  && patchelf --set-rpath /usr/local/lib/glibc /rover \
  && patchelf --set-rpath /usr/local/lib/glibc /tini

FROM alpine:3.20

COPY --from=cc --chown=root:root --chmod=755 /lib/*-linux-gnu/* /usr/local/lib/glibc/
COPY --from=cc --chown=root:root --chmod=755 /lib/ld-linux-* /lib/

RUN apk add --no-cache ca-certificates \
  && addgroup --gid 1993 rover \
  && adduser --uid 1993 --disabled-password rover --ingroup rover \
  && mkdir /rover-home \
  && chown rover:rover /rover-home \
  && mkdir -p /lib64 \
  && ln -sf /usr/local/lib/glibc/ld-linux-* /lib64/

ENV ROVER_HOME=/rover-home
ENV PATH=/usr/local/bin:${PATH}

ARG ROVER_VERSION
ENV ROVER_VERSION=${ROVER_VERSION}

LABEL org.opencontainers.image.title="Rover" \
      org.opencontainers.image.description="Rover Docker image (Alpine)" \
      org.opencontainers.image.url="https://rover.lu" \
      org.opencontainers.image.source="https://github.com/thalesgelinger/rover" \
      org.opencontainers.image.licenses="MIT" \
      org.opencontainers.image.version="${ROVER_VERSION}"

COPY --from=patcher /rover /usr/local/bin/rover
COPY --from=patcher /tini /tini
COPY docker/entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod 755 /usr/local/bin/docker-entrypoint.sh

WORKDIR /app
ENTRYPOINT ["/tini", "--", "docker-entrypoint.sh"]
CMD ["--help"]
