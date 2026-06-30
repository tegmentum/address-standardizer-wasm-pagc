//! Compile the vendored PAGC C core into a static library and link it
//! into the cdylib. The `cc` crate honours `CC_wasm32_wasip2` and the
//! `--target/--sysroot` flags from the wasi-sdk env.sh, so the host
//! invocation:
//!
//!     source ~/.wasi-sdk/env.sh
//!     cargo component build --release --target wasm32-wasip2
//!
//! produces a single wasm component with PAGC linked in.

use std::env;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let pagc_dir = manifest_dir.join("src/pagc");

    // Rerun if any vendored source changes.
    println!("cargo:rerun-if-changed=src/pagc");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=WASI_SDK_PATH");

    // Files known to be PG-server-tied or PCRE-tied are pruned from the
    // vendored tree; the remaining .c files compile against our shim
    // `postgres.h` (also in src/pagc).
    let sources = [
        "analyze.c",
        "err_param.c",
        "export.c",
        "gamma.c",
        "hash.c",
        "lexicon.c",
        "pagc_tools.c",
        "standard.c",
        "tokenize.c",
        "wasm_loader.c",
    ];

    let mut build = cc::Build::new();
    build
        .include(&pagc_dir)
        // BUILD_API is also self-#defined inside pagc_api.h, so we let the
        // header set it rather than passing -DBUILD_API on the command line
        // (the latter triggers -Wmacro-redefined for every .c we compile).
        .flag_if_supported("-Wno-unused-function")
        .flag_if_supported("-Wno-unused-variable")
        .flag_if_supported("-Wno-unused-but-set-variable")
        .flag_if_supported("-Wno-implicit-function-declaration")
        .flag_if_supported("-Wno-parentheses")
        .flag_if_supported("-Wno-pointer-sign")
        .flag_if_supported("-Wno-incompatible-pointer-types");

    for src in &sources {
        build.file(pagc_dir.join(src));
    }

    build.compile("pagc");
}
