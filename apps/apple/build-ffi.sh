#!/bin/sh
# Builds akari-ffi for macOS and puts the XCFramework and its Swift bindings into AkariKit.
# Usage: apps/apple/build-ffi.sh [--host-only] [--debug]; see apps/apple/AkariKit/README.md.
set -eu

profile=release
host_only=false
for arg in "$@"; do
    case "$arg" in
        --debug) profile=debug ;;
        --host-only) host_only=true ;;
        *)
            echo "usage: $0 [--host-only] [--debug]" >&2
            exit 64
            ;;
    esac
done

root=$(cd "$(dirname "$0")/../.." && pwd)
kit="$root/apps/apple/AkariKit"
work="$root/target/apple"
cd "$root"

case "$(uname -m)" in
    arm64) host=aarch64-apple-darwin ;;
    x86_64) host=x86_64-apple-darwin ;;
    *)
        echo "unsupported Mac architecture: $(uname -m)" >&2
        exit 1
        ;;
esac
if $host_only; then
    targets=$host
else
    targets="aarch64-apple-darwin x86_64-apple-darwin"
fi
release_flag=
if [ "$profile" = release ]; then
    release_flag=--release
fi

# The same minimum macOS as Package.swift, so the linker doesn't warn about newer objects.
export MACOSX_DEPLOYMENT_TARGET=14.0
libraries=
for target in $targets; do
    # Always -p akari-ffi: a workspace build would turn on akari-cli's dev features.
    cargo build -p akari-ffi --locked $release_flag --target "$target"
    libraries="$libraries target/$target/$profile/libakari_ffi.a"
done

rm -rf "$work"
mkdir -p "$work/include" "$work/swift"
# shellcheck disable=SC2086 # one argument per library
lipo -create $libraries -output "$work/libakari_ffi.a"

# A separate cargo invocation, so the generator's features never reach the library build.
cargo run -p akari-bindgen --locked --quiet -- \
    --swift-sources --headers --modulemap \
    --module-name akari_ffiFFI --modulemap-filename module.modulemap \
    "target/$host/$profile/libakari_ffi.a" "$work/swift"
mv "$work/swift/akari_ffiFFI.h" "$work/swift/module.modulemap" "$work/include/"

xcodebuild -create-xcframework \
    -library "$work/libakari_ffi.a" -headers "$work/include" \
    -output "$work/akari_ffiFFI.xcframework" >/dev/null

rm -rf "$kit/Frameworks/akari_ffiFFI.xcframework" "$kit/Sources/AkariFFI/Generated"
mkdir -p "$kit/Frameworks" "$kit/Sources/AkariFFI/Generated"
mv "$work/akari_ffiFFI.xcframework" "$kit/Frameworks/"
mv "$work/swift/akari_ffi.swift" "$kit/Sources/AkariFFI/Generated/"
echo "AkariKit is ready: $kit"
