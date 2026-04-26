#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "${SCRIPT_DIR}/../../.." && pwd)
CONTAINER_NAME="babar-playground-$$"
KEEP=0
REUSE=0

usage() {
  cat <<USAGE
playground.sh [--keep] [--reuse]

Starts a disposable Postgres container for the playground example and runs:
  cargo run -p babar --example playground
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --keep) KEEP=1 ;;
    --reuse) REUSE=1 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown flag: $1" >&2; usage; exit 1 ;;
  esac
  shift
done

cleanup() {
  if [[ $KEEP -eq 0 ]]; then
    docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

if [[ $REUSE -eq 0 ]]; then
  docker run -d --rm \
    --name "$CONTAINER_NAME" \
    -p 127.0.0.1:54320:5432 \
    -e POSTGRES_USER=babar \
    -e POSTGRES_PASSWORD=secret \
    -e POSTGRES_DB=babar \
    postgres:17-alpine >/dev/null
fi

export PGHOST=127.0.0.1
export PGPORT=54320
export PGUSER=babar
export PGPASSWORD=secret
export PGDATABASE=babar

cargo run -p babar --example playground
