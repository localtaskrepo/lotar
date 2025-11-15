# syntax=docker/dockerfile:1.7

FROM alpine:3.21 AS downloader
ARG LOTAR_VERSION
ARG TARGETARCH
ENV LOTAR_VERSION=${LOTAR_VERSION}
ENV TARGETARCH=${TARGETARCH}

RUN set -euxo pipefail \
    && apk add --no-cache ca-certificates curl tar \
    && update-ca-certificates \
    && case "$TARGETARCH" in \
        amd64) ARCH_SUFFIX="x64" ;; \
        arm64) ARCH_SUFFIX="arm64" ;; \
        *) echo "Unsupported TARGETARCH: $TARGETARCH" >&2; exit 1 ;; \
    esac \
    && download_base="https://github.com/localtaskrepo/lotar/releases/download/v${LOTAR_VERSION}" \
    && tarball="lotar-v${LOTAR_VERSION}-linux-musl-${ARCH_SUFFIX}.tar.gz" \
    && checksum_file="lotar-v${LOTAR_VERSION}-linux-musl-${ARCH_SUFFIX}.sha256" \
    && curl -sSfL "${download_base}/${tarball}" -o /tmp/lotar.tar.gz \
    && curl -sSfL "${download_base}/${checksum_file}" -o /tmp/lotar.sha256 \
    && hash=$(cut -d ' ' -f1 /tmp/lotar.sha256) \
    && echo "${hash}  /tmp/lotar.tar.gz" | sha256sum -c - \
    && tar -xzf /tmp/lotar.tar.gz -C /usr/local/bin \
    && chmod +x /usr/local/bin/lotar \
    && rm -f /tmp/lotar.tar.gz /tmp/lotar.sha256

FROM scratch AS runtime
LABEL org.opencontainers.image.source="https://github.com/localtaskrepo/lotar" \
      org.opencontainers.image.description="LoTaR CLI packaged in a minimal image" \
      org.opencontainers.image.licenses="MIT"

COPY --from=downloader /usr/local/bin/lotar /usr/local/bin/lotar
COPY --from=downloader /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt

ENV LOTAR_TASKS_DIR=/tasks

USER 1000:1000
WORKDIR /workspace
VOLUME ["/workspace", "/tasks"]
ENTRYPOINT ["/usr/local/bin/lotar"]
CMD ["--help"]
