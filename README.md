# blazingly-json

`blazingly-json` is a focused, Tokio-free JSON engine for the small and
medium protocol payloads used by Blazingly, MCP servers, Weavatrix, and
RadioChron.

The project is intentionally not a clone of every `serde_json` feature. Its
compatibility boundary is derived from production call sites in those four
consumers, then tested differentially against `serde_json`.

Current status: pre-release implementation and benchmark work. Do not replace a
production dependency with this crate until the differential, fuzz, and
consumer compatibility gates are complete.

## Design constraints

- no Tokio, Hyper, or Axum;
- no runtime JSON dependency;
- strict RFC 8259 parsing;
- Serde-compatible typed encoding and decoding;
- owned `Value` only where callers need a mutable DOM;
- optimized paths for small JSON-RPC, HTTP, JWT, config, snapshot, and JSONL
  payloads;
- correctness is established before performance claims.

See [docs/consumer-contract.md](docs/consumer-contract.md) for the audited API
surface and exclusions. See [docs/benchmarks.md](docs/benchmarks.md) for the
current local comparison against `serde_json`.

## Development gates

```text
cargo fmt --check
cargo check --all-targets
cargo test
cargo clippy --all-targets -- -D warnings
cargo bench --bench serde_json_comparison
```
