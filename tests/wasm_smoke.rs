//! Smoke test for the `wasm` feature (browser Fetch API), run with:
//!   CHROMEDRIVER=/usr/bin/chromedriver \
//!   CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
//!   cargo test --target wasm32-unknown-unknown --test wasm_smoke
//! against a live local Kratos (see tests/common/mod.rs's env vars, though this
//! file can't reuse that module directly -- see the note below).
//!
//! Why this exists as a separate, narrower thing from tests/*_api.rs: those run
//! over the `reqwest` feature, and reqwest is one of THREE separate,
//! independently hand-maintained implementations of the same API surface (see
//! src/apis/ vs src/wasm_apis/ vs src/worker_apis/ -- confirmed in the task-1
//! architecture trace that these are not a shared-logic thin-transport-swap, so
//! a bug can live in one tree and not the others; the window().unwrap() bug
//! fixed in task 1 is a real example -- present in wasm_apis's
//! identity_api.rs/metadata_api.rs but not courier_api.rs/frontend_api.rs, and
//! not applicable to worker_apis/apis at all). reqwest-feature tests give zero
//! coverage of wasm_apis's own request-building/response-parsing code. This
//! file is a narrow smoke test (not full API-surface coverage, unlike the
//! reqwest tests) proving wasm_apis can build a real request and parse a real
//! Kratos response through an actual browser Fetch call.
//!
//! Setup notes for whoever runs/maintains this:
//! - Requires wasm-bindgen-cli installed at the EXACT version pinned in
//!   Cargo.lock for `wasm-bindgen` (confirmed here via
//!   `cargo install wasm-bindgen-cli --version <that version> --locked`) --
//!   wasm-bindgen-test-runner and the compiled test .wasm must agree on the
//!   wasm-bindgen ABI or this fails with an opaque version-mismatch panic.
//! - Requires a browser + matching webdriver on PATH or via CHROMEDRIVER/
//!   GECKODRIVER env vars (confirmed working here with system chromium +
//!   chromedriver, both already present in this environment).
//! - Deliberately NOT wired into `.cargo/config.toml`'s `[target.*] runner`
//!   (which is already pinned to wasm32-unknown-unknown as the *build* default
//!   for this repo) -- the runner is passed via
//!   CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER on the command line instead, so
//!   a bare `cargo test`/`cargo build` in this repo doesn't suddenly require
//!   chromedriver to be present. See the crate-level report for why this is an
//!   opt-in invocation rather than a config default.
//! - CURRENTLY `#[ignore]`D: this fails with "TypeError: Failed to fetch" against
//!   this local Kratos, and it's a real, confirmed CORS rejection, not a crate
//!   bug -- wasm-bindgen-test-runner serves the test page from a browser-chosen
//!   ephemeral port (e.g. http://127.0.0.1:43023) with no way to pin it (checked
//!   `wasm-bindgen-test-runner --help`: no address/port flag), and that origin
//!   isn't on Kratos's `serve.public.cors.allowed_origins` list
//!   (schemas/kratos/kratos.yml in the PidgeIoT repo: only
//!   http://127.0.0.1:4433, http://127.0.0.1:4455, and the bare/no-port
//!   http://127.0.0.1 and http://localhost, which do NOT wildcard-match a
//!   browser Origin header that always includes a port). Confirmed precisely
//!   with a CORS preflight probe: `curl -X OPTIONS .../health/alive -H "Origin:
//!   http://127.0.0.1:43023" ...` gets a 204 with no
//!   `Access-Control-Allow-Origin` header at all, while the same probe with
//!   `Origin: http://127.0.0.1:4455` gets one back correctly. So this is Kratos
//!   correctly doing its job, not a broken test -- it's exactly the same
//!   rejection a real browser page on an unlisted origin would get. Un-ignore
//!   this once there's a way to either pin the test runner's port to something
//!   on the allow-list, or add a permissive local-dev CORS entry to
//!   kratos.yml for this purpose (a PidgeIoT-repo decision, not this crate's).
//! - Base URL is a literal (not env-configurable via tests/common/mod.rs's
//!   pattern): this file can't `mod common;` and share that module with the
//!   reqwest-feature tests, because tests/common/mod.rs imports
//!   `apis::identity_api`/`apis::configuration`, which resolve to a DIFFERENT
//!   module (src/apis/ vs src/wasm_apis/) depending on which feature is
//!   active -- under the `wasm` feature (this file's feature), `apis` is
//!   `wasm_apis` (see src/lib.rs's `pub use wasm_apis as apis`), so reusing
//!   tests/common/mod.rs here would silently compile against a completely
//!   different, incompatible `Configuration`/`Error` type than the one
//!   tests/identity_api.rs etc. use. Kept deliberately separate and minimal
//!   rather than trying to share fixture code across two API surfaces that
//!   only coincidentally share a module path.

#![cfg(feature = "wasm")]

use ory_kratos_client_wasm::apis::configuration::Configuration;
use ory_kratos_client_wasm::apis::metadata_api;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

fn public_config() -> Configuration {
    Configuration {
        base_path: "http://127.0.0.1:4433".to_owned(),
        ..Configuration::default()
    }
}

#[wasm_bindgen_test::wasm_bindgen_test]
#[ignore = "blocked on Kratos CORS allow-listing the test runner's ephemeral origin -- see module doc comment"]
async fn is_alive_reports_ok_over_browser_fetch() {
    let cfg = public_config();

    let status = metadata_api::is_alive(&cfg)
        .await
        .expect("is_alive should succeed against a live Kratos over the browser Fetch API");

    assert_eq!(status.status, "ok");
}
