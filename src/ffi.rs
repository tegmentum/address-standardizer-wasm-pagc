//! Raw FFI surface over the vendored PAGC C library. PAGC's standalone
//! API (`pagc_std_api.h`) plus our `wasm_loader.c` builder are wrapped
//! here as plain `extern "C"` declarations; safe wrappers live in
//! `ops.rs`.

use core::ffi::{c_char, c_double, c_int};

#[repr(C)]
pub struct Standardizer {
    _opaque: [u8; 0],
}

/// Mirrors `ADDRESS` in `parseaddress-api.h`. Field ownership matches
/// `pagc_parse_address`: each `*mut c_char` is heap-allocated or NULL
/// and freed via `pagc_address_free`.
#[repr(C)]
pub struct Address {
    pub num: *mut c_char,
    pub street: *mut c_char,
    pub street2: *mut c_char,
    pub address1: *mut c_char,
    pub city: *mut c_char,
    pub st: *mut c_char,
    pub zip: *mut c_char,
    pub zipplus: *mut c_char,
    pub cc: *mut c_char,
    pub lat: c_double,
    pub lon: c_double,
}

/// Mirrors `STDADDR` in `pagc_std_api.h`. All 16 fields are owned C
/// strings or NULL; freed via `stdaddr_free`.
#[repr(C)]
pub struct Stdaddr {
    pub building: *mut c_char,
    pub house_num: *mut c_char,
    pub predir: *mut c_char,
    pub qual: *mut c_char,
    pub pretype: *mut c_char,
    pub name: *mut c_char,
    pub suftype: *mut c_char,
    pub sufdir: *mut c_char,
    pub ruralroute: *mut c_char,
    pub extra: *mut c_char,
    pub city: *mut c_char,
    pub state: *mut c_char,
    pub country: *mut c_char,
    pub postcode: *mut c_char,
    pub r#box: *mut c_char,
    pub unit: *mut c_char,
}

extern "C" {
    /// Build a `STANDARDIZER` from the three SQL data buffers vendored
    /// in `data/`. Defined in `src/pagc/wasm_loader.c`.
    pub fn pagc_build_standardizer(
        lex_sql: *const c_char,
        lex_len: usize,
        gaz_sql: *const c_char,
        gaz_len: usize,
        rules_sql: *const c_char,
        rules_len: usize,
    ) -> *mut Standardizer;

    /// Standardize a micro+macro-split address. The upstream PAGC
    /// `std_standardize_one` was commented out; the working entry
    /// point is `std_standardize_mm` which accepts the address line
    /// as `micro` and an optional `macro` (city/state/zip) buffer.
    /// Caller owns the returned `STDADDR` and frees via `stdaddr_free`.
    pub fn std_standardize_mm(
        std: *mut Standardizer,
        micro: *mut c_char,
        r#macro: *mut c_char,
        options: c_int,
    ) -> *mut Stdaddr;

    pub fn stdaddr_free(addr: *mut Stdaddr);

    /// Parse an address string via PAGC's `parseaddress()` path (PCRE2
    /// regex splitting on the trailing macro, zip, state, city). Defined
    /// in `src/pagc/wasm_parser.c` on top of the vendored
    /// `parseaddress-api.c`. Returns NULL only on OOM or state-hash
    /// bootstrap failure; regex misses still return an ADDRESS with
    /// NULL fields.
    pub fn pagc_parse_address(input: *const c_char) -> *mut Address;

    /// Free an ADDRESS returned by `pagc_parse_address`.
    pub fn pagc_address_free(addr: *mut Address);
}
