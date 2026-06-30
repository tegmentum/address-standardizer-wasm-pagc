#!/usr/bin/env bash
#
# Build the address-standardizer-wasm-pagc component.
#
#   ./scripts/build.sh
#
# Expects:
#   - wasi-sdk available at $WASI_SDK_PATH (else ~/.wasi-sdk)
#   - cargo-component installed (cargo install cargo-component)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

: "${WASI_SDK_PATH:=$HOME/.wasi-sdk}"

if [[ ! -d "$WASI_SDK_PATH" ]]; then
    echo "WASI SDK not found at $WASI_SDK_PATH" >&2
    echo "Install wasi-sdk-28+ and set WASI_SDK_PATH, or use the bundled env.sh." >&2
    exit 1
fi

# Source the wasi-sdk env shim so the `cc` build-dep picks up the right
# clang and sysroot for the wasm32-wasip2 target.
if [[ -f "$WASI_SDK_PATH/env.sh" ]]; then
    # shellcheck disable=SC1091
    source "$WASI_SDK_PATH/env.sh"
else
    export CC_wasm32_wasip2="$WASI_SDK_PATH/bin/clang"
    export AR_wasm32_wasip2="$WASI_SDK_PATH/bin/llvm-ar"
    export CFLAGS_wasm32_wasip2="--sysroot=$WASI_SDK_PATH/share/wasi-sysroot"
fi

cd "$PROJECT_DIR"
cargo component build --release --target wasm32-wasip2 "$@"

ART="target/wasm32-wasip2/release/address_standardizer_wasm_pagc.wasm"
if [[ -f "$ART" ]]; then
    echo
    echo "Built: $ART ($(wc -c < "$ART") bytes)"
fi
