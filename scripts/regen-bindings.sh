#!/usr/bin/env bash
# Regenerates Swift bindings from the UDL/proc-macros, then rebuilds the
# XCFramework.  Run this whenever the exported FFI API changes.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

echo "==> Building debug dylib..."
cargo build -p gv_ffi

echo "==> Generating Swift bindings..."
cargo run --bin uniffi-bindgen -- generate \
  --library target/debug/libgv_ffi.dylib \
  --language swift \
  --out-dir gv-ffi/bindings/

echo "==> Copying bindings into swift-app..."
cp gv-ffi/bindings/gv_ffi.swift        swift-app/Gainzville/gv_ffi.swift
cp gv-ffi/bindings/gv_ffiFFI.h         swift-app/Frameworks/gv_ffiFFI.h
cp gv-ffi/bindings/gv_ffiFFI.modulemap swift-app/Frameworks/gv_ffiFFI.modulemap

echo "==> Building release targets..."
cargo build --release --target aarch64-apple-ios
cargo build --release --target aarch64-apple-ios-sim
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin

echo "==> Creating universal macOS library..."
mkdir -p target/macos-universal/release
lipo -create \
  target/aarch64-apple-darwin/release/libgv_ffi.a \
  target/x86_64-apple-darwin/release/libgv_ffi.a \
  -output target/macos-universal/release/libgv_ffi.a

echo "==> Assembling XCFramework..."
rm -rf swift-app/Frameworks/GvFfi.xcframework
xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios/release/libgv_ffi.a \
    -headers gv-ffi/bindings/ \
  -library target/aarch64-apple-ios-sim/release/libgv_ffi.a \
    -headers gv-ffi/bindings/ \
  -library target/macos-universal/release/libgv_ffi.a \
    -headers gv-ffi/bindings/ \
  -output swift-app/Frameworks/GvFfi.xcframework

echo "Done."
