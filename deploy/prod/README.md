# Развёртывание на домашнем сервере

Подпроект `~/zachteno/` по правилам `AGENTS.md сервера`: сеть `proxy`, роутер Traefik на `zachteno.example.org` с `secure-headers@file`, база и хранилища только во внутренних сетях.

```
~/zachteno/
├── docker-compose.yml  ← копия src/deploy/prod/docker-compose.yml
├── .env                ← секреты (в git не попадает)
├── data/               ← postgres, garage-meta/-data, backup-garage-meta, asn
└── src/                ← git clone github.com/geibos/zachteno
```

1. `git clone https://github.com/geibos/zachteno.git src`, `cp src/deploy/prod/docker-compose.yml .`
2. `.env` по `src/.env.example` плюс: `POSTGRES_PASSWORD`, `ZACHTENO_OWNER_PASSWORD`, `ZACHTENO_APP_PASSWORD`, `GARAGE_RPC_SECRET`, `GARAGE_ADMIN_TOKEN`, `BACKUP_GARAGE_RPC_SECRET`, `BACKUP_GARAGE_ADMIN_TOKEN`, `S3_ACCESS_KEY_ID/SECRET` (основной Garage), `BACKUP_S3_KEY_ID/SECRET` (Garage бэкапов), `PASSWORD_PEPPERS`, `SIGNAL_HMAC_KEY`, `MAIL_MODE=smtp` и `SMTP_*` (Postbox), `YANDEX_CLIENT_ID/SECRET`, `UPTIME_PUSH_BACKUP`, `UPTIME_PUSH_RESTORE`.
3. `mkdir -p data/asn /mnt/backup/backups/zachteno/garage-data && touch /mnt/backup/backups/zachteno/.zachteno-backup-volume`
4. `docker compose build && docker compose up -d postgres garage backup-garage`, затем `src/deploy/garage/init.sh docker-compose.yml garage "$S3_ACCESS_KEY_ID" "$S3_SECRET_ACCESS_KEY" attachments` и то же для `backup-garage` с бакетом `pg-backups` и `attachments-mirror`, `attachments-history`.
5. `docker compose up -d app`, `docker compose run --rm app create-superuser --email …`.
6. Таймеры: `cp src/deploy/prod/systemd/* ~/.config/systemd/user/ && systemctl --user daemon-reload && systemctl --user enable --now zachteno-backup.timer zachteno-restore-check.timer zachteno-asn.timer`.
