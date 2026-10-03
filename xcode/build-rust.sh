#!/bin/bash
set -euo pipefail
cd "$PROJECT_DIR"
if [ -f "$HOME/.cargo/env" ]; then
  source "$HOME/.cargo/env"
fi
case "$PLATFORM_NAME" in
  iphoneos) RUST_TARGET=aarch64-apple-ios ;;
  iphonesimulator)
    case "${CURRENT_ARCH:-${NATIVE_ARCH_ACTUAL:-arm64}}" in
      arm64) RUST_TARGET=aarch64-apple-ios-sim ;;
      x86_64) RUST_TARGET=x86_64-apple-ios ;;
      *) echo 'Unsupported simulator architecture' >&2; exit 1 ;;
    esac ;;
  *) echo "Unsupported PLATFORM_NAME=$PLATFORM_NAME" >&2; exit 1 ;;
esac
PROFILE_ARGS=()
PROFILE_DIR=debug
if [ "$CONFIGURATION" = Release ]; then
  PROFILE_ARGS=(--release)
  PROFILE_DIR=release
fi
export CARGO_TARGET_DIR="$SRCROOT/target/xcode"
export SDKROOT
SDKROOT="$(xcrun --sdk "$PLATFORM_NAME" --show-sdk-path)"
cargo build --locked -p phira-main --bin phira-main --target "$RUST_TARGET" "${PROFILE_ARGS[@]}"
mkdir -p "$PROJECT_DIR/build"
cp "$CARGO_TARGET_DIR/$RUST_TARGET/$PROFILE_DIR/phira-main" "$PROJECT_DIR/build/phira-main"
