#!/usr/bin/env bash
set -euo pipefail

if [[ "$PLATFORM_NAME" != iphonesimulator || "$ARCHS" != arm64 ]]; then
    echo "This packaging target requires the Apple Silicon iOS Simulator." >&2
    exit 1
fi

export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR="$DERIVED_FILE_DIR/cargo"
export IPHONEOS_DEPLOYMENT_TARGET="$IPHONEOS_DEPLOYMENT_TARGET"
export SLINT_EMBED_RESOURCES=embed-files

cargo build --locked --release --manifest-path "$SRCROOT/../Cargo.toml" \
    --target aarch64-apple-ios-sim --bin offering-recorder-slint
install -m 755 \
    "$CARGO_TARGET_DIR/aarch64-apple-ios-sim/release/offering-recorder-slint" \
    "$TARGET_BUILD_DIR/$EXECUTABLE_PATH"
