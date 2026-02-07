#![allow(unused_imports)]

#[cfg(all(feature = "wasm", feature = "reqwest"))]
compile_error!("feature \"wasm\" and feature \"reqwest\" cannot be enabled at the same time");

extern crate serde;
extern crate serde_json;
extern crate serde_repr;
extern crate url;

#[cfg(feature = "reqwest")]
extern crate reqwest;

#[cfg(feature = "wasm")]
extern crate gloo_utils;
#[cfg(feature = "wasm")]
extern crate wasm_bindgen;
#[cfg(feature = "wasm")]
extern crate wasm_bindgen_futures;
#[cfg(feature = "wasm")]
extern crate web_sys;

#[cfg(feature = "reqwest")]
pub mod apis;

#[cfg(feature = "wasm")]
pub mod wasm_apis;
#[cfg(feature = "wasm")]
pub use wasm_apis as apis;

pub mod models;
