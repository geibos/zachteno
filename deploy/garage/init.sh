#!/usr/bin/env bash
# Идемпотентная инициализация single-node Garage: layout, ключ, бакеты.
# Использование: init.sh <compose-файл> <сервис> <key-id> <key-secret> <bucket>...
set -euo pipefail
compose_file=$1; service=$2; key_id=$3; key_secret=$4; shift 4
garage() { docker compose -f "$compose_file" exec -T "$service" /garage "$@"; }

for _ in $(seq 1 30); do garage status >/dev/null 2>&1 && break; sleep 1; done
node_id=$(garage status 2>/dev/null | awk '/^[0-9a-f]{16}/ {print $1; exit}')
if garage layout show 2>/dev/null | grep -q "No nodes currently have a role"; then
  garage layout assign -z dc1 -c 10G "$node_id"
  garage layout apply --version 1
fi
garage key info "$key_id" >/dev/null 2>&1 || garage key import --yes -n zachteno "$key_id" "$key_secret"
for bucket in "$@"; do
  garage bucket info "$bucket" >/dev/null 2>&1 || garage bucket create "$bucket"
  garage bucket allow --read --write --owner "$bucket" --key "$key_id"
done
echo "garage ready: $*"
