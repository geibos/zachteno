# Восстановление «Зачтено» из бэкапа

Бэкапы лежат во втором экземпляре Garage (`backup-garage`), данные которого — в `BACKUP_DATA_DIR` на отдельном диске:

- `pg-backups/zachteno` — WAL-G: ежедневный base backup и непрерывный архив WAL (не реже раза в 5 минут, `archive_timeout=300`);
- `attachments-mirror` — зеркало картинок, `attachments-history/<время>` — удалённые и изменённые за 14 дней.

Хранение — 14 дней. Еженедельная проверка восстановления (`zachteno-restore-check.timer`) разворачивает последнюю копию во временный контейнер и выполняет контрольный запрос; итог виден в метрике `zachteno_restore_check_last_success_timestamp` и в push-мониторинге (`RESTORE_PUSH_URL`).

## Восстановить базу на момент времени

```bash
cd "$ZACHTENO_HOME"
src/deploy/compose/backup/restore-pitr.sh '2026-10-13 14:04:00+03'   # время по МСК
# скрипт остановит app и postgres, отложит текущие данные в data/postgres.before-restore-*,
# развернёт последний base backup и проиграет WAL до указанного момента
docker compose logs -f postgres        # дождаться "database system is ready to accept connections"
docker compose up -d app
```

Отрепетировать восстановление, не трогая рабочую базу: `src/deploy/compose/backup/pitr-drill.sh` (или `just pitr-drill` из `src/`). Скрипт ставит в рабочей базе две служебные метки, между ними фиксирует момент T, восстанавливает копию на T во временный контейнер и проверяет, что первая метка есть, а второй нет; метки затем удаляются.

Если нужен не последний base backup, а более ранний: `docker compose exec -u postgres postgres wal-g backup-list` и подставить имя в `restore-pitr.sh` вместо `LATEST`.

## Восстановить картинки

```bash
docker compose run --rm backup rclone sync vault:attachments-mirror main:attachments
# отдельный файл из истории:
docker compose run --rm backup rclone lsf -R vault:attachments-history | grep <ключ>
```

## Отказ основного диска

1. Восстановить `ZACHTENO_HOME` (src — `git clone`, `.env` — из хранилища секретов).
2. Поднять `backup-garage` (`docker compose up -d backup-garage`): его данные на внешнем диске, метаданные пересоздать нельзя — если пропали и они, перенести `data/backup-garage-meta` из последней копии сервера; без них данные Garage не читаются.
3. `docker compose up -d postgres garage`, затем `restore-pitr.sh` на нужный момент и `rclone sync` картинок обратно.
