# blazingly-json

`blazingly-json` is a focused, Tokio-free JSON engine for the small and
medium protocol payloads used by Blazingly, MCP servers, Weavatrix, and
RadioChron.

The project is intentionally not a clone of every `serde_json` feature. Its
compatibility boundary is derived from production call sites in those four
consumers, then tested differentially against `serde_json`.

Current status: pre-release. Differential/property tests and isolated consumer
compile/test probes pass; fuzzing and cross-platform performance evidence are
still release gates. No production consumer has been switched.

## Design constraints

- no Tokio, Hyper, or Axum;
- no runtime JSON dependency;
- Rust 1.78 minimum supported version;
- strict RFC 8259 parsing;
- Serde-compatible typed encoding and decoding;
- owned `Value` only where callers need a mutable DOM;
- allocation-free `RawJson` borrowing for envelopes that can defer payload
  decoding;
- allocation-free `CanonicalScanner` recognition for generated or
  protocol-specific layouts, with mandatory general-parser fallback;
- optimized paths for small JSON-RPC, HTTP, JWT, config, snapshot, and JSONL
  payloads;
- correctness is established before performance claims.

See [docs/consumer-contract.md](docs/consumer-contract.md) for the audited API
surface and exclusions. See [docs/benchmarks.md](docs/benchmarks.md) for the
current local comparison against `serde_json`. See
[docs/consumer-validation.md](docs/consumer-validation.md) for temporary
drop-in compile and test probes against all four consumer families. See
[docs/competitors.md](docs/competitors.md) for the JSON and MCP competitor
boundary.

## Zero-copy envelope path

`RawJson` validates a nested value and borrows its exact input bytes without
building a DOM. A protocol can decode only its routing fields and materialize
the payload only when a handler needs it:

```rust
use blazingly_json::{RawJson, Value};
use serde::Deserialize;

#[derive(Deserialize)]
struct Call<'a> {
    method: &'a str,
    #[serde(borrow)]
    arguments: RawJson<'a>,
}

let call: Call<'_> = blazingly_json::from_str(
    r#"{"method":"query_graph","arguments":{"limit":20}}"#,
)?;
let arguments = call.arguments.deserialize::<Value>()?;
# Ok::<(), blazingly_json::Error>(())
```

## Schema-aware canonical path

When both ends produce one known compact layout, `CanonicalScanner` can match
fixed structure and borrow typed fields without building a DOM. A mismatch is
not a JSON error: callers must fall back to `Cursor` or `from_slice`, and a
successful recognizer must consume the whole input. `new(&str)` reuses UTF-8
validation already performed by line-oriented stdio; `from_slice(&[u8])`
validates a byte payload once before matching.

```rust
use blazingly_json::CanonicalScanner;

let input = r#"{"method":"search","limit":20}"#;
let mut scanner = CanonicalScanner::new(input);
scanner.literal(r#"{"method":"#).unwrap();
let method = scanner.plain_string().unwrap();
scanner.literal(r#","limit":"#).unwrap();
let limit = scanner.unsigned().unwrap();
scanner.literal("}").unwrap();

assert!(scanner.is_finished());
assert_eq!((method, limit), ("search", 20));
```

## Development gates

```text
cargo fmt --check
cargo check --all-targets
cargo test
cargo clippy --all-targets -- -D warnings
cargo bench --bench paired_comparison
cargo bench --bench large_payload
cargo bench --bench mcp_fast_path
cargo bench --bench mcp_allocations
cargo bench --bench mcport_end_to_end
```
