# worker_smoke

A throwaway Cloudflare Worker that calls `ory-kratos-client-wasm`'s `worker`
feature (`worker_apis::metadata_api::is_alive`) against a live local Kratos, so
the `worker` feature's own request-building/response-parsing code gets a real
end-to-end check the way `reqwest`-feature tests (`../*_api.rs`) and the
`wasm`-feature smoke test (`../wasm_smoke.rs`) do for their features. See
`src/lib.rs` for the full rationale.

Not wired into `cargo test` -- `worker::Fetch` only works inside an actual
Workers runtime, so this has to run as a real `wrangler dev` Worker and be
polled from outside.

## Running it

Requires the PidgeIoT docker-compose Kratos stack up (public API on
`127.0.0.1:4433`), plus `worker-build` (`cargo install worker-build`) and
`wrangler` (this repo uses `bunx wrangler`, matching PidgeIoT's own dovecote
workflow).

```sh
cd tests/worker_smoke
worker-build --dev
bunx wrangler dev --local --ip 127.0.0.1 --port 8899
```

In another terminal:

```sh
curl -sS -w '\n[%{http_code}]\n' http://127.0.0.1:8899/
```

Expect `PASS: is_alive returned status="ok"` with a `[200]`. Anything else
(including a connection error, which usually means Kratos isn't running) is a
real finding -- see the crate-level report for what "real finding" turned up
here versus in the `reqwest`/`wasm` smoke tests.

Stop the worker with Ctrl-C (or `pkill -f "wrangler dev"`) when done. `build/`
and `target/` are gitignored; nothing here should be committed except this
README, `Cargo.toml`, `wrangler.toml`, and `src/lib.rs`.
