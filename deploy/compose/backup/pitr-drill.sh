#!/usr/bin/env bash
# Учебное восстановление на момент времени, не трогая рабочую базу (docs/ops/restore.md).
# В рабочей базе ставятся метки A и B, между ними фиксируется момент T. Последний base backup
# с WAL восстанавливается во временный Postgres на момент T: метка A должна быть, B — нет.
. "$(dirname "$0")/common.sh"
require_backup_disk

sql() { compose exec -T postgres psql -U postgres -d zachteno -v ON_ERROR_STOP=1 -qAt -c "$1"; }
cleanup_marks() { sql "DELETE FROM ops_status WHERE name IN ('pitr_drill_a', 'pitr_drill_b')" >/dev/null || true; }
name=zachteno-pitr-drill
trap 'docker rm -f "$name" >/dev/null 2>&1 || true; cleanup_marks' EXIT

sql "INSERT INTO ops_status (name, last_success_at, details) VALUES ('pitr_drill_a', now(), 'до момента T')
     ON CONFLICT (name) DO UPDATE SET last_success_at = now()" >/dev/null
sleep 2
target=$(sql "SELECT now()")
sleep 2
sql "INSERT INTO ops_status (name, last_success_at, details) VALUES ('pitr_drill_b', now(), 'после момента T')
     ON CONFLICT (name) DO UPDATE SET last_success_at = now()" >/dev/null
# Сегмент с меткой B должен уйти в архив: иначе восстановление не дойдёт до T.
sql "SELECT pg_switch_wal()" >/dev/null
archived_before=$(sql "SELECT archived_count FROM pg_stat_archiver")
for _ in $(seq 1 60); do
  [ "$(sql "SELECT archived_count FROM pg_stat_archiver")" -gt "$archived_before" ] && break; sleep 2
done
echo "момент T: $target"

docker rm -f "$name" >/dev/null 2>&1 || true
docker run -d --name "$name" --network zachteno_backup \
  -e WALG_S3_PREFIX=s3://pg-backups/zachteno -e AWS_ENDPOINT=http://backup-garage:3900 -e AWS_REGION=garage \
  -e AWS_S3_FORCE_PATH_STYLE=true -e AWS_ACCESS_KEY_ID="$BACKUP_S3_KEY_ID" -e AWS_SECRET_ACCESS_KEY="$BACKUP_S3_SECRET" \
  -e TARGET="$target" --entrypoint sleep zachteno-postgres:latest infinity >/dev/null
result=$(docker exec -u postgres "$name" bash -c '
  set -euo pipefail
  export PGDATA=/tmp/restore
  wal-g backup-fetch "$PGDATA" LATEST >/dev/null 2>&1
  touch "$PGDATA/recovery.signal"
  cat >> "$PGDATA/postgresql.auto.conf" <<CONF
restore_command = '"'"'wal-g wal-fetch %f %p'"'"'
recovery_target_time = '"'"'$TARGET'"'"'
recovery_target_action = '"'"'promote'"'"'
archive_mode = off
CONF
  pg_ctl -D "$PGDATA" -w -t 600 -l /tmp/restore.log start -o "-c listen_addresses= -c port=5433" >/dev/null
  for _ in $(seq 1 120); do [ "$(psql -p 5433 -At -c "select pg_is_in_recovery()" postgres)" = f ] && break; sleep 2; done
  psql -p 5433 -At -d zachteno -c "select string_agg(name, \",\" order by name) from ops_status where name like '"'"'pitr_drill_%'"'"'"
')
echo "метки в копии на момент T: ${result:-нет}"
[ "$result" = pitr_drill_a ] || { echo "учение не пройдено: ожидалась только pitr_drill_a" >&2; exit 1; }
echo "учение пройдено: копия на момент T содержит A и не содержит B"
