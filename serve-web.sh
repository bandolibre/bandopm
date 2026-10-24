#!/usr/bin/env bash
set -euo pipefail

# Navigate to repository root
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT_DIR"

PORT="${1:-8080}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "[serve-web] ERROR: cargo not found. Install Rust from https://rustup.rs" >&2
  exit 1
fi

echo "[serve-web] Building synth-wasm (wasm32-unknown-unknown --release)..."
if ! cargo build --release --target wasm32-unknown-unknown -p synth-wasm; then
  echo "[serve-web] ERROR: wasm build failed (see compiler output above)." >&2
  if command -v rustup >/dev/null 2>&1 \
    && ! rustup target list --installed 2>/dev/null | grep -qx wasm32-unknown-unknown; then
    echo "[serve-web] The wasm32-unknown-unknown target is not installed. Fix with:" >&2
    echo "[serve-web]   rustup target add wasm32-unknown-unknown" >&2
  fi
  exit 1
fi
cp target/wasm32-unknown-unknown/release/synth_wasm.wasm docs/synth_wasm.wasm

echo "[serve-web] Serving docs/ on http://localhost:${PORT}/ (Ctrl+C to stop)"
exec python3 -m http.server "$PORT" --directory docs
