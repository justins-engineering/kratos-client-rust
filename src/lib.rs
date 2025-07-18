#![allow(unused_imports)]
#![allow(clippy::too_many_arguments)]

extern crate serde;
extern crate serde_json;
extern crate url;

#[cfg(not(target_family = "wasm"))]
extern crate reqwest;

#[cfg(target_family = "wasm")]
extern crate gloo_utils;
extern crate wasm_bindgen;
extern crate wasm_bindgen_futures;
extern crate web_sys;

pub mod apis;
pub mod models;
