#!/usr/bin/env bash
# Spin up a throwaway Postgres on 127.0.0.1:54320 and run the playground
# example against it. Cleans up on exit.
#
# Override these env vars if you want a different setup; defaults match
# what playground.rs reads.
#
# Usage:
#   ./crates/core/examples/playground.sh           # full run with a fresh container
#   ./crates/core/examples/playground.sh --keep    # leave container running afterwards
#   ./crates/core/examples/playground.sh --reuse   # don't start a container; assume one exists

set -euo pipefail

PG_IMAGE="${PG_IMAGE:-postgres:17-alpine}"
PG_USER="${PGUSER:-babar}"
PG_PASSWORD="${PGPASSWORD:-secret}"
PG_DB="${PGDATABASE:-babar}"
PG_PORT="${PGPORT:-54320}"
CONTAINER_NAME="${CONTAINER_NAME:-babar-playground}"

mode="run"
case "${1:-}" in
    --keep) mode="keep" ;;
    --reuse) mode="reuse" ;;
    "") ;;
    *) echo "unknown flag: $1" >&2; exit 2 ;;
esac

cleanup() {
    if [[ "$mode" == "keep" ]]; then
        echo "leaving container ${CONTAINER_NAME} running on port ${PG_PORT}"
        return
    fi
    if [[ "$mode" == "reuse" ]]; then
        return
    fi
    docker rm -f "${CONTAINER_NAME}" >/dev/null 2>&1 || true
}
trap cleanup EXIT

if [[ "$mode" != "reuse" ]]; then
    # Make sure no stale container is in the way.
    docker rm -f "${CONTAINER_NAME}" >/dev/null 2>&1 || true

    echo "starting ${PG_IMAGE} as ${CONTAINER_NAME} on 127.0.0.1:${PG_PORT}"
    docker run -d --rm \
        --name "${CONTAINER_NAME}" \
        -p "127.0.0.1:${PG_PORT}:5432" \
        -e "POSTGRES_USER=${PG_USER}" \
        -e "POSTGRES_PASSWORD=${PG_PASSWORD}" \
        -e "POSTGRES_DB=${PG_DB}" \
        "${PG_IMAGE}" >/dev/null

    echo -n "waiting for postgres to accept connections"
    for _ in $(seq 1 60); do
        if docker exec "${CONTAINER_NAME}" pg_isready -U "${PG_USER}" -d "${PG_DB}" >/dev/null 2>&1; then
            echo " ready."
            break
        fi
        echo -n "."
        sleep 0.5
    done
fi

# cd to repo root so cargo finds the workspace.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
cd "${REPO_ROOT}"

PGHOST=127.0.0.1 \
PGPORT="${PG_PORT}" \
PGUSER="${PG_USER}" \
PGPASSWORD="${PG_PASSWORD}" \
PGDATABASE="${PG_DB}" \
    cargo run --example playground "$@"
