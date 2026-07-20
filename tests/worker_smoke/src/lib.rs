//! Smoke test fixture for the `worker` feature (Cloudflare workers-rs Fetch API).
//!
//! This is a throwaway Worker, NOT wired into `cargo test` -- workers-rs's
//! `worker::Fetch` bindings only exist inside an actual Workers/V8 isolate
//! runtime, so there's no way to exercise them from a plain `cargo test`
//! process the way tests/*_api.rs do for the `reqwest` feature, or even the way
//! tests/wasm_smoke.rs does for the `wasm` feature (that one at least runs
//! inside a real browser via wasm-bindgen-test-runner). This crate instead gets
//! run as a real local Worker via `wrangler dev --local`, and its response
//! polled with curl -- see the README in this directory for the exact steps.
//!
//! Why this exists at all, given tests/wasm_smoke.rs already proved the wasm
//! feature's Fetch path with a real Kratos response: src/apis/, src/wasm_apis/,
//! and src/worker_apis/ are three separate, independently hand-maintained
//! implementations of the same API surface (see the task-1 architecture trace)
//! -- reqwest-feature and wasm-feature tests give zero coverage of
//! worker_apis's own request-building/response-parsing code, and a bug (like
//! the window().unwrap() one fixed in task 1, present in some trees and not
//! others) could live there and nowhere else.
//!
//! Result as of this writing: confirmed working end-to-end against the live
//! local Kratos, unlike tests/wasm_smoke.rs -- CORS (which blocked the browser
//! Fetch smoke test) doesn't apply here, since CORS is a browser security
//! policy, not something the Workers runtime's own outbound `fetch` enforces.

use ory_kratos_client_wasm::apis::configuration::Configuration;
use ory_kratos_client_wasm::apis::metadata_api;
use worker::*;

#[event(fetch)]
async fn main(_req: Request, _env: Env, _ctx: Context) -> Result<Response> {
  let cfg = Configuration {
    base_path: "http://127.0.0.1:4433".to_owned(),
    ..Configuration::default()
  };

  match metadata_api::is_alive(&cfg).await {
    Ok(status) if status.status == "ok" => {
      Response::ok(format!("PASS: is_alive returned status={:?}", status.status))
    }
    Ok(status) => Response::error(
      format!("FAIL: is_alive returned unexpected status={:?}", status.status),
      500,
    ),
    Err(e) => Response::error(format!("FAIL: is_alive errored: {e:?}"), 500),
  }
}
