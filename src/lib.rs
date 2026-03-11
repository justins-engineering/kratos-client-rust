#![allow(unused_imports)]

#[cfg(all(feature = "wasm", feature = "reqwest"))]
compile_error!("feature \"wasm\" and feature \"reqwest\" cannot be enabled at the same time");

extern crate serde;
extern crate serde_json;
extern crate serde_repr;
extern crate url;

#[cfg(feature = "reqwest")]
extern crate reqwest;

#[cfg(any(feature = "wasm", feature = "worker"))]
extern crate gloo_utils;
#[cfg(any(feature = "wasm", feature = "worker"))]
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

#[cfg(feature = "worker")]
pub mod worker_apis;
#[cfg(feature = "worker")]
pub use worker_apis as apis;

pub mod models;
