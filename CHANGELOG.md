# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]


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


[unreleased]: https://github.com/justins-engineering/kratos-client-rust/compare/v0.1.4...master
[0.1.4]: https://github.com/justins-engineering/kratos-client-rust/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/justins-engineering/kratos-client-rust/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/justins-engineering/kratos-client-rust/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/justins-engineering/kratos-client-rust/compare/v0.1.0...v0.1.1
