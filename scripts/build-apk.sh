#!/bin/bash
set -e

# Layream Android APK Build Script
# Usage: ./scripts/build-apk.sh [--sign] [--release]

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# Load Android build environment
source "$SCRIPT_DIR/env.sh"

# Ensure gcc is available (gets uninstalled on container restart)
which gcc >/dev/null 2>&1 || {
  echo "[build] Installing gcc..."
  sudo apt-get update -qq && sudo apt-get install -y -qq gcc pkg-config libdbus-1-dev
}

cd "$ROOT_DIR/layream-app"

# 1. Build frontend
echo "[build] Building frontend..."
npm run build

# 2. Copy frontend to Android assets (for GeckoView AssetServer)
echo "[build] Syncing assets..."
ASSETS_DIR="src-tauri/gen/android/app/src/main/assets"
rm -rf "$ASSETS_DIR/assets" "$ASSETS_DIR/index.html"
cp dist/index.html "$ASSETS_DIR/"
cp -r dist/assets "$ASSETS_DIR/"

# 3. Clean previous Rust build artifacts
echo "[build] Cleaning Rust cache..."
rm -rf ../target/aarch64-linux-android/release/deps/liblayream_app*

# 4. Full Tauri build (Rust + Kotlin + Gradle)
echo "[build] Building APK (this takes ~2-3 minutes)..."
CARGO_BUILD_JOBS=2 npm run tauri android build -- --apk --target aarch64

APK_PATH="src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release-unsigned.apk"

if [ ! -f "$APK_PATH" ]; then
  echo "[build] ERROR: APK not found at $APK_PATH"
  exit 1
fi

echo "[build] APK built: $(du -h "$APK_PATH" | cut -f1)"

# 5. Sign if requested
if [[ "$*" == *"--sign"* ]]; then
  VERSION="${VERSION:-dev}"
  OUT_APK="$ROOT_DIR/layream-v${VERSION}.apk"
  echo "[build] Signing APK..."
  $ANDROID_HOME/build-tools/35.0.0/apksigner sign \
    --ks "$ROOT_DIR/layream.keystore" \
    --ks-key-alias layream \
    --ks-pass pass:layream123 \
    --key-pass pass:layream123 \
    --out "$OUT_APK" \
    "$APK_PATH"
  echo "[build] Signed: $OUT_APK ($(du -h "$OUT_APK" | cut -f1))"
fi

echo "[build] Done."
