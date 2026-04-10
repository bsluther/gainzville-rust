#!/usr/bin/env bash
# Rebuilds the XCFramework from the current Rust implementation without
# regenerating Swift bindings.
#
# ONLY safe when changes are purely internal Rust (no edits to #[uniffi::export]
# signatures, FfiAction/FfiActivity/FfiError types, or anything in types.rs).
# If anything in the FFI surface changed, use regen-bindings.sh instead —
# UniFFI embeds interface checksums in both the Swift bindings and the compiled
# library, and a mismatch causes a fatal crash at launch.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

echo "==> Building release targets..."
cargo build --release --target aarch64-apple-ios
cargo build --release --target aarch64-apple-ios-sim

echo "==> Assembling XCFramework..."
rm -rf swift-app/Frameworks/GvFfi.xcframework
xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios/release/libgv_ffi.a \
    -headers gv-ffi/bindings/ \
  -library target/aarch64-apple-ios-sim/release/libgv_ffi.a \
    -headers gv-ffi/bindings/ \
  -output swift-app/Frameworks/GvFfi.xcframework

echo "Done."
