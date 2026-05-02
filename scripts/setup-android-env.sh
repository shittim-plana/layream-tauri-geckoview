#!/usr/bin/env bash
set -euo pipefail

# =============================================================================
# Fully persistent Android build environment for Layream (Tauri 2.0)
# Host: aarch64 Linux — everything under /config/android/ (persists)
#
# All tools are aarch64 native — no emulation, no symlink tricks.
#
# /config/android/
#   jdk17/         — Eclipse Temurin OpenJDK 17 (aarch64)
#   ndk/r29/       — Android NDK r29 (aarch64-linux-musl, HomuHomu833)
#   sdk/           — Android SDK (cmdline-tools, platform-tools, platforms)
#   build-tools/   — aarch64 aapt2/aapt/zipalign (lzhiyong 35.0.2)
#
# After initial setup (~400MB), restart only needs: source env.sh
# =============================================================================

ANDROID_BASE="/config/android"
JDK_DIR="$ANDROID_BASE/jdk17"
NDK_DIR="$ANDROID_BASE/ndk/r29"
SDK_DIR="$ANDROID_BASE/sdk"
BT_DIR="$SDK_DIR/build-tools/35.0.0"
TMPDIR="/tmp/android-setup"

JDK_ARCHIVE="OpenJDK17U-jdk_aarch64_linux_hotspot_17.0.13_11.tar.gz"
JDK_URL="https://github.com/adoptium/temurin17-binaries/releases/download/jdk-17.0.13%2B11/$JDK_ARCHIVE"

NDK_ARCHIVE="android-ndk-r29-aarch64-linux-musl.tar.xz"
NDK_URL="https://github.com/HomuHomu833/android-ndk-custom/releases/download/r29/$NDK_ARCHIVE"

BT_ARCHIVE="android-sdk-tools-static-aarch64.zip"
BT_URL="https://github.com/lzhiyong/android-sdk-tools/releases/download/35.0.2/$BT_ARCHIVE"

CMDLINE_TOOLS_ZIP="commandlinetools-linux-11076708_latest.zip"
CMDLINE_TOOLS_URL="https://dl.google.com/android/repository/$CMDLINE_TOOLS_ZIP"

mkdir -p "$ANDROID_BASE" "$TMPDIR"

# =============================================================================
echo "=== [1/5] JDK 17 → $JDK_DIR ==="
# =============================================================================
if [ -f "$JDK_DIR/bin/java" ]; then
    echo "JDK 17 already installed"
else
    echo "Downloading Eclipse Temurin JDK 17 for aarch64..."
    [ ! -f "$TMPDIR/$JDK_ARCHIVE" ] && curl -fSL -o "$TMPDIR/$JDK_ARCHIVE" "$JDK_URL"
    mkdir -p "$JDK_DIR"
    tar -xzf "$TMPDIR/$JDK_ARCHIVE" -C "$JDK_DIR" --strip-components=1
    echo "JDK 17 installed"
fi
echo "  java: $($JDK_DIR/bin/java -version 2>&1 | head -1)"

export JAVA_HOME="$JDK_DIR"
export PATH="$JAVA_HOME/bin:$PATH"

# =============================================================================
echo ""
echo "=== [2/5] NDK r29 (aarch64 native) → $NDK_DIR ==="
# =============================================================================
if [ -f "$NDK_DIR/source.properties" ]; then
    echo "NDK r29 already installed"
else
    echo "Downloading NDK r29 aarch64-linux-musl (~197MB)..."
    [ ! -f "$TMPDIR/$NDK_ARCHIVE" ] && curl -fSL -o "$TMPDIR/$NDK_ARCHIVE" "$NDK_URL"
    mkdir -p "$ANDROID_BASE/ndk"
    echo "Extracting NDK..."
    tar -xf "$TMPDIR/$NDK_ARCHIVE" -C "$ANDROID_BASE/ndk"
    mv "$ANDROID_BASE/ndk/android-ndk-r29" "$NDK_DIR"
    echo "NDK r29 installed"
fi

NDK_VERSION=$(grep "Pkg.Revision" "$NDK_DIR/source.properties" | cut -d= -f2 | tr -d ' ')
echo "  NDK: $NDK_VERSION"
echo "  clang: $($NDK_DIR/toolchains/llvm/prebuilt/linux-arm64/bin/clang --version 2>&1 | head -1)"

# Create version symlink for Gradle
mkdir -p "$SDK_DIR/ndk"
ln -sfn "$NDK_DIR" "$SDK_DIR/ndk/$NDK_VERSION"

# =============================================================================
echo ""
echo "=== [3/5] Android SDK cmdline-tools → $SDK_DIR ==="
# =============================================================================
SDKMANAGER="$SDK_DIR/cmdline-tools/latest/bin/sdkmanager"
if [ -f "$SDKMANAGER" ]; then
    echo "cmdline-tools already installed"
else
    echo "Downloading Android cmdline-tools..."
    [ ! -f "$TMPDIR/$CMDLINE_TOOLS_ZIP" ] && curl -fSL -o "$TMPDIR/$CMDLINE_TOOLS_ZIP" "$CMDLINE_TOOLS_URL"
    mkdir -p "$SDK_DIR/cmdline-tools"
    rm -rf "$TMPDIR/cmdline-tools"
    unzip -qo "$TMPDIR/$CMDLINE_TOOLS_ZIP" -d "$TMPDIR"
    rm -rf "$SDK_DIR/cmdline-tools/latest"
    mv "$TMPDIR/cmdline-tools" "$SDK_DIR/cmdline-tools/latest"
    echo "cmdline-tools installed"
fi

export ANDROID_HOME="$SDK_DIR"
echo "Accepting SDK licenses..."
yes 2>/dev/null | "$SDKMANAGER" --licenses --sdk_root="$SDK_DIR" 2>&1 | tail -3 || true

echo "Installing platform-tools + platforms..."
"$SDKMANAGER" --sdk_root="$SDK_DIR" "platform-tools" "platforms;android-35" 2>&1 | tail -3

# =============================================================================
echo ""
echo "=== [4/5] Build-tools (aarch64 native) → $BT_DIR ==="
# =============================================================================
if [ -f "$BT_DIR/aapt2" ] && "$BT_DIR/aapt2" version >/dev/null 2>&1; then
    echo "Build-tools already installed"
else
    echo "Downloading aarch64 build-tools (lzhiyong 35.0.2, ~14MB)..."
    [ ! -f "$TMPDIR/$BT_ARCHIVE" ] && curl -fSL -o "$TMPDIR/$BT_ARCHIVE" "$BT_URL"
    mkdir -p "$BT_DIR"
    unzip -qo "$TMPDIR/$BT_ARCHIVE" -d "$TMPDIR/bt-extract"
    cp "$TMPDIR/bt-extract/build-tools/aapt2" "$BT_DIR/aapt2"
    cp "$TMPDIR/bt-extract/build-tools/aapt" "$BT_DIR/aapt"
    cp "$TMPDIR/bt-extract/build-tools/zipalign" "$BT_DIR/zipalign"
    chmod +x "$BT_DIR/aapt2" "$BT_DIR/aapt" "$BT_DIR/zipalign"
    rm -rf "$TMPDIR/bt-extract"
    echo "Build-tools installed"
fi
echo "  aapt2: $($BT_DIR/aapt2 version 2>&1)"

# =============================================================================
echo ""
echo "=== [5/5] Verification ==="
# =============================================================================
echo "JAVA_HOME=$JDK_DIR"
echo "  java: $($JDK_DIR/bin/java -version 2>&1 | head -1)"
echo "NDK_HOME=$NDK_DIR"
echo "  clang: $($NDK_DIR/toolchains/llvm/prebuilt/linux-arm64/bin/clang --version 2>&1 | head -1)"
echo "ANDROID_HOME=$SDK_DIR"
echo "  platforms: $(ls "$SDK_DIR/platforms/" 2>/dev/null || echo 'none')"
echo "  aapt2: $($BT_DIR/aapt2 version 2>&1)"

echo ""
echo "=== Setup complete (fully persistent, all aarch64 native) ==="
echo ""
echo "Next session: source /config/workspace/layream/scripts/env.sh"
echo "Build:        cd layream-app && npm run tauri android build -- --apk --target aarch64"
