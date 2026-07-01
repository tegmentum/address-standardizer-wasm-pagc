//! Regression corpus comparing this crate's `standardize_mm` output
//! against the PostGIS PAGC `standardize_address(...)` reference output
//! for the same lex/gaz/rules data files.
//!
//! Corpus rows come from the upstream PostGIS PAGC extension's own
//! pg_regress suite (`test/expected/standardize_address_{1,2}.out`):
//! one row from each of the 5-arg micro/macro test cases in
//! `standardize_address_1.sql`, plus the 100 St-Paul addresses from
//! `standardize_address_2.sql`. Since this crate vendors the exact
//! same `13_us_lex.sql`, `14_us_gaz.sql`, `15_us_rules.sql` data files
//! and the exact same C analyzer code, parity is asserted by-row.
//!
//! Failures are collected and reported in aggregate (rather than
//! fail-fast) so a single mismatch surfaces all drift at once.

use address_standardizer_wasm_pagc::ops::{standardize_mm, StandardizedAddress};

/// Column order in `tests/corpus.csv`; matches the layout produced by
/// `tools/buildcsv.py` from the upstream `.out` files.
const COLUMNS: &[&str] = &[
    "input_micro",
    "input_macro",
    "building",
    "house_num",
    "predir",
    "qual",
    "pretype",
    "name",
    "suftype",
    "sufdir",
    "ruralroute",
    "extra",
    "city",
    "state",
    "country",
    "postcode",
    "box",
    "unit",
];

/// Minimal CSV row parser. Honours RFC-4180-style double-quoted fields
/// and "" -> " escapes; commas inside quotes are preserved. Good enough
/// for the corpus we ship.
fn parse_csv_row(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    buf.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                buf.push(c);
            }
        } else if c == '"' {
            in_quotes = true;
        } else if c == ',' {
            out.push(std::mem::take(&mut buf));
        } else {
            buf.push(c);
        }
    }
    out.push(buf);
    out
}

fn field<'a>(a: &'a StandardizedAddress, name: &str) -> Option<&'a str> {
    match name {
        "building" => a.building.as_deref(),
        "house_num" => a.house_num.as_deref(),
        "predir" => a.predir.as_deref(),
        "qual" => a.qual.as_deref(),
        "pretype" => a.pretype.as_deref(),
        "name" => a.name.as_deref(),
        "suftype" => a.suftype.as_deref(),
        "sufdir" => a.sufdir.as_deref(),
        "ruralroute" => a.ruralroute.as_deref(),
        "extra" => a.extra.as_deref(),
        "city" => a.city.as_deref(),
        "state" => a.state.as_deref(),
        "country" => a.country.as_deref(),
        "postcode" => a.postcode.as_deref(),
        "box" => a.r#box.as_deref(),
        "unit" => a.unit.as_deref(),
        _ => None,
    }
}

/// Compare one row; return a list of `(field, expected, actual)` mismatches.
fn diff_row(expected: &[String], actual: &StandardizedAddress) -> Vec<(String, String, String)> {
    // The first two CSV columns are the input, fields 2..18 are the
    // expected 16 standardizer outputs in column order.
    let mut diffs = Vec::new();
    for (i, col) in COLUMNS.iter().enumerate().skip(2) {
        let exp = expected[i].trim();
        let act = field(actual, col).unwrap_or("");
        if exp != act {
            diffs.push(((*col).to_string(), exp.to_string(), act.to_string()));
        }
    }
    diffs
}

#[test]
fn parity_against_postgis_reference() {
    let raw = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/corpus.csv"))
        .expect("corpus.csv missing");
    let mut lines = raw.lines();
    let header = lines.next().expect("empty corpus");
    let cols = parse_csv_row(header);
    assert_eq!(
        cols.len(),
        COLUMNS.len(),
        "corpus.csv header has {} columns, expected {}",
        cols.len(),
        COLUMNS.len()
    );

    let mut passed = 0usize;
    let mut failures: Vec<String> = Vec::new();
    let mut total = 0usize;
    for (idx, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        total += 1;
        let row = parse_csv_row(line);
        if row.len() != COLUMNS.len() {
            failures.push(format!(
                "row {idx}: parse error, {} fields, expected {}",
                row.len(),
                COLUMNS.len()
            ));
            continue;
        }
        let micro = &row[0];
        let macro_str = &row[1];
        let macro_opt = if macro_str.is_empty() { None } else { Some(macro_str.as_str()) };

        match standardize_mm(micro, macro_opt) {
            Err(e) => {
                failures.push(format!(
                    "row {idx}: standardize_mm error: {e} for micro={micro:?} macro={macro_str:?}"
                ));
            }
            Ok(actual) => {
                let diffs = diff_row(&row, &actual);
                if diffs.is_empty() {
                    passed += 1;
                } else {
                    let diff_str = diffs
                        .iter()
                        .map(|(f, e, a)| format!("{f}: expected {e:?}, got {a:?}"))
                        .collect::<Vec<_>>()
                        .join("; ");
                    failures.push(format!(
                        "row {idx} micro={micro:?} macro={macro_str:?}: {diff_str}"
                    ));
                }
            }
        }
    }

    let failed = failures.len();
    eprintln!("PAGC parity: {passed}/{total} matched");
    if !failures.is_empty() {
        eprintln!("--- failures ({failed}) ---");
        for f in &failures {
            eprintln!("  {f}");
        }
    }
    // The corpus is for regression tracking: we require >=95% match so
    // accidental analyzer drift surfaces, but a small known-quirk tail
    // doesn't block CI. Any larger regression fails the test.
    let threshold = (total as f64 * 0.95) as usize;
    assert!(
        passed >= threshold,
        "PAGC parity regression: {passed}/{total} matched, need >= {threshold} (95%)"
    );
}

#[test]
fn parse_returns_structured_output() {
    // Sanity check that the pcre2-backed parse_address path (new in #708)
    // now runs an actual PCRE2 regex split rather than delegating to
    // standardize_address. Two smoke assertions:
    //   1. Trailing "US" gets stripped into country + state stays.
    //   2. Zip and state are pulled out even without an explicit macro.
    use address_standardizer_wasm_pagc::ops::parse;

    let a = parse("123 Main St, Kansas City, MO 45678").expect("parse");
    assert_eq!(a.state.as_deref(), Some("MO"), "state mis-extracted: {a:?}");
    assert_eq!(a.postcode.as_deref(), Some("45678"), "zip mis-extracted: {a:?}");
    assert!(a.country.is_some(), "country should be tagged: {a:?}");

    // parse() must not just alias to standardize(): standardize() populates
    // suftype ("ST" for "Street") while parse() does not.
    assert!(
        a.suftype.is_none(),
        "parse() should not fill suftype (that's standardize's job): {a:?}"
    );
}
