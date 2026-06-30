//! Safe Rust wrapper over PAGC's standardize API. The wasm `Guest`
//! impl in `wit.rs` delegates here; this module is native-testable.
//!
//! At first call we lazily build the `STANDARDIZER` from the bundled
//! `data/13_us_lex.sql`, `data/14_us_gaz.sql` and `data/15_us_rules.sql`
//! files (embedded via `include_bytes!`) and keep it alive for the
//! lifetime of the component instance. PAGC's `STANDARDIZER` is
//! initialised once and reused across calls; PAGC itself maintains a
//! per-call scratch arena.

use core::ffi::{c_char, c_int};
use std::ffi::CString;
use std::sync::OnceLock;

use crate::ffi;

/// Bundled PAGC lex table (US English lexicon).
const LEX_SQL: &[u8] = include_bytes!("../data/13_us_lex.sql");
/// Bundled PAGC gazetteer (US place/state names).
const GAZ_SQL: &[u8] = include_bytes!("../data/14_us_gaz.sql");
/// Bundled PAGC parse rules.
const RULES_SQL: &[u8] = include_bytes!("../data/15_us_rules.sql");

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StandardizedAddress {
    pub building: Option<String>,
    pub house_num: Option<String>,
    pub predir: Option<String>,
    pub qual: Option<String>,
    pub pretype: Option<String>,
    pub name: Option<String>,
    pub suftype: Option<String>,
    pub sufdir: Option<String>,
    pub ruralroute: Option<String>,
    pub extra: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub country: Option<String>,
    pub postcode: Option<String>,
    pub r#box: Option<String>,
    pub unit: Option<String>,
}

/// The built PAGC standardizer. We wrap the raw pointer so it can sit in
/// a `OnceLock<&'static StandardizerHandle>`; PAGC is not thread-safe for
/// concurrent standardize calls, so callers that need concurrency must
/// synchronise externally. Wasm components are single-threaded so this is
/// fine in practice.
struct StandardizerHandle(*mut ffi::Standardizer);

// Safety: the WASM component-model instance is single-threaded; PAGC's
// per-call state lives inside the STANDARDIZER's misc_stand which is
// reused but not crossed-by-threads here.
unsafe impl Send for StandardizerHandle {}
unsafe impl Sync for StandardizerHandle {}

static STANDARDIZER: OnceLock<StandardizerHandle> = OnceLock::new();

fn get_standardizer() -> Result<*mut ffi::Standardizer, String> {
    let handle = STANDARDIZER.get_or_init(|| {
        let ptr = unsafe {
            ffi::pagc_build_standardizer(
                LEX_SQL.as_ptr() as *const c_char,
                LEX_SQL.len(),
                GAZ_SQL.as_ptr() as *const c_char,
                GAZ_SQL.len(),
                RULES_SQL.as_ptr() as *const c_char,
                RULES_SQL.len(),
            )
        };
        StandardizerHandle(ptr)
    });
    if handle.0.is_null() {
        Err("PAGC: failed to build standardizer from bundled lex/gaz/rules".to_string())
    } else {
        Ok(handle.0)
    }
}

/// Convert a `*mut c_char` (PAGC-owned) to an `Option<String>`. NULL or
/// empty strings become `None`.
unsafe fn cstr_field(p: *mut c_char) -> Option<String> {
    if p.is_null() {
        return None;
    }
    let s = std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

pub fn standardize(addr: &str) -> Result<StandardizedAddress, String> {
    let std_ptr = get_standardizer()?;
    let c_addr = CString::new(addr).map_err(|e| format!("invalid address: {e}"))?;

    // PAGC's std_standardize_mm takes micro + macro buffers. We pass the
    // full input as micro and NULL macro; PAGC's analyzer treats macro
    // (city/state/postcode) as optional and parses everything from micro
    // when absent. PAGC writes through these buffers, so they must be
    // mutable heap allocations.
    let stdaddr_ptr =
        unsafe { ffi::std_standardize_mm(std_ptr, c_addr.into_raw(), core::ptr::null_mut(), 0 as c_int) };
    if stdaddr_ptr.is_null() {
        return Err("PAGC: std_standardize_one returned NULL".to_string());
    }

    let out = unsafe {
        let r = &*stdaddr_ptr;
        StandardizedAddress {
            building: cstr_field(r.building),
            house_num: cstr_field(r.house_num),
            predir: cstr_field(r.predir),
            qual: cstr_field(r.qual),
            pretype: cstr_field(r.pretype),
            name: cstr_field(r.name),
            suftype: cstr_field(r.suftype),
            sufdir: cstr_field(r.sufdir),
            ruralroute: cstr_field(r.ruralroute),
            extra: cstr_field(r.extra),
            city: cstr_field(r.city),
            state: cstr_field(r.state),
            country: cstr_field(r.country),
            postcode: cstr_field(r.postcode),
            r#box: cstr_field(r.r#box),
            unit: cstr_field(r.unit),
        }
    };
    unsafe { ffi::stdaddr_free(stdaddr_ptr) };
    Ok(out)
}

/// `parse_address` shares the standardize path; PAGC's separate parse
/// route uses PCRE2 which is not yet wired in for wasm.
pub fn parse(addr: &str) -> Result<StandardizedAddress, String> {
    standardize(addr)
}

/// Single-line render of a `StandardizedAddress`. Mirrors what PostGIS
/// gets back via the SRF columns: house_num + predir + name + suftype +
/// sufdir + ", " + unit + ", " + city + " " + state + " " + postcode.
pub fn as_text(a: &StandardizedAddress) -> String {
    let mut parts: Vec<String> = Vec::new();
    let line1: Vec<&str> = [
        a.house_num.as_deref(),
        a.predir.as_deref(),
        a.pretype.as_deref(),
        a.name.as_deref(),
        a.suftype.as_deref(),
        a.sufdir.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !line1.is_empty() {
        parts.push(line1.join(" "));
    }
    if let Some(u) = &a.unit {
        parts.push(u.clone());
    }
    let line3: Vec<&str> = [a.city.as_deref(), a.state.as_deref(), a.postcode.as_deref()]
        .into_iter()
        .flatten()
        .collect();
    if !line3.is_empty() {
        parts.push(line3.join(" "));
    }
    parts.join(", ")
}
