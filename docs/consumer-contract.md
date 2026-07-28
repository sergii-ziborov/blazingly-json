# Consumer-derived contract

This contract was derived before integration. No consumer repository is
modified by this work.

## Audited consumers

| Consumer | JSON roles |
| --- | --- |
| mcport | JSON-RPC/MCP line parsing, mutable request values, response writing |
| Weavatrix Rust | MCP tools, CLI JSON, graph snapshots, config/import maps, memory codec |
| RadioChron | persisted configuration and reports |
| radiochron-mcp | JSON-RPC/MCP tools, resources, schemas, progress and cancellation |
| Blazingly / BlazingAPI | HTTP bodies, typed extraction, MCP, JSON-RPC, OpenAPI/docs, JWT claims, Cargo JSONL |

## Required public surface

- `Value`, `Map<String, Value>`, and `Number`;
- `json!`;
- `from_str`, `from_slice`, `from_value`;
- `to_string`, `to_string_pretty`, `to_vec`, `to_vec_pretty`, `to_writer`,
  `to_value`;
- borrowed, validated raw sub-values for routing an MCP/JSON-RPC envelope
  without constructing a complete mutable DOM;
- a slice deserializer usable by `serde_path_to_error`;
- string, bool, signed, unsigned, float, null, array, and object values;
- object/array access, mutation, indexing, JSON Pointer, and `take`;
- literal and dynamic `json!` object keys;
- Serde structs, maps, sequences, options, enums, newtypes, and byte sequences.

## Deliberate exclusions for 0.1

- arbitrary-precision numbers;
- JSON5, comments, trailing commas, NaN, and infinities;
- `preserve_order`;
- synchronous `Read`-based incremental parsing;
- direct compatibility with every private `serde_json` implementation detail.

An excluded feature is added only after a real consumer call site or a measured
hot path justifies it.

## Acceptance gates before any replacement decision

1. Differential fixtures must agree with `serde_json` on valid values and
   reject the same invalid JSON classes used by the consumers.
2. Property tests must round-trip the supported value domain.
3. Parser depth and malformed Unicode must fail safely.
4. Benchmarks must cover MCP requests, typed HTTP bodies, JWT claims, graph
   payloads, JSONL metadata, and compact response serialization.
5. Each consumer must compile and pass its own tests in a separate temporary
   integration experiment before any real dependency edit.
6. `mcport` must preserve response semantics end to end, not merely improve a
   parser microbenchmark.
