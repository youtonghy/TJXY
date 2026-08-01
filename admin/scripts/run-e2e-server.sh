#!/bin/sh
set -eu

admin_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
repo_dir=$(CDPATH= cd -- "$admin_dir/.." && pwd)
run_dir=$(mktemp -d "${TMPDIR:-/tmp}/tjxy-e2e.XXXXXX")

cleanup() {
  rm -rf "$run_dir"
}
trap cleanup EXIT INT TERM

mkdir -p "$run_dir/assets"

export TJXY_SERVER_ID="018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11"
export TJXY_SERVER_NAME="TJXY E2E"
export TJXY_BIND="127.0.0.1:${TJXY_E2E_PORT:-18096}"
export TJXY_DATABASE_URL="sqlite://$run_dir/tjxy.db?mode=rwc"
export TJXY_ASSETS_DIR="$run_dir/assets"
export TJXY_REDIS_MODE="disabled"
export TJXY_ENABLE_REMOTE_PROVIDERS="false"
export TJXY_FILESYSTEM_REALTIME="false"
export TJXY_CREDENTIAL_KEYRING='{"active_version":1,"keys":{"1":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="}}'
export TJXY_BOOTSTRAP_ADMIN_USERNAME="Admin"
export TJXY_BOOTSTRAP_ADMIN_PASSWORD="admin-password"
export TJXY_ADMIN_DIST_DIR="$admin_dir/dist"

cd "$repo_dir"
cargo +1.88.0 run -p tjxy-server --bin tjxy-server --locked
