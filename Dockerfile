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

# Alpine rather than scratch: history, stats, changelog, scans and agent
# worktrees run the git executable, and automation `run:` actions need /bin/sh
# (DEV-289). The base image already ships the CA bundle.
FROM alpine:3.21 AS runtime
LABEL org.opencontainers.image.source="https://github.com/localtaskrepo/lotar" \
      org.opencontainers.image.description="LoTaR CLI packaged in a minimal image" \
      org.opencontainers.image.licenses="MIT"

# A bind-mounted repository belongs to the host user, not to the container's
# UID, so git would refuse it as a "dubious ownership" repository. The home
# directory is writable for any UID, so `docker run --user "$(id -u):$(id -g)"`
# works without a matching /etc/passwd entry.
RUN set -eux \
    && apk add --no-cache git \
    && git config --system --add safe.directory '*' \
    && mkdir -p /home/lotar /tasks \
    && chmod 1777 /home/lotar

COPY --from=downloader /usr/local/bin/lotar /usr/local/bin/lotar
# Picks /tasks only when it is mounted and the repository has no .tasks, so
# git-backed commands see tasks inside the repository. /tasks is therefore not
# a VOLUME: an anonymous volume would always count as mounted.
COPY --chmod=0755 packaging/docker-entrypoint.sh /usr/local/bin/docker-entrypoint

ENV HOME=/home/lotar

USER 1000:1000
WORKDIR /workspace
VOLUME ["/workspace"]
ENTRYPOINT ["/usr/local/bin/docker-entrypoint"]
CMD ["--help"]
