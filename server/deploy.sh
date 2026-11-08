#!/usr/bin/env bash
# Build and (re)start the multiplayer server on the "hermes" host (port 4001).
set -euo pipefail
cd "$(dirname "$0")"
HOST=${BTS_DEPLOY_HOST:-hermes}
DIR=${BTS_DEPLOY_DIR:-bts-server}
go vet ./... && go test -count=1 ./... >/dev/null
tar --exclude='*_test.go' -czf - go.mod *.go Dockerfile docker-compose.yml .dockerignore \
  | ssh "$HOST" "mkdir -p $DIR && tar -xzf - -C $DIR"
ssh "$HOST" "cd $DIR && docker compose up -d --build && docker compose ps"
