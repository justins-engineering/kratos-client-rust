//! Shared setup for integration tests that run the `reqwest` feature against
//! a real, locally running Ory Kratos (see PidgeIoT's docker-compose.yml).
//!
//! Base URLs are overridable via env vars so this is CI-friendly:
//!   KRATOS_PUBLIC_URL (default http://127.0.0.1:4433)
//!   KRATOS_ADMIN_URL  (default http://127.0.0.1:4434)

use ory_kratos_client_wasm::apis::configuration::Configuration;
use ory_kratos_client_wasm::apis::identity_api;
use ory_kratos_client_wasm::models;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn public_url() -> String {
    std::env::var("KRATOS_PUBLIC_URL").unwrap_or_else(|_| "http://127.0.0.1:4433".to_owned())
}

pub fn admin_url() -> String {
    std::env::var("KRATOS_ADMIN_URL").unwrap_or_else(|_| "http://127.0.0.1:4434".to_owned())
}

pub fn public_config() -> Configuration {
    Configuration {
        base_path: public_url(),
        ..Configuration::default()
    }
}

pub fn admin_config() -> Configuration {
    Configuration {
        base_path: admin_url(),
        ..Configuration::default()
    }
}

/// A unique-enough identifier for this test run, so parallel `cargo test`
/// invocations (and repeated runs against a persistent Kratos instance)
/// don't collide on unique fields like email.
pub fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before UNIX_EPOCH")
        .as_nanos();
    format!("{nanos}")
}

pub fn unique_email() -> String {
    format!("kratos-client-test-{}@example.com", unique_suffix())
}

/// Creates an identity against the `user` schema (schemas/kratos/identity.user.schema.json
/// in the PidgeIoT repo) with a password credential already attached, so tests can
/// immediately exercise login/session flows without a separate registration step.
///
/// Returns (identity, email, password). Callers are responsible for deleting the
/// identity via `identity_api::delete_identity` when done, to keep the local Kratos
/// instance clean across repeated test runs.
pub async fn create_password_identity(
    admin_cfg: &Configuration,
) -> (models::Identity, String, String) {
    let email = unique_email();
    let password = "correct-horse-battery-staple-1".to_owned();

    let credentials = models::IdentityWithCredentials {
        password: Some(Box::new(models::IdentityWithCredentialsPassword {
            config: Some(Box::new(models::IdentityWithCredentialsPasswordConfig {
                password: Some(password.clone()),
                hashed_password: None,
                use_password_migration_hook: None,
            })),
        })),
        oidc: None,
        saml: None,
    };

    let body = models::CreateIdentityBody {
        schema_id: "user".to_owned(),
        traits: serde_json::json!({ "email": email.clone() }),
        credentials: Some(Box::new(credentials)),
        ..Default::default()
    };

    let identity = identity_api::create_identity(admin_cfg, Some(body))
        .await
        .expect("create_identity should succeed against a live Kratos");

    (identity, email, password)
}
