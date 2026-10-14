# Команды разработки «Зачтено». Окружение — из .env.
set dotenv-load := true

default:
    @just --list

# Разработка: бэкенд и Vite с прокси /api; Ctrl+C останавливает оба
dev:
    #!/usr/bin/env bash
    set -euo pipefail
    trap 'kill 0' EXIT
    (cd backend && cargo run -q -p app -- serve) &
    (cd frontend && npm run dev) &
    wait

# Миграции базы
migrate:
    cd backend && cargo run -q -p app -- migrate

# Сервер с фронтендом из frontend/build
serve:
    cd backend && cargo run -q -p app -- serve

# Тесты бэкенда (нужен TEST_DATABASE_URL)
test:
    cd backend && DATABASE_URL="$TEST_DATABASE_URL" cargo test --workspace

# Линты бэкенда без предупреждений
lint:
    cd backend && cargo clippy --workspace --all-targets -- -D warnings
    cd backend && cargo fmt --all --check

fmt:
    cd backend && cargo fmt --all

# OpenAPI → TypeScript-типы фронтенда
gen-api:
    cd backend && cargo run -q -p app -- openapi > ../frontend/src/lib/api/openapi.json
    cd frontend && npx openapi-typescript src/lib/api/openapi.json -o src/lib/api/schema.d.ts

# Фронтенд
web-build:
    cd frontend && npm run build

web-check:
    cd frontend && npm run check
    cd frontend && npm run check:e2e
    cd frontend && npm test

# Сгенерированные OpenAPI-типы совпадают с кодом бэкенда
api-check: gen-api
    git diff --exit-code -- frontend/src/lib/api/openapi.json frontend/src/lib/api/schema.d.ts

# Полный набор проверок перед коммитом
ci: lint test web-check api-check
    openspec validate --all --strict

# Восстановление на момент времени (останавливает приложение): just restore-pitr '2026-10-13 14:04:00+03'
restore-pitr target:
    deploy/compose/backup/restore-pitr.sh '{{target}}'

# Учебное восстановление на момент времени во временный Postgres, рабочая база не трогается
pitr-drill:
    deploy/compose/backup/pitr-drill.sh
