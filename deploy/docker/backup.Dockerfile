# Разовые задачи бэкапа: WAL-G (через образ Postgres), rclone, psql.
FROM zachteno-postgres:latest
ARG RCLONE_VERSION=v1.75.2
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl unzip \
    && curl -fsSL -o /tmp/rclone.zip https://downloads.rclone.org/${RCLONE_VERSION}/rclone-${RCLONE_VERSION}-linux-amd64.zip \
    && curl -fsSL -o /tmp/rclone.sums https://downloads.rclone.org/${RCLONE_VERSION}/SHA256SUMS \
    && echo "$(grep " rclone-${RCLONE_VERSION}-linux-amd64.zip$" /tmp/rclone.sums | cut -d' ' -f1)  /tmp/rclone.zip" | sha256sum -c - \
    && unzip -q /tmp/rclone.zip -d /tmp && install -m 0755 /tmp/rclone-*-linux-amd64/rclone /usr/local/bin/rclone \
    && rm -rf /tmp/rclone* /var/lib/apt/lists/*
