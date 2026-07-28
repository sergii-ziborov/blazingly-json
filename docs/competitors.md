# Competitive position

This is a product boundary, not a claim that one parser wins every workload.

## JSON engines

| Engine | Strongest advantage | Constraint relevant here |
| --- | --- | --- |
| [`serde_json`](https://docs.rs/serde_json/latest/serde_json/) | compatibility, maturity, ecosystem, and `RawValue` | the selected owned API is already highly optimized; its raw envelope path is currently 4-6% faster than ours |
| [`sonic-rs`](https://docs.rs/sonic-rs/latest/sonic_rs/) | SIMD parsing, lazy field access, mutable DOM | fastest path expects x86_64/aarch64 and recommends `target-cpu=native` |
| [`simd-json`](https://docs.rs/simd-json/latest/simd_json/) | SIMD tape and borrowed/owned DOMs | rewrites mutable input for its Serde slice path and deliberately uses substantial unsafe SIMD code |
| [`serde_json_borrow`](https://docs.rs/serde_json_borrow/latest/serde_json_borrow/) | borrowed DOM with fewer string allocations | different DOM semantics and escaped-key limitations unless `cowkeys` is enabled |
| [`jiter`](https://docs.rs/jiter/latest/jiter/) | schema-aware iterator plus Serde deserializer | broader parser with a different value/iteration API rather than a selected `serde_json` drop-in surface |

The defensible goal for `blazingly-json` is therefore:

1. keep the selected consumer-derived compatible API correct;
2. provide a safe, allocation-free envelope path with a low MSRV;
3. win complete protocol workloads through deferred DOM construction and
   direct serialization;
4. add SIMD only as an optional, portable-dispatch acceleration after scalar
   correctness, fuzzing, and cross-platform baselines are established.

Replacing `serde_json` everywhere today is not justified. Replacing an owned
MCP envelope with the measured borrowed design is.

## Rust MCP runtimes

The [official Rust MCP SDK](https://github.com/modelcontextprotocol/rust-sdk)
is the feature-complete reference direction: tools, resources, prompts,
sampling, roots, logging, completions, notifications, subscriptions, tasks,
OAuth, and local/remote transports. It uses Tokio. The MCP site currently
classifies the Rust SDK as Tier 2.

[`rust-mcp-sdk`](https://docs.rs/rust-mcp-sdk/latest/rust_mcp_sdk/mcp_server/)
also provides type-safe server runtimes with default high-level handlers and a
lower-level core handler surface.

Current `mcport` is not yet a breadth competitor to either:

- it is server-only and stdio-only;
- it exposes tools but not resources, prompts, roots, sampling, completions,
  subscriptions, tasks, progress, or cancellation;
- it does not yet ship generated typed tool schemas or conformance automation;
- its default protocol revision predates the current stable MCP revision.

Its differentiated advantage is narrower and real: a blocking ordered stdio
runtime needs no Tokio executor, the public trait is tiny, the proposed builder
is smaller still, and the measured full request/response path is 3.16-6.59x
faster than its current owned-DOM implementation.

The route to “best” is a layered product:

- `mcport-core`: runtime-neutral protocol types, lifecycle, cancellation, and
  conformance;
- `mcport-stdio`: the tiny blocking default with the borrowed fast path;
- optional transport adapters outside the core;
- `McpServer` builder for small servers and `ToolServer`/lower-level traits for
  custom zero-cost dispatch.

That preserves the Tokio-free invariant without limiting `mcport` to one
application or pretending that a minimal tools-only server already matches the
official SDK’s protocol coverage.
