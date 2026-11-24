#!/usr/bin/env bash
set -euo pipefail
arch="${1:-}"
case "$arch" in
  arm64-v8a) target=aarch64-unknown-linux-ohos; rust_arch=aarch64 ;;
  x86_64) target=x86_64-unknown-linux-ohos; rust_arch=x86_64 ;;
  *) echo 'usage: build-ohos.sh <arm64-v8a|x86_64>' >&2; exit 2 ;;
esac
: "${OHOS_NDK_HOME:?set OHOS_NDK_HOME to an installed OpenHarmony SDK/NDK}"
command -v rustup >/dev/null || { echo 'rustup not found' >&2; exit 1; }
command -v cmake >/dev/null || { echo 'cmake not found' >&2; exit 1; }
ndk_bin="${OHOS_NDK_HOME}/native/llvm/bin"
case "$arch" in
  arm64-v8a) linker="${ndk_bin}/aarch64-linux-ohos-clang"; cxx="${ndk_bin}/aarch64-linux-ohos-clang++" ;;
  x86_64) linker="${ndk_bin}/x86_64-linux-ohos-clang"; cxx="${ndk_bin}/x86_64-linux-ohos-clang++"; rust_arch=x86_64 ;;
esac
ar="${ndk_bin}/llvm-ar"
for tool in "$linker" "$cxx" "$ar" "${OHOS_NDK_HOME}/native/build/cmake/ohos.toolchain.cmake"; do
  [[ -e "$tool" ]] || { echo "OpenHarmony toolchain component not found: $tool" >&2; exit 1; }
done
[[ -x "$linker" && -x "$cxx" && -x "$ar" ]] || { echo 'OpenHarmony compiler/linker tools must be executable' >&2; exit 1; }
rustup target list --installed | grep -qx "$target" || { echo "Install Rust target first: rustup target add $target" >&2; exit 1; }
root="$(cd "$(dirname "$0")/.." && pwd)"
build="${root}/build/ohos-${arch}"
export CARGO_TARGET_"${rust_arch^^}"_UNKNOWN_LINUX_OHOS_LINKER="$linker"
export CARGO_TARGET_"${rust_arch^^}"_UNKNOWN_LINUX_OHOS_AR="$ar"
cmake -S "$root" -B "$build" \
  -DCMAKE_TOOLCHAIN_FILE="$OHOS_NDK_HOME/native/build/cmake/ohos.toolchain.cmake" \
  -DOHOS_ARCH="$arch" -DRUST_TARGET="$target" \
  -DCMAKE_C_COMPILER="$linker" \
  -DCMAKE_CXX_COMPILER="$cxx" \
  -DCMAKE_BUILD_TYPE=Release
cmake --build "$build" --parallel
