#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

APP_CRATE="rtxlauncher-ui-egui"
DIST="$ROOT/dist"
mkdir -p "$DIST"

echo "==> Ensuring Rust targets are installed"
rustup target add x86_64-unknown-linux-musl >/dev/null 2>&1 || true
rustup target add x86_64-pc-windows-gnu >/dev/null 2>&1 || true

echo "==> Building Linux (musl) release"
cargo build --release -p "$APP_CRATE" --target x86_64-unknown-linux-musl || {
  echo "WARN: musl build failed; falling back to glibc build"
  cargo build --release -p "$APP_CRATE"
}

LINUX_BIN_MUSL="$ROOT/target/x86_64-unknown-linux-musl/release/$APP_CRATE"
LINUX_BIN_GLIBC="$ROOT/target/release/$APP_CRATE"
if [[ -f "$LINUX_BIN_MUSL" ]]; then
  cp "$LINUX_BIN_MUSL" "$DIST/${APP_CRATE}-linux-x86_64"
elif [[ -f "$LINUX_BIN_GLIBC" ]]; then
  cp "$LINUX_BIN_GLIBC" "$DIST/${APP_CRATE}-linux-x86_64"
else
  echo "ERROR: Linux binary not found"
  exit 1
fi

if command -v strip >/dev/null 2>&1; then
  strip "$DIST/${APP_CRATE}-linux-x86_64" || true
fi

echo "==> Building Windows (gnu) release (attempt static CRT)"
export RUSTFLAGS="${RUSTFLAGS:-} -C target-feature=+crt-static"
if command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
  cargo build --release -p "$APP_CRATE" --target x86_64-pc-windows-gnu
else
  echo "WARN: x86_64-w64-mingw32-gcc not found; attempting build anyway (may fail)."
  cargo build --release -p "$APP_CRATE" --target x86_64-pc-windows-gnu || true
fi

WIN_BIN="$ROOT/target/x86_64-pc-windows-gnu/release/${APP_CRATE}.exe"
if [[ -f "$WIN_BIN" ]]; then
  cp "$WIN_BIN" "$DIST/${APP_CRATE}-windows-x86_64.exe"
  if command -v x86_64-w64-mingw32-strip >/dev/null 2>&1; then
    x86_64-w64-mingw32-strip "$DIST/${APP_CRATE}-windows-x86_64.exe" || true
  fi
else
  echo "ERROR: Windows build did not produce an .exe. Ensure MinGW-w64 toolchain is installed."
fi

echo "==> Artifacts in $DIST:"
ls -lh "$DIST" || true

echo "Done."
exit 0


