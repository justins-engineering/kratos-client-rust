# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.5] - 2026-07-23

### Changed

- Synced `src/apis`, `src/models`, `docs`, and `.openapi-generator` to upstream
  ory/kratos-client-rust@86df448 (Kratos client for API v26.2.0), and
  regenerated `wasm_apis`/`worker_apis` from it via `tools/regen-fetch-apis`.

### Fixed

- `UiNodeAttributes` now deserializes real Kratos responses again. Upstream's
  v26.2.0 openapi-generator output re-emitted the enum as
  `#[serde(tag = "node_type")]`, which fails with a "missing field node_type"
  error: each variant struct also declares a `node_type` field, and serde's
  internally-tagged representation consumes the discriminator key before the
  variant can read it. Reverted to `#[serde(untagged)]`, re-applying this
  fork's standing workaround for upstream #3.

### Internal

- Hardened the scheduled `upstream-sync` workflow and CI: content-based change
  detection (no longer re-syncs already-synced content off a frozen
  merge-base), idempotent label creation, `GH_REPO`-pinned `gh` invocations, a
  PR-state-aware duplicate-PR guard, and a retry around the intermittently
  port-bind-flaky ChromeDriver wasm smoke test. No public API changes.

## [0.2.4] - 2026-07-20

### Added

- `tools/regen-fetch-apis`: a codemod that regenerates `wasm_apis`/`worker_apis`
  from `src/apis` (the openapi-generator-tech reqwest tree, merged in from
  upstream), replacing the manual hand-port step this fork has needed since
  splitting into three feature trees. Not part of the published lib surface.
  See `tools/regen-fetch-apis/src/main.rs` for the design notes.
- `handle_response`/`handle_empty_response` helpers in `wasm_apis::mod`/
  `worker_apis::mod`, factoring out the response-handling tail (status check,
  content-type match, deserialize) that used to be duplicated in every
  function (~150 call sites) into one place per feature.
- Integration test suite (`tests/*_api.rs`, `reqwest` feature) against a live
  Kratos instance, covering every public function in `metadata_api`,
  `identity_api`, `courier_api`, and `frontend_api`.
- `tests/wasm_smoke.rs` and `tests/worker_smoke/`: smoke tests proving the
  `wasm` and `worker` features build a real request and parse a real Kratos
  response through their actual Fetch implementations, not just the
  `reqwest` feature.

### Fixed

- `is_alive` in both `wasm_apis::metadata_api` and `worker_apis::metadata_api`
  called `/health/ready` instead of `/health/alive` (independently, in both
  hand-ported files) -- a copy-paste error from the adjacent `is_ready`.
  Every `wasm`/`worker`-feature caller of `is_alive` has silently been
  getting `is_ready`'s result. Fixed by construction: the regenerated trees
  derive the URI from the correct `src/apis` source rather than a
  hand-copied one.
- `web_sys::window().unwrap()` (a bare, unhelpful panic) replaced with the
  same descriptive `.expect("Failed to get Window object")` already used
  elsewhere in `wasm_apis`, in the files that had been missed when this was
  first done in 0.2.1 (`identity_api.rs`, `metadata_api.rs`).

### Changed

- Regenerated `wasm_apis`/`worker_apis` (all four API files, 56 functions)
  via the new codemod. Net -4000 lines from the response-tail dedup. A few
  things the hand-port was internally inconsistent about are now uniform
  (disclosed, reviewed, no behavioral difference in any case checked):
  `RequestCredentials::Include` on every `wasm`-feature request (was mixed
  `Include`/`SameOrigin`/omitted across files -- see the 0.1.2/0.1.3 history
  above for why `Include` is the intended default), and
  `Option<Vec<String>>` query params always routed through the same
  `add_query` mechanism as any other param.
- `src/apis` (the `reqwest` feature, openapi-generator autogen output) is
  unaffected by any of the above -- it continues to omit
  `Accept: application/json` on `create_browser_*_flow`/
  `update_verification_flow` (a real, confirmed gap: these calls currently
  cannot succeed over the `reqwest` feature against real Kratos), which is
  upstream's tree and not something this fork hand-patches. Noted here so
  it's easy to find later, not because this release fixes it.
- Updated the tracked API version reference to `v26.2.0` (README, CI Kratos
  image `oryd/kratos:v26.2`, `.ci/kratos/kratos.yml`). Upstream `v26.2.0` is
  a pure version relabel of the previously-tracked `v25.4.0` -- the OpenAPI
  surface is unchanged, so no generated code differs; only the version label
  moves.

## [0.2.3] - 2026-05-01

### Changed

- Updated cargo dependencies

## [0.2.2] - 2026-03-12

### Changed

- Crate features `default = ["wasm"]`
- Updated README

## [0.2.1] - 2026-03-12

### Changed

- wasm_apis
  - `web_sys::window().unwrap()` to `web_sys::window().expect("Failed to get Window object")`
  - `req.dyn_into().unwrap()` to `req.dyn_into().expect("Failed to dynamically cast JsFuture into Response")`

### Added

- Add worker_apis for compatibility with workers-rs

## [0.2.0] - 2026-02-07

### Added

- Upstream changes
- `query` feature to reqwest
- `ContentType` enum from upstream
- `Missing` type to `ContentType`
- `impl From<&web_sys::Response> for ContentType`

### Changed

- Update API version to v25.4.0
- Updated dependencies
- Seperated wasm and reqwest code for easier merge
- Re-export `wasm_apis` as `apis`
- Identies API multi-params each get their own key instead of being passed as array
- `ContentType` based error messages

### Removed

- All `local_var_` prefixes
- Stand-alone `add_query` function

## [0.1.6] - 2025-09-22

### Changed

- Conditional compile gates based on features only, not target_family
- Updated dependencies

### Removed

- `uuid` crate dependency

### Fixed

- Crate now compiles correctly with Dioxus fullstack

## [0.1.5] - 2025-09-07

### Changed

- Fixed metadata "documentation" link


## [0.1.4] - 2025-09-07

### Added

- `default`, `reqwest`, and `wasm` features
- `wasm` feature gates to AddQuery trait and function
- compile_error if `wasm` and `reqwest` features are enabled at the same time

### Changed

- Conditional compilation flags for features
- Retroactively added v0.1.1 to CHANGELOG

### Removed

- `#![allow(clippy::too_many_arguments)]`
- Unused `use std::fmt::Debug;` import in apis/mod.rs
- `flate2` dependency
- `serde_with` features "base64" and "macros"


## [0.1.3] - 2025-09-07

### Changed

- Revert "Replaced all instances of `RequestCredentials::Include` with `RequestCredentials::SameOrigin`"


## [0.1.2] - 2025-09-06

### Added

- CHANGLOG.md

### Changed

- Replaced all instances of `RequestCredentials::Include` with `RequestCredentials::SameOrigin` to fix CORS issues with chrome
- Updated cargo dependencies


## [0.1.1] - 2025-09-04

### Changed

- Updated READEME to reflect this fork
- Updated Cargo.toml info and edition


[unreleased]: https://github.com/justins-engineering/kratos-client-rust/compare/v0.2.5...master
[0.2.5]: https://github.com/justins-engineering/kratos-client-rust/compare/v0.2.4...v0.2.5
[0.2.4]: https://github.com/justins-engineering/kratos-client-rust/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/justins-engineering/kratos-client-rust/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/justins-engineering/kratos-client-rust/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/justins-engineering/kratos-client-rust/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/justins-engineering/kratos-client-rust/compare/v0.1.6...v0.2.0
[0.1.6]: https://github.com/justins-engineering/kratos-client-rust/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/justins-engineering/kratos-client-rust/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/justins-engineering/kratos-client-rust/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/justins-engineering/kratos-client-rust/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/justins-engineering/kratos-client-rust/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/justins-engineering/kratos-client-rust/compare/v0.1.0...v0.1.1
