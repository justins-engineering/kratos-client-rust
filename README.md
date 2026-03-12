# ory-kratos-client-wasm

This is an unofficial Ory Kratos SDK for rust. Created to use the [Fetch API](https://developer.mozilla.org/en-US/docs/Web/API/Fetch_API) instead of reqwest when building for wasm.

For the official crate, use [ory-kratos-client](https://crates.io/crates/ory-kratos-client).

API version: v25.4.0

## Features

There are 3 features to pick from:
- `wasm`, the default, which uses the browser's native Fetch API
  - For use with WASM/SPAs, i.e. [Dioxus](https://dioxuslabs.com/) web, [Yew](https://yew.rs/), [Leptos](https://leptos.dev/), etc.
- `worker`, which uses Cloudflare's [workers-rs](https://github.com/cloudflare/workers-rs) Fetch API
  - For use with [Cloudflare workers](https://developers.cloudflare.com/workers/languages/rust/)
- `reqwest`, equivalent to the official crate with more up to date dependencies
  - Not for use with WASM

You should only use one feature per project.

### Using with WASM

For WASM projects deployed in the browser add the following line to your `Cargo.toml`:

```toml
ory-kratos-client-wasm = "0.2"
```

### Using with WASM

For Cloudflare's [workers-rs](https://github.com/cloudflare/workers-rs) projects add the following line to your `Cargo.toml`:

```toml
ory-kratos-client-wasm = { version = "0.2", default-features = false, features = ["worker"] }
```

### Using with reqwest

For feature parity with the official lib, add the following line to your `Cargo.toml`:

```toml
ory-kratos-client-wasm = { version = "0.2", default-features = false, features = ["reqwest"] }
```

### Ory Self-Hosted

This SDK is for use with self-hosted Ory Kratos.
If you are developing against Ory Network, please use the [Ory Network SDK](https://www.ory.sh/docs/sdk).

### Official Kratos Documentation
- [Ory Kratos](https://www.ory.sh/kratos/docs/sdk)
