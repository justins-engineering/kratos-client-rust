//! Integration tests for `courier_api`, run with:
//!   cargo test --no-default-features --features reqwest --target x86_64-unknown-linux-gnu --test courier_api
//! against a live local Kratos (see tests/common/mod.rs for base URLs).
//!
//! Creating an identity via the admin API does NOT by itself trigger a
//! verification email (confirmed against real Kratos: waited 30s, nothing sent),
//! so this deliberately drives a native verification flow with the `code` method
//! to generate a real courier message to inspect.
//!
//! That submission is expected to fail client-side (see the KNOWN BUG comment
//! below -- the same missing-Accept-header issue documented in
//! tests/frontend_api.rs's browser-flow test, confirmed here to affect
//! update_verification_flow too, not just the create_browser_*_flow functions).
//! Confirmed directly against real Kratos with curl: the verification email is
//! still dispatched server-side even though the HTTP response Kratos sends back
//! is an unparseable redirect rather than JSON -- the side effect happens before
//! Kratos decides how to format the response. So this test proceeds past the
//! expected `Err` and still finds the resulting courier message, letting it
//! exercise list_courier_messages/get_courier_message with a real message.

#![cfg(feature = "reqwest")]

mod common;

use ory_kratos_client_wasm::apis::{courier_api, frontend_api, identity_api};
use ory_kratos_client_wasm::models;

#[tokio::test]
async fn list_and_get_courier_message() {
    let admin_cfg = common::admin_config();
    let public_cfg = common::public_config();
    let (identity, email, _password) = common::create_password_identity(&admin_cfg).await;

    let flow = frontend_api::create_native_verification_flow(&public_cfg, None)
        .await
        .expect("create_native_verification_flow should succeed");

    // KNOWN BUG (see the crate-level report and tests/frontend_api.rs): this is
    // expected to fail client-side because src/apis/frontend_api.rs never sends
    // `Accept: application/json`, and Kratos's verification-flow submission
    // endpoint 303-redirects instead of returning JSON when that header is absent
    // -- confirmed directly against real Kratos, and NOT specific to browser-type
    // flows (this is a native/`api`-type flow and still redirects). The server-side
    // action (queuing the verification email) still happens; only the client-level
    // response parsing fails.
    let submit_result = frontend_api::update_verification_flow(
        &public_cfg,
        &flow.id,
        models::UpdateVerificationFlowBody::Code(Box::new(
            models::UpdateVerificationFlowWithCodeMethod {
                email: Some(email.clone()),
                method: models::update_verification_flow_with_code_method::MethodEnum::Code,
                code: None,
                csrf_token: None,
                transient_payload: None,
            },
        )),
        None,
        None,
    )
    .await;
    assert!(
        submit_result.is_err(),
        "update_verification_flow currently cannot succeed over the reqwest feature \
     (see the KNOWN BUG comment above) -- if this assertion starts failing, the \
     missing Accept header was fixed and this test should be flipped to assert \
     success instead."
    );

    // Kratos dispatches the email asynchronously via the courier worker; poll
    // briefly rather than assuming it's instantaneous.
    let mut found = None;
    for _ in 0..20 {
        let messages =
            courier_api::list_courier_messages(&admin_cfg, None, None, None, Some(&email))
                .await
                .expect("list_courier_messages should succeed");
        if let Some(message) = messages.into_iter().next() {
            found = Some(message);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    let listed = found.expect(
        "expected a courier message for the verification code email despite the client-level error",
    );
    assert_eq!(listed.recipient, email);

    let fetched = courier_api::get_courier_message(&admin_cfg, &listed.id)
        .await
        .expect("get_courier_message should succeed");
    assert_eq!(fetched.id, listed.id);
    assert_eq!(fetched.recipient, email);

    identity_api::delete_identity(&admin_cfg, &identity.id)
        .await
        .expect("cleanup delete_identity should succeed");
}
