# blazingly-json

[![CI](https://github.com/sergii-ziborov/blazingly-json/actions/workflows/ci.yml/badge.svg)](https://github.com/sergii-ziborov/blazingly-json/actions/workflows/ci.yml)

Focused, safe, Tokio-free JSON for MCP, JSON-RPC, API, config, snapshot, and
JSONL workloads.

`blazingly-json` is not an incomplete reimplementation of every
`serde_json` feature. Its supported surface was derived from real call sites
in mcport, Weavatrix Rust, RadioChron, Blazingly, and BlazingAPI, then tested
differentially against `serde_json`.

The general compatible parser is only modestly faster. The large gain comes
from changing protocol architecture: borrow the envelope, avoid a mutable DOM,
serialize responses directly, and use an exact schema-aware recognizer where
the producer emits a canonical layout.

Current status: pre-release. The crate passes differential/property tests,
Rust 1.78, strict Clippy, rustdoc, packaging, Linux CI, and Windows CI. No
production consumer has been switched yet.

## Performance summary

Local Windows measurements on an Intel Core Ultra 7 255U. Paired harnesses
alternate engine order and report medians. These are workload measurements,
not universal parser claims.

### Canonical typed MCP call

A generated compact `tools/call` recognizer borrows three strings, parses a
`u64` and a boolean, verifies complete consumption, and falls back to the
strict order-independent parser on any mismatch.

| Path | Median range | Compared with canonical `&str` |
| --- | ---: | ---: |
| `CanonicalScanner` over prevalidated `&str` | 56.24-74.61 ns | 1.00x |
| `CanonicalScanner::from_slice` including UTF-8 validation | 66.30-94.02 ns | 1.10-1.26x slower |
| strict order-independent `Cursor` | 295.31-421.29 ns | 5.26-5.65x slower |
| typed `serde_json` derive | 368.26-554.43 ns | 6.55-7.43x slower |

Both canonical paths allocate zero bytes. Even when UTF-8 validation is
included, the measured advantage over typed `serde_json` remains 5.55-5.99x.

This path intentionally accepts only its exact schema, field order, compact
separators, and plain strings. Whitespace, reordered fields, escaped strings,
or another schema are not treated as errors: they take the general parser
fallback.

### MCP runtime architecture

Borrowed request routing plus direct response serialization compared with
mcport's original owned-`Value` + clone implementation:

| Request | Borrowed path | Original mcport | Speedup |
| --- | ---: | ---: | ---: |
| ping | 344.93 ns | 1,425.04 ns | 4.13x |
| initialize | 1,014.87 ns | 6,683.85 ns | 6.59x |
| tools/list | 2,524.99 ns | 7,987.72 ns | 3.16x |
| tools/call | 2,103.43 ns | 7,298.68 ns | 3.47x |

Allocation measurements for request dispatch:

| Request | Borrowed path | Original mcport |
| --- | ---: | ---: |
| ping | 0 allocations / 0 bytes | 8 allocations / 670 bytes |
| tools/call with owned arguments | 5 allocations / 668 bytes | 27 allocations / 2,702 bytes |
| canonical typed tools/call | 0 allocations / 0 bytes | 27 allocations / 2,702 bytes |

### `RawValue`

`blazingly_json::value::RawValue` implements the common
`serde_json::value::RawValue` contract: borrowed and boxed deserialization,
verbatim serialization, `from_string`, `from_string_unchecked`, `get`,
`into_string`, `to_raw_value`, constants, cloning, and direct deserialization
from `&RawValue`.

Three paired local runs after warm-up produced these ranges:

| Workload | Speedup over `serde_json::RawValue` |
| --- | ---: |
| small MCP borrowed parse | 1.22-1.59x |
| small MCP boxed parse | 1.21-1.59x |
| small MCP `from_string` | 1.34-1.54x |
| small MCP verbatim `to_vec` | 1.69-2.36x |
| 1,000,000-number borrowed parse, 6.57 MiB | 1.42-1.79x |
| 1,000,000-number boxed parse | 1.19-1.55x |
| compact protocol-record array borrowed parse | 1.62-2.07x |
| large preallocated writer output | approximately 1.00x |

Large output is memory-bandwidth-bound and is reported as parity, not as a
multiple-times win. For a complete raw document, `RawValue::to_vec` and
`RawValue::write_to` bypass the generic Serde marker path. Embedded raw values
remain compatible with ordinary `to_vec`, `to_writer`, and pretty output.

Allocation measurements:

| Operation | blazingly-json | serde_json |
| --- | ---: | ---: |
| small borrowed parse | 0 | 1 allocation / 8 bytes |
| small boxed parse | 1 allocation / 154 bytes | 2 allocations / 162 bytes |
| `from_string` with reusable capacity | 0 | 1 allocation / 8 bytes |
| 1,000,000-number borrowed parse | 0 | 0 |
| same payload as owned `Vec<u64>` in serde_json | - | 1 allocation + 18 reallocations / 8 MiB live |

The raw parser first tries strict compact numeric-array and flat
protocol-record recognizers. They must consume a complete value; whitespace,
escaping, nesting, or any shape mismatch takes the fully validating general
fallback.

### Compatible API and large payloads

The compatible owned/typed path does not reach a multiple-times speedup:

| Workload | blazingly-json | serde_json | Difference |
| --- | ---: | ---: | ---: |
| MCP mutable `Value` parse | 2.195 us | 2.313 us | +5.37% |
| MCP typed parse | 1.391 us | 1.423 us | +2.33% |
| MCP typed encode | 420.75 ns | 427.20 ns | +1.53% |
| Weavatrix-like graph parse | 77.72 us | 80.00 us | +2.92% |
| Weavatrix-like graph encode | 22.13 us | 24.70 us | +11.64% |
| Cargo artifact JSONL parse | 1.650 us | 1.850 us | +12.13% |
| 1,000,000 `u64` values | 254.9 MiB/s | 249.8 MiB/s | +2.01% |
| 1,000,000 typed records, 80.98 MiB | 150.1 MiB/s | 147.6 MiB/s | +1.70% |
| 1,000,000 varied MCP `Value` parses | 69.7 MiB/s | 68.0 MiB/s | +2.48% |

Parsing one million owned records makes exactly 2,000,001 allocations and
retains 100.27 MiB with either engine: the owned strings and vectors dominate,
not the parser. Minimal stripped Windows executables are also effectively tied:
990,208 bytes for `blazingly-json` and 989,184 bytes for `serde_json`.

The conclusion is deliberate: replacing `serde_json` with another compatible
owned DOM cannot honestly promise 2-3x less memory or 2-3x more speed. The
canonical and borrowed APIs exist to remove that ownership work.

## Usage

Until the first crates.io release:

```toml
[dependencies]
blazingly-json = { git = "https://github.com/sergii-ziborov/blazingly-json" }
serde = { version = "1", features = ["derive"] }
```

After publication, the Git dependency becomes:

```toml
blazingly-json = "0.1"
```

### Serde-compatible typed JSON

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
struct Request<'a> {
    method: &'a str,
    limit: u64,
}

let request: Request<'_> =
    blazingly_json::from_str(r#"{"method":"search","limit":20}"#)?;
let encoded = blazingly_json::to_string(&request)?;
# Ok::<(), blazingly_json::Error>(())
```

### Zero-copy envelope

`RawJson` validates a nested value and borrows its exact input without building
a DOM. The payload is materialized only if a selected handler needs it.

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

### Drop-in-style `RawValue`

The type is available both at `blazingly_json::value::RawValue` and at the
crate root.

```rust
use blazingly_json::value::{to_raw_value, RawValue};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct Input<'a> {
    #[serde(borrow)]
    payload: &'a RawValue,
}

#[derive(Serialize)]
struct Output<'a> {
    payload: &'a RawValue,
}

let input: Input<'_> =
    blazingly_json::from_str(r#"{"payload": { "limit": 20 }}"#)?;
assert_eq!(input.payload.get(), r#"{ "limit": 20 }"#);

let output = blazingly_json::to_string(&Output {
    payload: input.payload,
})?;
assert_eq!(output, r#"{"payload":{ "limit": 20 }}"#);

let owned = to_raw_value(&[1, 2, 3])?;
assert_eq!(owned.get(), "[1,2,3]");
# Ok::<(), blazingly_json::Error>(())
```

### Order-independent protocol cursor

`Cursor` visits only routing fields, skips unknown values safely, and validates
the complete document.

```rust
use blazingly_json::Cursor;

let mut method = None;
let mut cursor = Cursor::from_str(r#"{"jsonrpc":"2.0","method":"ping"}"#);
cursor.object(|request| {
    while let Some(field) = request.next_field()? {
        match field.name() {
            "method" => method = Some(field.string()?),
            _ => field.skip()?,
        }
    }
    Ok(())
})?;
cursor.end()?;
assert_eq!(method.as_deref(), Some("ping"));
# Ok::<(), blazingly_json::Error>(())
```

### Schema-aware canonical path

Use this only as a recognizer with a mandatory fallback. A successful
recognizer must consume the complete input.

```rust
use blazingly_json::CanonicalScanner;

let input = r#"{"method":"search","limit":20}"#;
let mut scanner = CanonicalScanner::new(input);
let recognized = (|| {
    scanner.literal(r#"{"method":"#)?;
    let method = scanner.plain_string()?;
    scanner.literal(r#","limit":"#)?;
    let limit = scanner.unsigned()?;
    scanner.literal("}")?;
    scanner.is_finished().then_some((method, limit))
})();

if let Some((method, limit)) = recognized {
    assert_eq!((method, limit), ("search", 20));
} else {
    // Parse with Cursor or from_str: the input may still be valid JSON.
}
```

## Supported surface

- `Value`, `Map<String, Value>`, `Number`, and `json!`;
- `from_str`, `from_slice`, `from_value`;
- `to_string`, pretty/string/vector/writer variants, and `to_value`;
- Serde structs, maps, sequences, options, enums, newtypes, and bytes;
- string, boolean, signed, unsigned, float, null, array, and object values;
- access, mutation, indexing, JSON Pointer, and `take`;
- validated borrowed `RawJson`;
- drop-in-style borrowed/boxed `RawValue` and `to_raw_value`;
- routing-oriented `Cursor`;
- exact-layout `CanonicalScanner`;
- strict RFC 8259 parsing with recursion limits and source locations.

Deliberate 0.1 exclusions:

- arbitrary-precision numbers;
- JSON5, comments, trailing commas, NaN, and infinities;
- `preserve_order`;
- synchronous `Read`-based incremental parsing;
- private `serde_json` implementation details other than the established
  `RawValue` Serde token used for serializer interoperability.

## Dependencies and safety

The runtime dependencies are `serde`, `memchr`, `itoa`, `lexical-core`, and
`zmij`. `serde_json` is a development-only oracle for differential tests and
benchmarks. Tokio, Hyper, and Axum are absent.

`unsafe_code` is denied crate-wide. The sole exception is `raw_value.rs`,
which contains three documented `repr(transparent)` DST conversions between
`str` and `RawValue`. No parser, scanner, serializer, or other module may use
unsafe code. The optional `from_string_unchecked` API is itself unsafe and
states the same validity invariant as the serde_json API it replaces.

## Competitive position

- `serde_json`: broad compatibility, maturity, ecosystem, and an excellent
  borrowed `RawValue`;
- `sonic-rs`: SIMD parsing and lazy field access;
- `simd-json`: SIMD tape plus borrowed and owned DOMs, with mutable-input and
  unsafe-code trade-offs;
- `serde_json_borrow`: borrowed DOM and reduced string allocation;
- `jiter`: iterator/schema-oriented parsing plus Serde support.

`blazingly-json` does not claim to beat every engine on every document. Its
target is portable, low-MSRV protocol performance, a tiny auditable unsafe
island for `RawValue`, and generated schema-aware MCP/API codecs with a strict
fallback.

See:

- [benchmark methodology and full results](docs/benchmarks.md);
- [consumer-derived compatibility contract](docs/consumer-contract.md);
- [consumer compile/test probes](docs/consumer-validation.md);
- [competitor and MCP runtime analysis](docs/competitors.md);
- [mcport integration spike](docs/mcport-integration.md).

## Verification

```text
cargo fmt --all -- --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo +1.78 check --lib
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
cargo package

cargo bench --bench paired_comparison
cargo bench --bench large_payload
cargo bench --bench allocation_comparison
cargo bench --bench mcp_fast_path
cargo bench --bench mcp_allocations
cargo bench --bench mcport_end_to_end
cargo bench --bench raw_value_comparison
cargo bench --bench raw_value_allocations
```

## License

MIT
