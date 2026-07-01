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

    // pcre2-wasm sibling checkout: provides libpcre2-8.a for wasm32-wasip2
    // and the pcre2.h public header (in deps/pcre2/). Location is
    // overridable so CI can point at whatever tree it cloned pcre2-wasm
    // into; defaults to the developer layout in the sqlink workspace.
    let pcre2_root = env::var("PCRE2_WASM_ROOT").unwrap_or_else(|_| {
        PathBuf::from(env::var("HOME").unwrap_or_default())
            .join("git/pcre2-wasm")
            .to_string_lossy()
            .into_owned()
    });
    let pcre2_root = PathBuf::from(pcre2_root);
    let pcre2_include = pcre2_root.join("deps/pcre2");
    let pcre2_lib = pcre2_root.join("build/lib");

    // Rerun if any vendored source changes.
    println!("cargo:rerun-if-changed=src/pagc");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=WASI_SDK_PATH");
    println!("cargo:rerun-if-env-changed=PCRE2_WASM_ROOT");

    // Vendored PAGC sources. `parseaddress-api.c` and `wasm_parser.c`
    // are the PCRE2-driven parse path added in #708; the rest is the
    // standardize path used since #698. `address_parser.c` is checked
    // in for provenance but not compiled — it is a PG SRF wrapper
    // (PG_FUNCTION_ARGS, TupleDesc, palloc/pfree against a PG memory
    // context) and is replaced in the wasm build by `wasm_parser.c`.
    let sources = [
        "analyze.c",
        "err_param.c",
        "export.c",
        "gamma.c",
        "hash.c",
        "lexicon.c",
        "pagc_tools.c",
        "parseaddress-api.c",
        "standard.c",
        "tokenize.c",
        "wasm_loader.c",
        "wasm_parser.c",
    ];

    let mut build = cc::Build::new();
    build
        .include(&pagc_dir)
        .include(&pcre2_include)
        // parseaddress-api.c gates its pcre1 vs pcre2 include on a
        // preprocessor PCRE_VERSION; the `#if PCRE_VERSION <= 1` branch
        // pulls in <pcre.h> (pcre1) which we do not ship. Force the
        // pcre2 branch.
        .define("PCRE_VERSION", "2")
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

    // Link the sibling pcre2-wasm's libpcre2-8.a statically. The .a was
    // produced against wasm32-wasip2 by the pcre2-wasm CMake build, so
    // linking is only valid when this crate is built for that target.
    // For native builds (e.g. host-side `cargo test`) fall back to a
    // system pcre2-8 via pkg-config so unit tests still link; the
    // parity corpus runs on the wasm build and does not exercise
    // `parse()` on the native path.
    let target = env::var("TARGET").unwrap_or_default();
    if target.starts_with("wasm32") {
        if !pcre2_lib.join("libpcre2-8.a").exists() {
            panic!(
                "libpcre2-8.a not found at {}. Set PCRE2_WASM_ROOT to a pcre2-wasm checkout that has been built.",
                pcre2_lib.display()
            );
        }
        println!("cargo:rustc-link-search=native={}", pcre2_lib.display());
        println!("cargo:rustc-link-lib=static=pcre2-8");
    } else if let Ok(lib) = pkg_config::Config::new()
        .atleast_version("10.0")
        .probe("libpcre2-8")
    {
        for p in &lib.link_paths {
            println!("cargo:rustc-link-search=native={}", p.display());
        }
        for l in &lib.libs {
            println!("cargo:rustc-link-lib={l}");
        }
    } else {
        // No system pcre2 available; link the pkg name and let ld
        // surface the missing lib with a clear message.
        println!("cargo:rustc-link-lib=pcre2-8");
    }
}
