#!/usr/bin/env bash
# Еженедельная проверка восстановления: последняя копия разворачивается во временный Postgres,
# выполняется контрольный запрос (spec platform-operations, «Проверка восстановления»).
# RESTORE_CHECK_PREFIX — другой префикс WAL-G; для учения «Испорченная копия» — несуществующий.
. "$(dirname "$0")/common.sh"

fail() { echo "проверка восстановления не удалась: $1" >&2; mark restore_check failure "$1" || true; exit 1; }
require_backup_disk || fail "нет внешнего диска"

name=zachteno-restore-check
docker rm -f "$name" >/dev/null 2>&1 || true
trap 'docker rm -f "$name" >/dev/null 2>&1 || true' EXIT
docker run -d --name "$name" --network zachteno_backup \
  -e WALG_S3_PREFIX="${RESTORE_CHECK_PREFIX:-s3://pg-backups/zachteno}" -e AWS_ENDPOINT=http://backup-garage:3900 -e AWS_REGION=garage \
  -e AWS_S3_FORCE_PATH_STYLE=true -e AWS_ACCESS_KEY_ID="$BACKUP_S3_KEY_ID" -e AWS_SECRET_ACCESS_KEY="$BACKUP_S3_SECRET" \
  -e POSTGRES_PASSWORD=restore --entrypoint sleep zachteno-postgres:latest infinity >/dev/null || fail "запуск контейнера"
docker exec -u postgres "$name" bash -c '
  set -euo pipefail
  export PGDATA=/tmp/restore
  wal-g backup-fetch "$PGDATA" LATEST
  touch "$PGDATA/recovery.signal"
  echo "restore_command = '"'"'wal-g wal-fetch %f %p'"'"'" >> "$PGDATA/postgresql.auto.conf"
  echo "archive_mode = off" >> "$PGDATA/postgresql.auto.conf"
  pg_ctl -D "$PGDATA" -w -t 600 start -o "-c listen_addresses= -c port=5433"
  for _ in $(seq 1 120); do [ "$(psql -p 5433 -At -c "select pg_is_in_recovery()" postgres)" = f ] && break; sleep 2; done
  users=$(psql -p 5433 -At -d zachteno -c "select count(*) from users")
  echo "восстановлено, пользователей: $users"
' || fail "восстановление или контрольный запрос"

mark restore_check success "контрольный запрос выполнен"
push RESTORE_PUSH_URL "ok"
echo "проверка восстановления пройдена"
