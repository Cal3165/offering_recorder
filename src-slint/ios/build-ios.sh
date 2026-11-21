#!/usr/bin/env bash
set -euo pipefail

case "$PLATFORM_NAME:$ARCHS" in
    iphonesimulator:arm64) rust_target=aarch64-apple-ios-sim ;;
    iphoneos:arm64) rust_target=aarch64-apple-ios ;;
    *) echo "Unsupported iOS platform or architecture: $PLATFORM_NAME ($ARCHS)" >&2; exit 1 ;;
esac

export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR="$DERIVED_FILE_DIR/cargo"
export IPHONEOS_DEPLOYMENT_TARGET="$IPHONEOS_DEPLOYMENT_TARGET"
export SLINT_EMBED_RESOURCES=true
export CARGO_PROFILE_RELEASE_DEBUG=1

cargo build --locked --release --manifest-path "$SRCROOT/../Cargo.toml" \
    --target "$rust_target" --bin offering-recorder-slint
install -m 755 \
    "$CARGO_TARGET_DIR/$rust_target/release/offering-recorder-slint" \
    "$TARGET_BUILD_DIR/$EXECUTABLE_PATH"
