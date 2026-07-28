use blazingly_json::{
    Value, from_slice, from_str, from_value, json, to_string, to_string_pretty, to_value, to_vec,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Deserialize, PartialEq, Serialize)]
struct ToolCall {
    jsonrpc: String,
    id: RequestId,
    method: String,
    params: ToolParams,
}

#[derive(Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
enum RequestId {
    Number(u64),
    String(String),
}

#[derive(Debug, Deserialize, PartialEq, Serialize)]
struct ToolParams {
    name: String,
    arguments: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, PartialEq, Serialize)]
enum Event {
    Ready,
    Progress { current: u64, total: u64 },
    Failed(String),
}

const VALID_FIXTURES: &[&str] = &[
    "null",
    "true",
    "false",
    "0",
    "-0",
    "18446744073709551615",
    "-9223372036854775808",
    "1.25e-3",
    r#""""#,
    r#""plain ASCII""#,
    r#""escaped\nline\t\u20ac""#,
    r#""surrogate \ud83d\ude80""#,
    "[]",
    "{}",
    r#"{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}"#,
    r#"{"a":[null,true,false,1,-2,3.5,"x"],"b":{"c":"d"}}"#,
];

const INVALID_FIXTURES: &[&str] = &[
    "",
    "nul",
    "True",
    "01",
    "-",
    "1.",
    "1e",
    "1e+",
    "[1,]",
    r#"{"a":1,}"#,
    r#"{"a" 1}"#,
    r#""unterminated"#,
    "\"control\ncharacter\"",
    r#""\x""#,
    r#""\ud800""#,
    r#""\udc00""#,
    "null true",
];

#[test]
fn valid_value_corpus_matches_serde_json() {
    for fixture in VALID_FIXTURES {
        let ours: Value = from_str(fixture).unwrap_or_else(|error| {
            panic!("blazingly-json rejected {fixture:?}: {error}");
        });
        let reference: serde_json::Value = serde_json::from_str(fixture).unwrap();

        let ours_as_reference: serde_json::Value =
            serde_json::from_str(&to_string(&ours).unwrap()).unwrap();
        assert_eq!(ours_as_reference, reference, "fixture: {fixture}");

        let reference_as_ours: Value =
            from_str(&serde_json::to_string(&reference).unwrap()).unwrap();
        assert_eq!(reference_as_ours, ours, "fixture: {fixture}");
    }
}

#[test]
fn invalid_value_corpus_is_rejected_by_both_engines() {
    for fixture in INVALID_FIXTURES {
        assert!(
            from_str::<Value>(fixture).is_err(),
            "blazingly-json accepted {fixture:?}"
        );
        assert!(
            serde_json::from_str::<serde_json::Value>(fixture).is_err(),
            "serde_json accepted {fixture:?}"
        );
    }
}

#[test]
fn typed_mcp_round_trip_matches_reference_bytes_semantically() {
    let fixture = br#"{"jsonrpc":"2.0","id":"req-7","method":"tools/call","params":{"name":"query_graph","arguments":{"query":"entry points","limit":"20"}}}"#;
    let ours: ToolCall = from_slice(fixture).unwrap();
    let reference: ToolCall = serde_json::from_slice(fixture).unwrap();
    assert_eq!(ours, reference);

    let ours_json: serde_json::Value = serde_json::from_slice(&to_vec(&ours).unwrap()).unwrap();
    let reference_json = serde_json::to_value(&reference).unwrap();
    assert_eq!(ours_json, reference_json);
}

#[test]
fn borrowed_strings_stay_borrowed_when_unescaped() {
    #[derive(Deserialize)]
    struct Borrowed<'a> {
        name: &'a str,
    }

    let input = r#"{"name":"query_graph"}"#;
    let decoded: Borrowed<'_> = from_str(input).unwrap();
    assert_eq!(decoded.name, "query_graph");
    let start = input.as_ptr() as usize;
    let end = start + input.len();
    let pointer = decoded.name.as_ptr() as usize;
    assert!((start..end).contains(&pointer));
}

#[test]
fn value_mutation_pointer_and_index_surface_matches_consumers() {
    let mut value = json!({
        "result": {
            "items": [{"id": 1}, {"id": 2}],
            "ok": true
        }
    });
    assert_eq!(value.pointer("/result/items/1/id"), Some(&json!(2)));
    value["result"]["items"][0]["id"] = json!(9);
    assert_eq!(value["result"]["items"][0]["id"], 9);
    assert!(value["missing"].is_null());
    assert_eq!(
        value.pointer("/result/ok").and_then(Value::as_bool),
        Some(true)
    );
}

#[test]
fn direct_value_bridge_supports_structs_and_enums() {
    let event = Event::Progress {
        current: 3,
        total: 10,
    };
    let value = to_value(&event).unwrap();
    let decoded: Event = from_value(value.clone()).unwrap();
    assert_eq!(decoded, event);

    let reference = serde_json::to_value(&event).unwrap();
    let converted: serde_json::Value = serde_json::from_str(&to_string(&value).unwrap()).unwrap();
    assert_eq!(converted, reference);
}

#[test]
fn duplicate_object_keys_use_last_value() {
    let value: Value = from_str(r#"{"key":1,"key":2}"#).unwrap();
    assert_eq!(value["key"], 2);
}

#[test]
fn syntax_errors_report_a_location() {
    let error = from_str::<Value>("{\n  \"key\": ]").unwrap_err();
    assert_eq!(error.line(), 2);
    assert!(error.column() > 0);
}

#[test]
fn numeric_representation_matches_the_supported_serde_json_domain() {
    let constructed = json!(1_i64);
    let parsed: Value = from_str("1").unwrap();
    assert_eq!(constructed, parsed);
    assert!(parsed.is_i64());
    assert!(parsed.is_u64());
    assert!(!parsed.is_f64());

    let large_integer: Value = from_str("9007199254740993").unwrap();
    let rounded_float: Value = from_str("9007199254740992.0").unwrap();
    assert_ne!(large_integer, rounded_float);

    let negative_zero: Value = from_str("-0").unwrap();
    assert!(negative_zero.is_f64());
    assert_eq!(to_string(&negative_zero).unwrap(), "-0.0");
}

#[test]
fn pretty_output_matches_serde_json_for_consumer_shapes() {
    let value = json!({
        "jsonrpc": "2.0",
        "result": {
            "items": [1, 2],
            "ok": true
        }
    });
    let ours = to_string_pretty(&value).unwrap();
    let reference_value: serde_json::Value =
        serde_json::from_str(&to_string(&value).unwrap()).unwrap();
    let reference = serde_json::to_string_pretty(&reference_value).unwrap();
    assert_eq!(ours, reference);
}

#[test]
fn non_finite_float_serialization_matches_serde_json() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            to_string(&value).unwrap(),
            serde_json::to_string(&value).unwrap()
        );
    }
}

#[test]
fn invalid_utf8_is_rejected_without_panicking() {
    let invalid = b"{\"value\":\"\xff\"}";
    assert!(from_slice::<Value>(invalid).is_err());
    assert!(serde_json::from_slice::<serde_json::Value>(invalid).is_err());
}

#[test]
fn difficult_float_round_trips_to_the_same_bits() {
    let reference = 1.018_054_596_033_137_3e179_f64;
    let encoded = serde_json::to_string(&reference).unwrap();
    let ours: Value = from_str(&encoded).unwrap();
    let decoded = ours.as_f64().unwrap();
    assert_eq!(
        decoded.to_bits(),
        reference.to_bits(),
        "input={encoded}, reencoded={}",
        to_string(&ours).unwrap()
    );
    let reencoded = to_string(&ours).unwrap();
    let final_value: Value = from_str(&reencoded).unwrap();
    assert_eq!(
        final_value.as_f64().unwrap().to_bits(),
        reference.to_bits(),
        "input={encoded}, reencoded={reencoded}"
    );
}
