# Consumer compatibility probes

No consumer repository was modified. Each probe used a temporary clone whose
`serde_json` dependency name was redirected to the local `blazingly-json`
package. The original imports and call sites stayed unchanged.

| Consumer | Probe | Result |
| --- | --- | --- |
| mcport | drop-in probe plus fast-path integration spike | original suite plus 2 new tests and 1 doctest passed; strict Clippy passed |
| radiochron-mcp | `cargo test` | 14 unit tests and 3 MCP conformance tests passed |
| Blazingly / BlazingAPI | `cargo check --workspace`, then `cargo test --workspace` | all workspace crates compiled; 266 non-zero test cases passed |
| Weavatrix Rust | unchanged `serde_json::...` call sites, direct Cargo alias to local blazingly-json, `cargo check --all-features`, then `cargo test --all-features` | all features compiled; 32 tests passed |

The probes discovered and moved four real requirements into this crate:

- conversion from the JSON error to `std::io::Error`;
- the by-value `to_value` signature;
- dynamic object keys in `json!({ (expression): value })`;
- `Map::entry` accepting values convertible into `String`;
- borrowed raw sub-values for an allocation-free MCP routing envelope;
- interoperable borrowed and boxed `RawValue`, including the established
  Serde marker used by serde_json.

This is evidence for source compatibility with the audited call sites. It is
not yet permission to replace dependencies in those repositories. Fuzzing,
longer cross-platform benchmarks, and explicit per-repository migration
reviews remain separate gates.
