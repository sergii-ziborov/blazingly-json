# mcport integration spike

The production `mcport` checkout was not modified. The proposed path was
implemented and tested in an isolated copy against the real crate source.

## What changed in the spike

- request envelopes borrow `id`, `method`, and routing parameters;
- `arguments` is captured as `RawJson` and decoded to `Value` only for
  `tools/call`;
- responses are serialized from typed borrowed structs instead of first
  building and cloning an owned response DOM;
- the existing `ToolServer`, `ToolReply`, `dispatch`, `serve`, and
  `serve_streams` surface remains available;
- an optional `McpServer` builder removes the three-method trait boilerplate
  for small servers while retaining the trait for custom dispatch.

The next integration layer can attach a generated canonical recognizer to a
typed tool handler. Measured matching is 6.55-7.43x faster than the equivalent
`serde_json` typed derive and allocates nothing. The existing order-independent
`Cursor` remains the mandatory fallback, so ordinary MCP clients are not
required to emit the canonical field order.

The builder shape tested in the spike is:

```rust
let mut server = McpServer::new(ServerIdentity::new("echo", "1.0.0", "Echoes."))
    .tool(
        "echo",
        "Echo the arguments.",
        json!({"type": "object", "additionalProperties": true}),
        ToolReply::structured,
    );
server.serve()
```

Stateful handlers use `McpServer::with_state(...).tool_with_state(...)`.

## Validation

- all original unit and documentation tests pass;
- a new differential session test compares the fast stream output with the
  existing public `dispatch` result for initialize, ping, tool listing,
  structured/plain/error tool calls, protocol errors, null IDs, and
  notifications;
- strict Clippy with pedantic warnings passes;
- the real consumer API remains source-compatible in the spike.

This establishes feasibility, not authorization to replace the dependency in
the real checkout. That switch remains a separate reviewed change after the
standalone JSON crate completes its release gates.
