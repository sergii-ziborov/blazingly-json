use blazingly_json::{CanonicalBytesScanner, CanonicalScanner, JsonCursor};

#[derive(Debug, Eq, PartialEq)]
struct Call<'a> {
    id: &'a str,
    name: &'a str,
    query: &'a str,
    limit: u64,
    include_source: bool,
}

fn canonical_call(input: &str) -> Option<Call<'_>> {
    let mut scanner = CanonicalScanner::new(input);
    scanner.literal(r#"{"jsonrpc":"2.0","id":"#)?;
    let id = scanner.plain_string()?;
    scanner.literal(r#","method":"tools/call","params":{"name":"#)?;
    let name = scanner.plain_string()?;
    scanner.literal(r#","arguments":{"query":"#)?;
    let query = scanner.plain_string()?;
    scanner.literal(r#","limit":"#)?;
    let limit = scanner.unsigned()?;
    scanner.literal(r#","include_source":"#)?;
    let include_source = scanner.boolean()?;
    scanner.literal("}}}")?;
    scanner.is_finished().then_some(Call {
        id,
        name,
        query,
        limit,
        include_source,
    })
}

#[test]
fn recognizes_the_complete_canonical_layout() {
    let input = r#"{"jsonrpc":"2.0","id":"req-7","method":"tools/call","params":{"name":"query_graph","arguments":{"query":"entry points","limit":20,"include_source":true}}}"#;
    assert_eq!(
        canonical_call(input),
        Some(Call {
            id: "req-7",
            name: "query_graph",
            query: "entry points",
            limit: 20,
            include_source: true,
        })
    );
}

#[test]
fn mismatch_is_a_fallback_signal_not_partial_success() {
    for (input, valid_general_json) in [
        (
            r#"{"jsonrpc": "2.0","id":"req-7","method":"tools/call","params":{"name":"query_graph","arguments":{"query":"entry points","limit":20,"include_source":true}}}"#,
            true,
        ),
        (
            r#"{"jsonrpc":"2.0","id":"req\u002d7","method":"tools/call","params":{"name":"query_graph","arguments":{"query":"entry points","limit":20,"include_source":true}}}"#,
            true,
        ),
        (
            r#"{"jsonrpc":"2.0","id":"req-7","method":"tools/call","params":{"name":"query_graph","arguments":{"query":"entry points","limit":020,"include_source":true}}}"#,
            false,
        ),
        (
            r#"{"jsonrpc":"2.0","id":"req-7","method":"tools/call","params":{"name":"query_graph","arguments":{"query":"entry points","limit":20,"include_source":true}}} trailing"#,
            false,
        ),
    ] {
        assert!(canonical_call(input).is_none());
        let mut fallback = JsonCursor::from_str(input);
        let parsed = fallback.object(|_| Ok(()));
        if valid_general_json {
            parsed.unwrap();
            fallback.end().unwrap();
        } else {
            assert!(parsed.and_then(|()| fallback.end()).is_err());
        }
    }
}

#[test]
fn invalid_utf8_never_becomes_a_borrowed_string() {
    let input = b"\"\xff\"";
    let mut scanner = CanonicalBytesScanner::new(input);
    assert!(scanner.plain_string().is_none());
    let mut scanner = CanonicalBytesScanner::new(input);
    assert!(scanner.plain_ascii_string().is_none());
}

#[test]
fn byte_input_is_validated_once_before_matching() {
    let mut scanner = CanonicalBytesScanner::new(br#"{"enabled":true}"#);
    scanner.literal(r#"{"enabled":"#).unwrap();
    assert_eq!(scanner.boolean(), Some(true));
    scanner.literal("}").unwrap();
    assert!(scanner.is_finished());
}
