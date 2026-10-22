#!/usr/bin/env bash
set -euo pipefail

# Navigate to repository root
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT_DIR"

PORT="${1:-8080}"

echo "[serve-web] Building synth-wasm (wasm32-unknown-unknown --release)..."
cargo build --release --target wasm32-unknown-unknown -p synth-wasm
cp target/wasm32-unknown-unknown/release/synth_wasm.wasm docs/synth_wasm.wasm

echo "[serve-web] Serving docs/ on http://localhost:${PORT}/ (Ctrl+C to stop)"
exec python3 -m http.server "$PORT" --directory docs
