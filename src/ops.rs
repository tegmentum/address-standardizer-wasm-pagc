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
    standardize_mm(addr, None)
}

/// Standardize with explicit micro+macro split, mirroring the PostGIS
/// `standardize_address(lex, gaz, rules, micro, macro)` 5-arg entry point.
/// `macro_addr` is the city/state/zip half; pass `None` for the 1-arg form.
pub fn standardize_mm(micro: &str, macro_addr: Option<&str>) -> Result<StandardizedAddress, String> {
    let std_ptr = get_standardizer()?;
    let c_micro = CString::new(micro).map_err(|e| format!("invalid micro: {e}"))?;
    let c_macro = match macro_addr {
        Some(m) => Some(CString::new(m).map_err(|e| format!("invalid macro: {e}"))?),
        None => None,
    };

    // PAGC's std_standardize_mm writes through both buffers; pass owned
    // heap copies via CString::into_raw and reclaim them after the call.
    let micro_raw = c_micro.into_raw();
    let macro_raw = c_macro.map(|c| c.into_raw()).unwrap_or(core::ptr::null_mut());
    let stdaddr_ptr =
        unsafe { ffi::std_standardize_mm(std_ptr, micro_raw, macro_raw, 0 as c_int) };
    // Reclaim the CStrings so they are dropped (PAGC does not free them).
    unsafe {
        let _ = CString::from_raw(micro_raw);
        if !macro_raw.is_null() {
            let _ = CString::from_raw(macro_raw);
        }
    }
    if stdaddr_ptr.is_null() {
        return Err("PAGC: std_standardize_mm returned NULL".to_string());
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

/// `parse_address` runs PAGC's PCRE2-driven regex splitter
/// (`parseaddress()` in the vendored `parseaddress-api.c`) rather than
/// the rules/lex standardizer. It extracts the trailing macro half of
/// the address (city / state / postcode / country) plus a leading
/// house-number, and leaves the street name intact rather than
/// normalising it against the lex tables.
///
/// The output is packed into the same `StandardizedAddress` shape as
/// `standardize()`. The PAGC `ADDRESS` fields map as:
///
///   ADDRESS.num       -> house_num
///   ADDRESS.street    -> name
///   ADDRESS.street2   -> extra
///   ADDRESS.city      -> city
///   ADDRESS.st        -> state
///   ADDRESS.zip[+zipplus] -> postcode
///   ADDRESS.cc        -> country
///
/// PAGC's parse path has no notion of building / predir / qual /
/// pretype / suftype / sufdir / ruralroute / box / unit — those fields
/// stay `None`. Callers that need the normalised street-name split
/// should still use `standardize()`.
pub fn parse(addr: &str) -> Result<StandardizedAddress, String> {
    let c_input = CString::new(addr).map_err(|e| format!("invalid address: {e}"))?;
    let addr_ptr = unsafe { ffi::pagc_parse_address(c_input.as_ptr()) };
    if addr_ptr.is_null() {
        return Err("PAGC: pagc_parse_address returned NULL".to_string());
    }

    let out = unsafe {
        let r = &*addr_ptr;
        let zip = cstr_field(r.zip);
        let zipplus = cstr_field(r.zipplus);
        let postcode = match (&zip, &zipplus) {
            (Some(z), Some(p)) => Some(format!("{z}-{p}")),
            (Some(z), None) => Some(z.clone()),
            (None, Some(p)) => Some(p.clone()),
            (None, None) => None,
        };
        // PAGC always tags the country as "US" unless it stripped an
        // explicit trailing token; leave the field populated as-is.
        let mut extra = cstr_field(r.street2);
        if let Some(a1) = cstr_field(r.address1) {
            // If parseaddress could not split num/street it drops the
            // whole cleaned input into address1. Surface it in `extra`
            // so callers still see something.
            if r.num.is_null() && r.street.is_null() && extra.is_none() {
                extra = Some(a1);
            }
        }
        StandardizedAddress {
            building: None,
            house_num: cstr_field(r.num),
            predir: None,
            qual: None,
            pretype: None,
            name: cstr_field(r.street),
            suftype: None,
            sufdir: None,
            ruralroute: None,
            extra,
            city: cstr_field(r.city),
            state: cstr_field(r.st),
            country: cstr_field(r.cc),
            postcode,
            r#box: None,
            unit: None,
        }
    };
    unsafe { ffi::pagc_address_free(addr_ptr) };
    Ok(out)
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
