#!/usr/bin/env bash
# Восстановление на момент времени (docs/ops/restore.md). ОСТАНАВЛИВАЕТ приложение.
# Использование: restore-pitr.sh '2026-10-13 14:04:00+03'
. "$(dirname "$0")/common.sh"
target=${1:?укажите момент восстановления, например '2026-10-13 14:04:00+03'}
require_backup_disk
echo "Будет восстановлена база на $target. Текущие данные переедут в data/postgres.before-restore-$(date +%s)."
read -r -p "Продолжить? (yes/no) " answer; [ "$answer" = yes ] || exit 1
compose stop app postgres
mv data/postgres "data/postgres.before-restore-$(date +%s)"
mkdir -p data/postgres
compose run --rm -T --no-deps -u postgres --entrypoint bash postgres -c "
  set -euo pipefail
  wal-g backup-fetch /var/lib/postgresql/data LATEST
  touch /var/lib/postgresql/data/recovery.signal
  cat >> /var/lib/postgresql/data/postgresql.auto.conf <<CONF
restore_command = 'wal-g wal-fetch %f %p'
recovery_target_time = '$target'
recovery_target_action = 'promote'
CONF"
compose up -d postgres
echo "Postgres восстанавливается до $target. Проверьте данные, затем: docker compose up -d app"
