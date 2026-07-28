use blazingly_json::{from_str, RawJson};
use serde::Deserialize;
use serde_json::value::RawValue;

#[derive(Debug, Deserialize)]
struct Envelope<'a> {
    jsonrpc: &'a str,
    #[serde(borrow)]
    id: RawJson<'a>,
    method: &'a str,
    #[serde(borrow)]
    params: RawJson<'a>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct Params<'a> {
    name: &'a str,
    limit: u64,
}

#[test]
fn borrows_exact_raw_values_and_defers_typed_work() {
    let input = r#"{"jsonrpc":"2.0","id":"req-7","method":"tools/call","ignored":[1,{"x":"y"}],"params": { "name":"query_graph", "limit":20 }}"#;
    let request: Envelope<'_> = from_str(input).unwrap();

    assert_eq!(request.jsonrpc, "2.0");
    assert_eq!(request.id.get(), r#""req-7""#);
    assert_eq!(request.method, "tools/call");
    assert_eq!(
        request.params.get(),
        r#"{ "name":"query_graph", "limit":20 }"#
    );
    assert_eq!(
        request.params.deserialize::<Params<'_>>().unwrap(),
        Params {
            name: "query_graph",
            limit: 20
        }
    );
}

#[test]
fn raw_capture_validates_nested_json_without_allocating_a_dom() {
    for malformed in [
        r#"{"value":[1,]}"#,
        r#"{"value":{"key":}}"#,
        r#"{"value":"\uD800"}"#,
        r#"{"value":"line
break"}"#,
    ] {
        assert!(
            from_str::<RawJson<'_>>(malformed).is_err(),
            "{malformed:?} must be rejected"
        );
    }
}

#[test]
fn raw_capture_matches_serde_json_for_supported_values() {
    for fixture in [
        "null",
        "true",
        "-9223372036854775808",
        r#""escaped\ntext \u2764""#,
        r#"[1,2,{"nested":[false,null]}]"#,
        r#"{"query":"entry points","limit":20,"include_source":true}"#,
    ] {
        let ours = from_str::<RawJson<'_>>(fixture).unwrap();
        let reference = serde_json::from_str::<&RawValue>(fixture).unwrap();
        assert_eq!(ours.get(), reference.get(), "fixture: {fixture}");
    }
}
