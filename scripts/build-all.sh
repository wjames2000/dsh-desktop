#!/usr/bin/env bash
# 完整构建：prepare-bundle + tauri build
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# tauri-cli 装在 $CARGO_HOME/bin（本机 .dsh-toolchain/cargo/bin）；
# 构建时 PATH 需包含该目录，否则 `cargo tauri` 找不到。
# 构建机 tauri-cli 装在哪就加哪的 PATH，也可以改用 `cargo tauri` 全路径。
if [ -n "${CARGO_HOME:-}" ] && [ -d "$CARGO_HOME/bin" ]; then
  export PATH="$CARGO_HOME/bin:$HOME/.cargo/bin:${PATH:-}"
else
  export PATH="$HOME/.cargo/bin:${PATH:-}"
fi

./scripts/prepare-bundle.sh
cd src-tauri
cargo tauri build
