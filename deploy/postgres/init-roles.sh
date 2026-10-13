#!/usr/bin/env bash
# Роли базы: владелец схемы (миграции) и роль приложения (только DML).
# Выполняется entrypoint'ом postgres один раз при создании кластера.
set -euo pipefail
psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<SQL
CREATE ROLE zachteno_owner LOGIN PASSWORD '${ZACHTENO_OWNER_PASSWORD}';
CREATE ROLE zachteno_app LOGIN PASSWORD '${ZACHTENO_APP_PASSWORD}';
CREATE DATABASE zachteno OWNER zachteno_owner;
SQL
psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname zachteno <<SQL
REVOKE ALL ON SCHEMA public FROM PUBLIC;
ALTER SCHEMA public OWNER TO zachteno_owner;
GRANT USAGE ON SCHEMA public TO zachteno_app;
ALTER DEFAULT PRIVILEGES FOR ROLE zachteno_owner IN SCHEMA public
  GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO zachteno_app;
ALTER DEFAULT PRIVILEGES FOR ROLE zachteno_owner IN SCHEMA public
  GRANT USAGE, SELECT ON SEQUENCES TO zachteno_app;
SQL
