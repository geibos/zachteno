# Образ приложения: статика фронтенда встраивается в бинарь (design D2).
FROM node:24-bookworm-slim AS web
WORKDIR /src/frontend
COPY frontend/package.json frontend/package-lock.json frontend/.npmrc ./
RUN npm ci
COPY frontend ./
RUN npm run build

FROM rust:1.99-bookworm AS server
WORKDIR /src
COPY backend backend
COPY --from=web /src/frontend/build frontend/build
RUN cd backend && cargo build --release -p app

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* && useradd --uid 10001 --system zachteno
COPY --from=server /src/backend/target/release/app /usr/local/bin/zachteno
USER zachteno
EXPOSE 8080 9100
HEALTHCHECK --interval=15s --timeout=5s --start-period=20s CMD ["zachteno", "healthcheck"]
ENTRYPOINT ["zachteno"]
CMD ["serve"]
