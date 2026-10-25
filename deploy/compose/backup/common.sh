#!/usr/bin/env bash
# Общие функции бэкапа «Зачтено».
# ZACHTENO_HOME — каталог с docker-compose.yml и .env (по умолчанию — три уровня вверх от скрипта:
# <home>/src/deploy/compose/backup → <home>).
set -euo pipefail
ZACHTENO_HOME=${ZACHTENO_HOME:-$(cd "$(dirname "$0")/../../../.." && pwd)}
cd "$ZACHTENO_HOME"
set -a; . ./.env; set +a
: "${BACKUP_DATA_DIR:?задайте BACKUP_DATA_DIR в .env}"

SENTINEL="$BACKUP_DATA_DIR/.zachteno-backup-volume"
compose() { docker compose "$@"; }

# Без метки (и смонтированного диска, если задан BACKUP_MOUNTPOINT) ничего не пишем:
# иначе при отвалившемся диске бэкап тихо уйдёт на основной.
require_backup_disk() {
  if [ -n "${BACKUP_MOUNTPOINT:-}" ]; then
    mountpoint -q "$BACKUP_MOUNTPOINT" || { echo "диск $BACKUP_MOUNTPOINT не смонтирован" >&2; return 1; }
  fi
  [ -f "$SENTINEL" ] || { echo "нет метки $SENTINEL" >&2; return 1; }
}

# Свободное место: не меньше BACKUP_MIN_FREE_GB (по умолчанию 10 ГБ) на основном диске и на диске
# бэкапов. Порог абсолютный, а не в процентах: на большом диске бэкапов 2% свободного — это
# сотни гигабайт, а бэкап «Зачтено» весит мегабайты.
require_free_space() {
  local need=${BACKUP_MIN_FREE_GB:-10} target free
  for target in "$ZACHTENO_HOME" "$BACKUP_DATA_DIR"; do
    free=$(df --output=avail -B1G "$target" | tail -1 | tr -dc '0-9')
    [ "$free" -ge "$need" ] || { echo "на диске с $target свободно ${free} ГБ, нужно не меньше ${need} ГБ (BACKUP_MIN_FREE_GB)" >&2; return 1; }
  done
}

# Отметка в ops_status (её читают метрики приложения).
mark() { # mark <name> success|failure <details>
  local column=last_success_at; [ "$2" = failure ] && column=last_failure_at
  compose exec -T postgres psql -U postgres -d zachteno -v ON_ERROR_STOP=1 -q -c \
    "INSERT INTO ops_status (name, $column, details) VALUES ('$1', now(), left('$3', 500))
     ON CONFLICT (name) DO UPDATE SET $column = now(), details = EXCLUDED.details" >/dev/null
}

# Push-мониторинг (Uptime Kuma, Healthchecks и т. п.): отсутствие push — тревога.
# PUSH_NETWORK — если монитор доступен только внутри docker-сети, push идёт
# из одноразового контейнера в ней, и в URL можно писать имя контейнера.
push() { # push <url-variable-name> <message>
  local url="${!1:-}"; [ -n "$url" ] || return 0
  local client=(curl)
  [ -n "${PUSH_NETWORK:-}" ] && client=(docker run --rm --network "$PUSH_NETWORK" curlimages/curl:8.17.0)
  "${client[@]}" -fsS -m 10 -G "$url" --data-urlencode "status=up" --data-urlencode "msg=$2" >/dev/null \
    || echo "push в мониторинг не прошёл" >&2
}
