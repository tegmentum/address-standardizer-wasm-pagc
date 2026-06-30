//! Address standardization as a WebAssembly component, backed by the
//! vendored PAGC C engine. The native (non-wasm) build skips the WIT
//! bindings and just exposes `ops` for unit testing.

#[cfg(target_family = "wasm")]
mod bindings;

pub mod ffi;
pub mod ops;

#[cfg(target_family = "wasm")]
mod wit;
