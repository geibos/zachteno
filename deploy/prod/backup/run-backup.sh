#!/usr/bin/env bash
# Ежедневный бэкап: base backup WAL-G, зеркало картинок rclone, хранение 14 дней.
. "$(dirname "$0")/common.sh"

fail() { echo "бэкап не удался: $1" >&2; mark backup failure "$1" || true; exit 1; }
require_backup_disk || fail "нет внешнего диска"
require_free_space || fail "мало места"

compose exec -T -u postgres postgres wal-g backup-push /var/lib/postgresql/data || fail "wal-g backup-push"
compose exec -T -u postgres postgres wal-g delete retain FULL 14 --confirm || fail "wal-g delete retain"

stamp=$(date -u +%Y%m%dT%H%M%SZ)
compose run --rm -T backup rclone sync main:attachments vault:attachments-mirror \
  --backup-dir "vault:attachments-history/$stamp" --checksum || fail "rclone sync"
cutoff=$(date -u -d '14 days ago' +%Y%m%dT%H%M%SZ)
compose run --rm -T backup sh -c "rclone lsf --dirs-only vault:attachments-history | tr -d / | while read -r dir; do
  if [ \"\$dir\" \\< \"$cutoff\" ]; then rclone purge \"vault:attachments-history/\$dir\"; fi; done" || fail "очистка истории картинок"

mark backup success "base backup + зеркало картинок $stamp"
push UPTIME_PUSH_BACKUP "ok $stamp"
echo "бэкап готов: $stamp"
