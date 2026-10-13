# PostgreSQL 17 с WAL-G для непрерывной архивации WAL (design D16).
FROM postgres:17-bookworm
ARG WALG_VERSION=v3.0.9
ADD https://github.com/wal-g/wal-g/releases/download/${WALG_VERSION}/wal-g-pg-22.04-amd64.tar.gz /tmp/wal-g.tar.gz
ADD https://github.com/wal-g/wal-g/releases/download/${WALG_VERSION}/wal-g-pg-22.04-amd64.tar.gz.sha256 /tmp/wal-g.tar.gz.sha256
RUN cd /tmp && echo "$(cat wal-g.tar.gz.sha256 | cut -d' ' -f1)  wal-g.tar.gz" | sha256sum -c - \
    && tar -xzf wal-g.tar.gz && install -m 0755 wal-g-pg-22.04-amd64 /usr/local/bin/wal-g && rm -f /tmp/wal-g*
