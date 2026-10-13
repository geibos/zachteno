# Развёртывание через Docker Compose

Стенд из пяти сервисов: `app` (бэкенд со встроенным фронтендом), `postgres` (PostgreSQL 17 с WAL-G), `garage` (картинки), `backup-garage` (хранилище бэкапов на отдельном диске) и разовый `backup` (WAL-G, rclone). Наружу публикуется только `app`; TLS и домен — у вашего обратного прокси (Caddy, nginx, Traefik и т. п.).

```
<ZACHTENO_HOME>/
├── docker-compose.yml            ← копия src/deploy/compose/docker-compose.yml
├── docker-compose.override.yml   ← (необязательно) особенности вашего стенда: сети и метки прокси
├── .env                          ← секреты и настройки (в git не кладите)
├── data/                         ← postgres, garage-meta/-data, backup-garage-meta, asn
└── src/                          ← git clone этого репозитория
```

## Установка

1. `git clone https://github.com/geibos/zachteno.git src && cp src/deploy/compose/docker-compose.yml .`
2. `.env` по образцу `src/.env.example` плюс переменные compose:
   - адрес: `PUBLIC_URL` (например, `https://tests.example.edu`), `HTTP_BIND`/`HTTP_PORT` (куда опубликовать порт приложения, по умолчанию `127.0.0.1:8080`), `TRUSTED_PROXIES` (подсеть, откуда приходит прокси — нужна для `X-Forwarded-For`);
   - база: `POSTGRES_PASSWORD`, `ZACHTENO_OWNER_PASSWORD`, `ZACHTENO_APP_PASSWORD`;
   - хранилища: `GARAGE_RPC_SECRET`, `GARAGE_ADMIN_TOKEN`, `S3_ACCESS_KEY_ID`, `S3_SECRET_ACCESS_KEY`, `BACKUP_GARAGE_RPC_SECRET`, `BACKUP_GARAGE_ADMIN_TOKEN`, `BACKUP_S3_KEY_ID`, `BACKUP_S3_SECRET` (секреты — `openssl rand -hex 32`, ключи Garage — `GK` + 24 hex и 64 hex);
   - бэкапы: `BACKUP_DATA_DIR` (каталог на **другом физическом диске**), `BACKUP_MOUNTPOINT` (необязательно: точка монтирования этого диска), `BACKUP_PUSH_URL`, `RESTORE_PUSH_URL` (необязательно: push-мониторинг), `PUSH_NETWORK` (необязательно: docker-сеть, из которой виден монитор);
   - приложение: `PASSWORD_PEPPERS`, `SIGNAL_HMAC_KEY`, `MAIL_FROM`, `MAIL_MODE=smtp` и `SMTP_*`, при желании `YANDEX_CLIENT_ID/SECRET`.
3. Каталоги и метка диска бэкапов: `mkdir -p data/asn "$BACKUP_DATA_DIR/garage-data" && touch "$BACKUP_DATA_DIR/.zachteno-backup-volume"`.
4. `docker compose build && docker compose up -d postgres garage backup-garage`, затем инициализация хранилищ:
   - `src/deploy/garage/init.sh docker-compose.yml garage "$S3_ACCESS_KEY_ID" "$S3_SECRET_ACCESS_KEY" attachments`
   - `src/deploy/garage/init.sh docker-compose.yml backup-garage "$BACKUP_S3_KEY_ID" "$BACKUP_S3_SECRET" pg-backups attachments-mirror attachments-history`
5. `docker compose up -d app` и первый суперпользователь: `docker compose run --rm app create-superuser --email you@example.com` (команда выведет ссылку для пароля).
6. Демо-курс ПДД (необязательно): `git clone --depth 1 https://github.com/etspring/pdd_russia.git data/pdd` и `docker compose run --rm -v "$PWD/data/pdd:/pdd:ro" app seed-pdd --dir /pdd --owner you@example.com`.
7. Бэкапы по таймерам systemd (пользовательские): `cp src/deploy/compose/systemd/* ~/.config/systemd/user/`, при другом `ZACHTENO_HOME` поправьте пути в копиях, затем `systemctl --user daemon-reload && systemctl --user enable --now zachteno-backup.timer zachteno-restore-check.timer zachteno-asn.timer`.

## Пример override для Traefik

```yaml
services:
  app:
    ports: !reset []
    networks: [proxy, internal]
    labels:
      - traefik.enable=true
      - traefik.docker.network=proxy
      - traefik.http.routers.zachteno.rule=Host(`tests.example.edu`)
      - traefik.http.routers.zachteno.tls=true
      - traefik.http.services.zachteno.loadbalancer.server.port=8080
networks:
  proxy:
    external: true
```

Восстановление из бэкапа — [`docs/ops/restore.md`](../../docs/ops/restore.md).
