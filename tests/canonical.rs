use blazingly_json::{CanonicalScanner, Cursor};

#[derive(Debug, Eq, PartialEq)]
struct Call<'a> {
    id: &'a str,
    name: &'a str,
    query: &'a str,
    limit: u64,
    include_source: bool,
}

fn canonical_call(input: &[u8]) -> Option<Call<'_>> {
    let mut scanner = CanonicalScanner::new(input);
    scanner.literal(br#"{"jsonrpc":"2.0","id":"#)?;
    let id = scanner.plain_string()?;
    scanner.literal(br#","method":"tools/call","params":{"name":"#)?;
    let name = scanner.plain_string()?;
    scanner.literal(br#","arguments":{"query":"#)?;
    let query = scanner.plain_string()?;
    scanner.literal(br#","limit":"#)?;
    let limit = scanner.unsigned()?;
    scanner.literal(br#","include_source":"#)?;
    let include_source = scanner.boolean()?;
    scanner.literal(b"}}}")?;
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
    let input = br#"{"jsonrpc":"2.0","id":"req-7","method":"tools/call","params":{"name":"query_graph","arguments":{"query":"entry points","limit":20,"include_source":true}}}"#;
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
            br#"{"jsonrpc": "2.0","id":"req-7","method":"tools/call","params":{"name":"query_graph","arguments":{"query":"entry points","limit":20,"include_source":true}}}"#
                .as_slice(),
            true,
        ),
        (
            br#"{"jsonrpc":"2.0","id":"req\u002d7","method":"tools/call","params":{"name":"query_graph","arguments":{"query":"entry points","limit":20,"include_source":true}}}"#
                .as_slice(),
            true,
        ),
        (
            br#"{"jsonrpc":"2.0","id":"req-7","method":"tools/call","params":{"name":"query_graph","arguments":{"query":"entry points","limit":020,"include_source":true}}}"#
                .as_slice(),
            false,
        ),
        (
            br#"{"jsonrpc":"2.0","id":"req-7","method":"tools/call","params":{"name":"query_graph","arguments":{"query":"entry points","limit":20,"include_source":true}}} trailing"#
                .as_slice(),
            false,
        ),
    ] {
        assert!(canonical_call(input).is_none());
        let mut fallback = Cursor::from_slice(input);
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
    let mut scanner = CanonicalScanner::new(input);
    assert!(scanner.plain_string().is_none());
    assert_eq!(scanner.remaining(), input);
}
