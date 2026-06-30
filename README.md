# address-standardizer-wasm-pagc

US address standardization/parsing as a [WebAssembly component](https://component-model.bytecodealliance.org/),
backed by the vendored [PAGC](http://www.pagcgeo.org/) C engine — the same
parser PostGIS's [`address_standardizer`](https://postgis.net/docs/Address_Standardizer.html)
extension uses, compiled to `wasm32-wasip2` via [wasi-sdk](https://github.com/WebAssembly/wasi-sdk).

Sibling of [`address-standardizer-wasm`](https://github.com/tegmentum/address-standardizer-wasm),
which uses a Rust CRF (`usaddress`) engine. The two components export
the same `address-standardizer` interface shape, so callers can swap
engines without changing record types — pick the CRF MVP for coverage,
or this one when bit-identical PostGIS parity matters.

## Status

Initial scaffold. The PAGC C core is vendored and wired through to a
Rust component glue layer via FFI. The bundled US lex/gaz/rules tables
ship as `include_bytes!` constants over the SQL data files PostGIS uses.

What works:

- Vendored PAGC C sources compile against a minimal `postgres.h` shim
  (`src/pagc/postgres.h`), no real PG server required.
- The wasm component is built via `cargo component` driving the `cc`
  crate against `wasi-sdk` clang/sysroot.
- `wasm_loader.c` parses the embedded `13_us_lex.sql` /
  `14_us_gaz.sql` / `15_us_rules.sql` VALUES tuples directly — no SQL
  engine, no PCRE.

What is **not yet wired**:

- `parse_address` currently delegates to `standardize_address`. PAGC's
  separate parse path (`address_parser.c`) depends on PCRE2 which has
  not been built for wasi-sdk in this repo yet.
- No regression test corpus comparing wasm output to PostGIS output
  on a fixed input set.

## Build

```sh
# wasi-sdk-28 or newer expected at ~/.wasi-sdk
./scripts/build.sh
```

Artifact: `target/wasm32-wasip2/release/address_standardizer_wasm_pagc.wasm`.

## Interface

See `wit/world.wit`. The `standardized-address` record carries PAGC's
full STDADDR field set (16 components), a superset of the sibling MVP
component's 10-field record.

## Licensing

PAGC sources are vendored from
[github.com/postgis/address_standardizer](https://github.com/postgis/address_standardizer)
under their original MIT license; see `COPYING.PAGC`. The Rust glue is
MIT-licensed.
