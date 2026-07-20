//! Integration tests for `metadata_api`, run with:
//!   cargo test --no-default-features --features reqwest --test metadata_api
//! against a live local Kratos (see tests/common/mod.rs for base URLs).

#![cfg(feature = "reqwest")]

mod common;

use ory_kratos_client_wasm::apis::metadata_api;

#[tokio::test]
async fn get_version_returns_a_version_string() {
    // Kratos only serves /version on the admin API (307 -> /admin/version);
    // the public API 404s. reqwest follows the redirect by default.
    let cfg = common::admin_config();

    let version = metadata_api::get_version(&cfg)
        .await
        .expect("get_version should succeed against a live Kratos admin API");

    assert!(
        !version.version.is_empty(),
        "expected a non-empty version string"
    );
}

#[tokio::test]
async fn is_alive_reports_ok() {
    let cfg = common::public_config();

    let status = metadata_api::is_alive(&cfg)
        .await
        .expect("is_alive should succeed against a live Kratos");

    assert_eq!(status.status, "ok");
}

#[tokio::test]
async fn is_ready_reports_ok() {
    let cfg = common::public_config();

    let status = metadata_api::is_ready(&cfg)
        .await
        .expect("is_ready should succeed against a live Kratos (DB must be reachable)");

    assert_eq!(status.status, "ok");
}
