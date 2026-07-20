//! Integration tests for `frontend_api` (native login/registration flows, sessions),
//! run with:
//!   cargo test --no-default-features --features reqwest --target x86_64-unknown-linux-gnu --test frontend_api
//! against a live local Kratos (see tests/common/mod.rs for base URLs).
//!
//! Scope notes (see the crate-level report for the reasoning):
//! - Registration is intentionally NOT completion-tested here. This Kratos instance
//!   uses v25.4's default two-step registration (profile traits, then password) —
//!   see schemas/kratos/kratos.yml's comment on the `registration` flow in the
//!   PidgeIoT repo and task #16 there. A native/JSON client has to submit `profile`
//!   first, read the 400 response for the next step's nodes, then submit `password`
//!   against the *same* flow id — a second real behavior to pin down, not just a
//!   client wiring problem. Login has no such two-step wrinkle (confirmed against
//!   real Kratos: a single `password` method submission with identifier+password
//!   succeeds directly), so the session-lifecycle tests below use an identity
//!   created directly via the admin API (tests/common/mod.rs) and log it in natively,
//!   which exercises the same update_login_flow/to_session/session-management
//!   surface without depending on the two-step registration behavior.
//! - create_fedcm_flow/update_fedcm_flow are browser/FedCM-API-specific and not
//!   practically exercisable from a plain HTTP client; not tested.
//! - exchange_session_token needs a `return_to_code` that's only minted mid a
//!   browser OIDC/return_to redirect; not practically exercisable here; not tested.
//! - The create_browser_*_flow functions currently CANNOT succeed at all over the
//!   reqwest feature (confirmed bug, see the KNOWN BUG comment in
//!   browser_flow_creation_currently_fails_via_reqwest below): src/apis/frontend_api.rs
//!   never sends `Accept: application/json` on any request (grep count: 0, vs. 26
//!   in each of wasm_apis/worker_apis, which set it on every request), and Kratos
//!   treats these specific GET endpoints as a real browser navigation without that
//!   header, 303-redirecting to the frontend's login UI instead of returning the
//!   flow as JSON. This is NOT a blanket "every endpoint breaks without the
//!   header" problem, though -- confirmed narrowly with curl: login, settings,
//!   recovery, and registration submissions all return JSON fine with no Accept
//!   header; only the six create_browser_*_flow functions and, separately,
//!   update_verification_flow (see tests/courier_api.rs) are affected. This also
//!   means completing a browser flow (which needs cookie continuity across the GET
//!   and POST, and this crate's reqwest feature has no `cookies` feature/jar
//!   configured -- see Cargo.toml) isn't practically reachable here regardless.

#![cfg(feature = "reqwest")]

mod common;

use ory_kratos_client_wasm::apis::{frontend_api, identity_api};
use ory_kratos_client_wasm::models;

/// Logs the given identity in via a native login flow and returns the session token
/// plus the session id.
async fn login(
    public_cfg: &ory_kratos_client_wasm::apis::configuration::Configuration,
    email: &str,
    password: &str,
) -> (String, String) {
    let flow = frontend_api::create_native_login_flow(
        public_cfg, None, None, None, None, None, None, None, None,
    )
    .await
    .expect("create_native_login_flow should succeed");

    let body = models::UpdateLoginFlowBody::Password(Box::new(
        models::UpdateLoginFlowWithPasswordMethod {
            identifier: email.to_owned(),
            method: "password".to_owned(),
            password: password.to_owned(),
            csrf_token: None,
            password_identifier: None,
            transient_payload: None,
        },
    ));

    let login_result = frontend_api::update_login_flow(public_cfg, &flow.id, body, None, None)
        .await
        .expect("update_login_flow should succeed with a valid identifier/password");

    let session_token = login_result
        .session_token
        .expect("native login should return a session_token");
    let session_id = login_result.session.id.clone();
    (session_token, session_id)
}

#[tokio::test]
async fn native_login_flow_creation_and_lookup() {
    let public_cfg = common::public_config();

    let flow = frontend_api::create_native_login_flow(
        &public_cfg,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .expect("create_native_login_flow should succeed");
    assert_eq!(flow.r#type, "api");

    let fetched = frontend_api::get_login_flow(&public_cfg, &flow.id, None)
        .await
        .expect("get_login_flow should succeed");
    assert_eq!(fetched.id, flow.id);
}

#[tokio::test]
async fn native_registration_flow_creation_and_lookup() {
    let public_cfg = common::public_config();

    let flow = frontend_api::create_native_registration_flow(&public_cfg, None, None, None, None)
        .await
        .expect("create_native_registration_flow should succeed");

    let fetched = frontend_api::get_registration_flow(&public_cfg, &flow.id, None)
        .await
        .expect("get_registration_flow should succeed");
    assert_eq!(fetched.id, flow.id);
}

#[tokio::test]
async fn login_then_to_session_then_logout() {
    let admin_cfg = common::admin_config();
    let public_cfg = common::public_config();
    let (identity, email, password) = common::create_password_identity(&admin_cfg).await;

    let (session_token, _session_id) = login(&public_cfg, &email, &password).await;

    let whoami = frontend_api::to_session(&public_cfg, Some(&session_token), None, None)
        .await
        .expect("to_session should succeed with a valid session token");
    assert_eq!(
        whoami.identity.expect("whoami should include identity").id,
        identity.id
    );
    assert_eq!(whoami.active, Some(true));

    frontend_api::perform_native_logout(
        &public_cfg,
        models::PerformNativeLogoutBody {
            session_token: session_token.clone(),
        },
    )
    .await
    .expect("perform_native_logout should succeed");

    let after_logout =
        frontend_api::to_session(&public_cfg, Some(&session_token), None, None).await;
    assert!(
        after_logout.is_err(),
        "expected to_session to fail once the session token has been logged out"
    );

    identity_api::delete_identity(&admin_cfg, &identity.id)
        .await
        .expect("cleanup delete_identity should succeed");
}

#[tokio::test]
async fn list_my_sessions_and_disable_my_other_sessions() {
    let admin_cfg = common::admin_config();
    let public_cfg = common::public_config();
    let (identity, email, password) = common::create_password_identity(&admin_cfg).await;

    let (session_token_a, _) = login(&public_cfg, &email, &password).await;
    let (session_token_b, _) = login(&public_cfg, &email, &password).await;

    // Despite the name, Kratos's own docstring on this endpoint says it returns all
    // *other* active sessions, excluding the one making the request (confirmed
    // against real Kratos: calling with session_token_a returns only session B, not
    // both) -- whoami (to_session) is how you get the calling session itself.
    let sessions = frontend_api::list_my_sessions(
        &public_cfg,
        None,
        None,
        None,
        None,
        Some(&session_token_a),
        None,
    )
    .await
    .expect("list_my_sessions should succeed");
    assert_eq!(
        sessions.len(),
        1,
        "expected exactly session B (list_my_sessions excludes the calling session)"
    );

    let revoked =
        frontend_api::disable_my_other_sessions(&public_cfg, Some(&session_token_a), None)
            .await
            .expect("disable_my_other_sessions should succeed");
    assert_eq!(
        revoked.count,
        Some(1),
        "expected exactly session B to be revoked"
    );

    // session_token_a (the one we called disable_my_other_sessions *with*) should
    // still work; session_token_b should now be dead.
    frontend_api::to_session(&public_cfg, Some(&session_token_a), None, None)
        .await
        .expect("the calling session should survive disable_my_other_sessions");
    assert!(
        frontend_api::to_session(&public_cfg, Some(&session_token_b), None, None)
            .await
            .is_err(),
        "the other session should be revoked after disable_my_other_sessions"
    );

    identity_api::delete_identity(&admin_cfg, &identity.id)
        .await
        .expect("cleanup delete_identity should succeed");
}

#[tokio::test]
async fn admin_session_management_via_identity_api() {
    let admin_cfg = common::admin_config();
    let public_cfg = common::public_config();
    let (identity, email, password) = common::create_password_identity(&admin_cfg).await;

    let (_session_token, session_id) = login(&public_cfg, &email, &password).await;

    // get_session (admin)
    let session = identity_api::get_session(&admin_cfg, &session_id, None)
        .await
        .expect("get_session should succeed");
    assert_eq!(session.id, session_id);

    // extend_session (admin)
    let extended = identity_api::extend_session(&admin_cfg, &session_id)
        .await
        .expect("extend_session should succeed");
    assert_eq!(extended.id, session_id);

    // list_identity_sessions (admin)
    let identity_sessions = identity_api::list_identity_sessions(
        &admin_cfg,
        &identity.id,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .expect("list_identity_sessions should succeed");
    assert!(identity_sessions.iter().any(|s| s.id == session_id));

    // list_sessions (admin, global)
    let all_sessions = identity_api::list_sessions(&admin_cfg, None, None, Some(true), None)
        .await
        .expect("list_sessions should succeed");
    assert!(all_sessions.iter().any(|s| s.id == session_id));

    // disable_session (admin)
    identity_api::disable_session(&admin_cfg, &session_id)
        .await
        .expect("disable_session should succeed");
    let disabled = identity_api::get_session(&admin_cfg, &session_id, None)
        .await
        .expect("get_session should still succeed for a disabled session");
    assert_eq!(disabled.active, Some(false));

    // delete_identity_sessions (admin) - log in again first so there's a session to revoke
    let (_second_token, _second_session_id) = login(&public_cfg, &email, &password).await;
    identity_api::delete_identity_sessions(&admin_cfg, &identity.id)
        .await
        .expect("delete_identity_sessions should succeed");
    let remaining = identity_api::list_identity_sessions(
        &admin_cfg,
        &identity.id,
        None,
        None,
        None,
        None,
        Some(true),
    )
    .await
    .expect("list_identity_sessions should succeed");
    assert!(
        remaining.is_empty(),
        "expected no active sessions left after delete_identity_sessions"
    );

    identity_api::delete_identity(&admin_cfg, &identity.id)
        .await
        .expect("cleanup delete_identity should succeed");
}

#[tokio::test]
async fn native_settings_flow_creation_and_password_update() {
    let admin_cfg = common::admin_config();
    let public_cfg = common::public_config();
    let (identity, email, password) = common::create_password_identity(&admin_cfg).await;
    let (session_token, _session_id) = login(&public_cfg, &email, &password).await;

    let flow = frontend_api::create_native_settings_flow(&public_cfg, Some(&session_token))
        .await
        .expect("create_native_settings_flow should succeed");

    let fetched =
        frontend_api::get_settings_flow(&public_cfg, &flow.id, Some(&session_token), None)
            .await
            .expect("get_settings_flow should succeed");
    assert_eq!(fetched.id, flow.id);

    let new_password = "another-correct-horse-2".to_owned();
    let body = models::UpdateSettingsFlowBody::Password(Box::new(
        models::UpdateSettingsFlowWithPasswordMethod {
            password: new_password.clone(),
            method: "password".to_owned(),
            csrf_token: None,
            transient_payload: None,
        },
    ));
    frontend_api::update_settings_flow(&public_cfg, &flow.id, body, Some(&session_token), None)
        .await
        .expect("update_settings_flow should succeed changing the password");

    // Old password should no longer work; new one should.
    let old_flow = frontend_api::create_native_login_flow(
        &public_cfg,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .expect("create_native_login_flow should succeed");
    let old_login = frontend_api::update_login_flow(
        &public_cfg,
        &old_flow.id,
        models::UpdateLoginFlowBody::Password(Box::new(
            models::UpdateLoginFlowWithPasswordMethod {
                identifier: email.clone(),
                method: "password".to_owned(),
                password: password.clone(),
                csrf_token: None,
                password_identifier: None,
                transient_payload: None,
            },
        )),
        None,
        None,
    )
    .await;
    assert!(old_login.is_err(), "the old password should no longer work");

    let (_new_token, _) = login(&public_cfg, &email, &new_password).await;

    identity_api::delete_identity(&admin_cfg, &identity.id)
        .await
        .expect("cleanup delete_identity should succeed");
}

#[tokio::test]
async fn browser_flow_creation_currently_fails_via_reqwest() {
    // CONFIRMED BUG (see the crate-level report): every create_browser_*_flow
    // function in this feature's `src/apis/frontend_api.rs` (the openapi-generator
    // autogen tree -- NOT hand-edited by this test change) never sends
    // `Accept: application/json` (confirmed: `grep -c '"Accept"' src/apis/frontend_api.rs`
    // is 0, vs. 26 in each of wasm_apis/frontend_api.rs and worker_apis/frontend_api.rs,
    // which both set it explicitly on every request). Kratos's browser-flow endpoints
    // do Accept-header content negotiation: with no Accept header, `GET
    // /self-service/login/browser` 303-redirects to the frontend's login UI
    // (`ui_url`/`ROOT_URL`-equivalent) instead of returning the flow as JSON, and a
    // plain reqwest client follows that redirect by default and lands on an HTML
    // page with no Content-Type header at all -- confirmed directly with curl
    // against this same local Kratos. So `create_browser_login_flow` here
    // deserializes garbage and fails, not because of a network/auth problem but
    // because of the missing header, and completing any browser flow purely over
    // this crate's reqwest feature is not currently practical. wasm_apis/worker_apis
    // are unaffected (they set the header). Document the failure instead of
    // asserting success until src/apis/ is regenerated with the header included.
    let public_cfg = common::public_config();

    let login_flow = frontend_api::create_browser_login_flow(
        &public_cfg,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await;
    assert!(
        login_flow.is_err(),
        "create_browser_login_flow currently cannot succeed over the reqwest feature \
     (see the KNOWN BUG comment above) -- if this assertion starts failing, the \
     missing Accept header was fixed and this test should be flipped to assert \
     success (type == \"browser\") instead."
    );

    let logout_flow = frontend_api::create_browser_logout_flow(&public_cfg, None, None).await;
    // No cookie/session presented, so Kratos correctly rejects this the same way it
    // would refuse to log out a browser that was never logged in.
    assert!(
        logout_flow.is_err(),
        "create_browser_logout_flow with no session cookie should fail"
    );
}

#[tokio::test]
async fn get_web_authn_java_script_returns_javascript() {
    let public_cfg = common::public_config();

    let result = frontend_api::get_web_authn_java_script(&public_cfg).await;

    // KNOWN BUG (see the crate-level report): get_web_authn_java_script's return type
    // is `String`, but the response-handling tail only ever maps `ContentType::Json`
    // to a successful `String` result — `ContentType::Text` and `::Unsupported`
    // (which is what `text/javascript`/`application/javascript` fall into, since
    // ContentType::from only special-cases "text/plain" as Text) both hit the
    // `Err(...)` arms instead. So this call *should* succeed and return the WebAuthn
    // JS, but currently cannot: the JS content type can never map to `ContentType::Json`,
    // and content that isn't valid JSON can't be parsed via serde_json::from_str::<String>
    // to satisfy that arm even if it were reached. Document the failure instead of
    // asserting success until that's fixed.
    assert!(
        result.is_err(),
        "get_web_authn_java_script currently cannot succeed for any real Kratos response \
     (see the KNOWN BUG comment above) -- if this assertion starts failing, the bug \
     was fixed upstream and this test should be flipped to assert success instead."
    );
}
