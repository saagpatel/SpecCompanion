#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LEAN_TMP_ROOT="$(mktemp -d -t speccompanion-lean.XXXXXX)"

cleanup() {
  rm -rf "$LEAN_TMP_ROOT"
  (
    cd "$ROOT_DIR"
    pnpm run clean:heavy >/dev/null 2>&1 || true
  )
}
trap cleanup EXIT INT TERM

export CARGO_TARGET_DIR="$LEAN_TMP_ROOT/cargo-target"
export VITE_CACHE_DIR="$LEAN_TMP_ROOT/vite-cache"
export XDG_CACHE_HOME="$LEAN_TMP_ROOT/xdg-cache"

cd "$ROOT_DIR"
pnpm tauri dev "$@"
