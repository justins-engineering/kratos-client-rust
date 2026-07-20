//! Integration tests for `identity_api` (admin identity CRUD, schemas), run with:
//!   cargo test --no-default-features --features reqwest --target x86_64-unknown-linux-gnu --test identity_api
//! against a live local Kratos admin API (see tests/common/mod.rs for base URLs).
//!
//! Session-lifecycle functions (extend_session, get_session, list_identity_sessions,
//! list_sessions, disable_session, delete_identity_sessions) live in tests/frontend_api.rs
//! instead, since they need a real session minted via a login flow.

#![cfg(feature = "reqwest")]

mod common;

use ory_kratos_client_wasm::apis::identity_api;
use ory_kratos_client_wasm::models;

#[tokio::test]
async fn create_get_update_patch_delete_identity_roundtrip() {
    let admin_cfg = common::admin_config();
    let (identity, email, _password) = common::create_password_identity(&admin_cfg).await;

    // get_identity
    let fetched = identity_api::get_identity(&admin_cfg, &identity.id, None)
        .await
        .expect("get_identity should succeed");
    assert_eq!(fetched.id, identity.id);
    assert_eq!(
        fetched.traits.expect("identity should have traits")["email"],
        email
    );

    // list_identities, filtered down to just this one via `ids`
    let listed = identity_api::list_identities(
        &admin_cfg,
        None,
        None,
        None,
        None,
        None,
        Some(vec![identity.id.clone()]),
        None,
        None,
        None,
        None,
    )
    .await
    .expect("list_identities should succeed");
    assert!(
        listed.iter().any(|i| i.id == identity.id),
        "expected the newly created identity to show up in list_identities"
    );

    // update_identity: full replace, flip a trait
    let new_email = common::unique_email();
    let update_body = models::UpdateIdentityBody {
        schema_id: "user".to_owned(),
        state: models::update_identity_body::StateEnum::Active,
        traits: serde_json::json!({ "email": new_email.clone() }),
        external_id: None,
        metadata_admin: None,
        metadata_public: None,
        credentials: None,
    };
    let updated = identity_api::update_identity(&admin_cfg, &identity.id, Some(update_body))
        .await
        .expect("update_identity should succeed");
    assert_eq!(
        updated.traits.expect("identity should have traits")["email"],
        new_email
    );

    // patch_identity: JSON Patch replace on the same trait
    let patched_email = common::unique_email();
    let patch = vec![models::JsonPatch {
        op: "replace".to_owned(),
        path: "/traits/email".to_owned(),
        value: Some(Some(serde_json::json!(patched_email.clone()))),
        from: None,
    }];
    let patched = identity_api::patch_identity(&admin_cfg, &identity.id, Some(patch))
        .await
        .expect("patch_identity should succeed");
    assert_eq!(
        patched.traits.expect("identity should have traits")["email"],
        patched_email
    );

    // delete_identity
    identity_api::delete_identity(&admin_cfg, &identity.id)
        .await
        .expect("delete_identity should succeed");

    let after_delete = identity_api::get_identity(&admin_cfg, &identity.id, None).await;
    assert!(
        after_delete.is_err(),
        "expected get_identity to fail (404) after delete_identity"
    );
}

#[tokio::test]
async fn get_identity_by_external_id_finds_it() {
    let admin_cfg = common::admin_config();
    let email = common::unique_email();
    let external_id = format!("pidgeiot-test-{}", common::unique_suffix());

    let body = models::CreateIdentityBody {
        schema_id: "user".to_owned(),
        traits: serde_json::json!({ "email": email }),
        external_id: Some(external_id.clone()),
        ..Default::default()
    };
    let identity = identity_api::create_identity(&admin_cfg, Some(body))
        .await
        .expect("create_identity should succeed");

    let found = identity_api::get_identity_by_external_id(&admin_cfg, &external_id, None)
        .await
        .expect("get_identity_by_external_id should succeed");
    assert_eq!(found.id, identity.id);

    identity_api::delete_identity(&admin_cfg, &identity.id)
        .await
        .expect("cleanup delete_identity should succeed");
}

#[tokio::test]
async fn list_and_get_identity_schema() {
    let admin_cfg = common::admin_config();

    let schemas = identity_api::list_identity_schemas(&admin_cfg, None, None, None, None)
        .await
        .expect("list_identity_schemas should succeed");
    assert!(
        schemas.iter().any(|s| s.id == "user"),
        "expected the `user` schema (schemas/kratos/identity.user.schema.json) to be registered"
    );

    let schema = identity_api::get_identity_schema(&admin_cfg, "user")
        .await
        .expect("get_identity_schema should succeed");
    assert_eq!(schema["title"], "User");
}

#[tokio::test]
async fn batch_patch_identities_creates_one() {
    let admin_cfg = common::admin_config();
    let email = common::unique_email();

    let create_body = models::CreateIdentityBody {
        schema_id: "user".to_owned(),
        traits: serde_json::json!({ "email": email.clone() }),
        ..Default::default()
    };
    // patch_id must parse as a UUID server-side if set at all (confirmed by hitting
    // real Kratos: a plain string like "test-patch-1" 400s with
    // "uuid: incorrect UUID length"), and it's optional, so just omit it.
    let body = models::PatchIdentitiesBody {
        identities: Some(vec![models::IdentityPatch {
            create: Some(Box::new(create_body)),
            patch_id: None,
        }]),
    };

    let response = identity_api::batch_patch_identities(&admin_cfg, Some(body))
        .await
        .expect("batch_patch_identities should succeed");
    let identities = response.identities.expect("expected an identities list");
    assert_eq!(identities.len(), 1);
    let created_id = identities[0]
        .identity
        .clone()
        .expect("expected the batch result to include the created identity id");

    identity_api::delete_identity(&admin_cfg, &created_id)
        .await
        .expect("cleanup delete_identity should succeed");
}

#[tokio::test]
async fn delete_identity_credentials_rejects_removing_the_last_first_factor() {
    // Kratos refuses to delete an identity's only first-factor credential (it would
    // leave the identity permanently unable to authenticate) — confirmed by hitting
    // real Kratos: this 400s with "You cannot remove the last first factor credential."
    // rather than the naive "delete succeeds" the function name might suggest.
    // That's correct, desirable server behavior; assert the client surfaces it as
    // an `Err`, and that the credential really is still there afterwards.
    let admin_cfg = common::admin_config();
    let (identity, _email, _password) = common::create_password_identity(&admin_cfg).await;

    let result =
        identity_api::delete_identity_credentials(&admin_cfg, &identity.id, "password", None).await;
    assert!(
        result.is_err(),
        "expected deleting the last first-factor credential to be rejected"
    );

    let fetched =
        identity_api::get_identity(&admin_cfg, &identity.id, Some(vec!["password".to_owned()]))
            .await
            .expect("get_identity should succeed");
    let has_password_cred = fetched
        .credentials
        .as_ref()
        .map(|c| c.contains_key("password"))
        .unwrap_or(false);
    assert!(
        has_password_cred,
        "the password credential should still be present since the delete was rejected"
    );

    identity_api::delete_identity(&admin_cfg, &identity.id)
        .await
        .expect("cleanup delete_identity should succeed");
}
