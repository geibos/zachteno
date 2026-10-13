#!/usr/bin/env bash
# Общие функции бэкапа «Зачтено». Запускается на хосте из ~/zachteno.
set -euo pipefail
cd "$(dirname "$0")/../../../.."   # ~/zachteno
set -a; . ./.env; set +a

SENTINEL=/mnt/backup/backups/zachteno/.zachteno-backup-volume
compose() { docker compose -f docker-compose.yml "$@"; }

# Без смонтированного внешнего диска и метки ничего не пишем: иначе бэкап тихо уйдёт на SSD.
require_backup_disk() {
  mountpoint -q /mnt/backup || { echo "внешний диск /mnt/backup не смонтирован" >&2; return 1; }
  [ -f "$SENTINEL" ] || { echo "нет метки $SENTINEL" >&2; return 1; }
}

# Свободное место: не меньше 5% на / и на /mnt/backup.
require_free_space() {
  for mount in / /mnt/backup; do
    used=$(df --output=pcent "$mount" | tail -1 | tr -dc '0-9')
    [ "$used" -le 95 ] || { echo "на $mount занято ${used}%" >&2; return 1; }
  done
}

# Отметка в ops_status (её читают метрики приложения).
mark() { # mark <name> success|failure <details>
  local column=last_success_at; [ "$2" = failure ] && column=last_failure_at
  compose exec -T postgres psql -U postgres -d zachteno -v ON_ERROR_STOP=1 -q -c \
    "INSERT INTO ops_status (name, $column, details) VALUES ('$1', now(), left('$3', 500))
     ON CONFLICT (name) DO UPDATE SET $column = now(), details = EXCLUDED.details" >/dev/null
}

# Push в Uptime Kuma: отсутствие push — тревога в Telegram.
push() { # push <url-variable-name> <message>
  local url="${!1:-}"; [ -n "$url" ] || return 0
  curl -fsS -m 10 -G "$url" --data-urlencode "status=up" --data-urlencode "msg=$2" >/dev/null || true
}
