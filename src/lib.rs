#![allow(unused_imports)]

#[cfg(all(feature = "wasm", feature = "reqwest"))]
compile_error!("feature \"wasm\" and feature \"reqwest\" cannot be enabled at the same time");

extern crate serde;
extern crate serde_json;
extern crate url;

#[cfg(any(not(target_family = "wasm"), feature = "reqwest"))]
extern crate reqwest;

#[cfg(all(target_family = "wasm", feature = "wasm"))]
extern crate gloo_utils;
#[cfg(all(target_family = "wasm", feature = "wasm"))]
extern crate wasm_bindgen;
#[cfg(all(target_family = "wasm", feature = "wasm"))]
extern crate wasm_bindgen_futures;
#[cfg(all(target_family = "wasm", feature = "wasm"))]
extern crate web_sys;

pub mod apis;
pub mod models;
