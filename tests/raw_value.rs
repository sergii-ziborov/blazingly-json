use blazingly_json::value::{to_raw_value, RawValue};
use blazingly_json::Value;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct BorrowedEnvelope<'a> {
    #[serde(borrow)]
    payload: &'a RawValue,
}

#[derive(Serialize)]
struct BorrowedOutput<'a> {
    payload: &'a RawValue,
}

#[test]
fn borrowed_raw_value_is_zero_copy_and_preserves_formatting() {
    let input = r#"{"payload": { "nested": [1, 2, 3], "ok": true }}"#;
    let envelope: BorrowedEnvelope<'_> = blazingly_json::from_str(input).unwrap();

    assert_eq!(
        envelope.payload.get(),
        r#"{ "nested": [1, 2, 3], "ok": true }"#
    );

    let input_start = input.as_ptr() as usize;
    let input_end = input_start + input.len();
    let raw_start = envelope.payload.get().as_ptr() as usize;
    let raw_end = raw_start + envelope.payload.get().len();
    assert!(raw_start >= input_start);
    assert!(raw_end <= input_end);
}

#[test]
fn raw_value_interoperates_with_serde_json_in_both_directions() {
    let json = r#"{ "answer": 42, "items": [true, null] }"#;

    let ours_with_serde: &RawValue = serde_json::from_str(json).unwrap();
    let theirs_with_ours: &serde_json::value::RawValue = blazingly_json::from_str(json).unwrap();

    assert_eq!(ours_with_serde.get(), json);
    assert_eq!(theirs_with_ours.get(), json);
    assert_eq!(serde_json::to_string(ours_with_serde).unwrap(), json);
    assert_eq!(blazingly_json::to_string(theirs_with_ours).unwrap(), json);
}

#[test]
fn borrowed_raw_value_serializes_verbatim_in_compact_and_pretty_output() {
    let raw: &RawValue = blazingly_json::from_str(r#"{ "x": [1, 2] }"#).unwrap();
    let output = BorrowedOutput { payload: raw };

    assert_eq!(
        blazingly_json::to_string(&output).unwrap(),
        r#"{"payload":{ "x": [1, 2] }}"#
    );
    assert_eq!(
        blazingly_json::to_string_pretty(&output).unwrap(),
        "{\n  \"payload\": { \"x\": [1, 2] }\n}"
    );

    assert_eq!(raw.to_vec(), br#"{ "x": [1, 2] }"#);
    let mut direct = Vec::new();
    raw.write_to(&mut direct).unwrap();
    assert_eq!(direct, raw.to_vec());
}

#[test]
fn boxed_raw_value_matches_serde_json_and_round_trips_allocation() {
    let mut json = String::with_capacity(64);
    json.push_str(r#"{"method":"tools/call","id":7}"#);
    json.shrink_to_fit();
    let original_pointer = json.as_ptr();

    let raw = RawValue::from_string(json).unwrap();
    assert_eq!(raw.get(), r#"{"method":"tools/call","id":7}"#);
    assert_eq!(raw.get().as_ptr(), original_pointer);

    let json = raw.into_string();
    assert_eq!(json.as_ptr(), original_pointer);
    assert_eq!(json, r#"{"method":"tools/call","id":7}"#);

    let ours: Box<RawValue> = blazingly_json::from_str("[1, 2, 3]").unwrap();
    let reference: Box<serde_json::value::RawValue> = serde_json::from_str("[1, 2, 3]").unwrap();
    assert_eq!(ours.get(), reference.get());
}

#[test]
fn from_string_matches_reference_validation_and_whitespace_behavior() {
    let cases = [
        "null",
        " true ",
        "\n{\"a\": 1, \"b\": [2, 3]}\t",
        r#""escaped\nstring""#,
        "-12.5e3",
        "",
        "null trailing",
        "{]",
    ];

    for input in cases {
        let ours = RawValue::from_string(input.to_owned());
        let reference = serde_json::value::RawValue::from_string(input.to_owned());
        assert_eq!(
            ours.as_ref().ok().map(|value| value.get()),
            reference.as_ref().ok().map(|value| value.get()),
            "input: {input:?}"
        );
        assert_eq!(ours.is_ok(), reference.is_ok(), "input: {input:?}");
    }
}

#[test]
fn constants_clone_default_and_box_conversion_match_expected_values() {
    assert_eq!(RawValue::NULL.get(), "null");
    assert_eq!(RawValue::TRUE.get(), "true");
    assert_eq!(RawValue::FALSE.get(), "false");
    assert_eq!(Box::<RawValue>::default().get(), "null");

    let raw = RawValue::from_string(r#"{"x":1}"#.to_owned()).unwrap();
    let cloned = raw.clone();
    assert_eq!(cloned.get(), raw.get());
    assert_ne!(cloned.get().as_ptr(), raw.get().as_ptr());

    let boxed_str: Box<str> = raw.into();
    assert_eq!(&*boxed_str, r#"{"x":1}"#);
}

#[test]
fn to_raw_value_and_to_value_preserve_json_semantics() {
    #[derive(Serialize)]
    struct Payload<'a> {
        method: &'a str,
        ids: [u64; 3],
    }

    let raw = to_raw_value(&Payload {
        method: "query_graph",
        ids: [1, 2, 3],
    })
    .unwrap();

    assert_eq!(raw.get(), r#"{"method":"query_graph","ids":[1,2,3]}"#);
    assert_eq!(
        blazingly_json::to_value(&raw).unwrap(),
        blazingly_json::json!({"method": "query_graph", "ids": [1, 2, 3]})
    );

    let deserialized: Value = Deserialize::deserialize(&*raw).unwrap();
    assert_eq!(
        deserialized,
        blazingly_json::json!({"method": "query_graph", "ids": [1, 2, 3]})
    );
}

#[test]
fn raw_value_rejects_invalid_json_and_trailing_data() {
    for invalid in ["", "{", "[1,]", "true false", "\"unterminated"] {
        assert!(blazingly_json::from_str::<&RawValue>(invalid).is_err());
        assert!(blazingly_json::from_str::<Box<RawValue>>(invalid).is_err());
    }

    let invalid_utf8 = [b'"', 0xff, b'"'];
    assert!(blazingly_json::from_slice::<&RawValue>(&invalid_utf8).is_err());
}

#[test]
fn raw_value_reference_has_the_same_fat_pointer_size_as_str() {
    assert_eq!(
        std::mem::size_of::<&RawValue>(),
        std::mem::size_of::<&str>()
    );
    assert_eq!(
        std::mem::align_of::<&RawValue>(),
        std::mem::align_of::<&str>()
    );
}

#[test]
fn raw_value_deserializer_preserves_typed_enum_and_tuple_entry_points() {
    #[derive(Debug, Deserialize, PartialEq)]
    enum Event {
        Ping,
        Data { id: u64 },
    }

    let event_raw: &RawValue = blazingly_json::from_str(r#"{"Data":{"id":17}}"#).unwrap();
    let event = Event::deserialize(event_raw).unwrap();
    assert_eq!(event, Event::Data { id: 17 });

    let tuple_raw: &RawValue = blazingly_json::from_str(r#"["query_graph",20]"#).unwrap();
    let tuple = <(String, u64)>::deserialize(tuple_raw).unwrap();
    assert_eq!(tuple, ("query_graph".to_owned(), 20));
}

#[test]
fn strict_raw_fast_paths_fall_back_without_changing_semantics() {
    let cases = [
        "[]",
        "[0,-0,1.25,3e8,-4.5E-2]",
        r#"[{"id":1,"ok":true,"name":"a"},{"id":2,"ok":false,"name":"b"}]"#,
        r#"[{"escaped":"line\nbreak","nested":[1,2,3]}]"#,
        r#"[ { "id": 1, "nested": { "x": true } } ]"#,
        "[01]",
        r#"[{"id":1,}]"#,
        r#"[{"unterminated":"value}]"#,
    ];

    for input in cases {
        let ours = blazingly_json::from_str::<&RawValue>(input);
        let reference = serde_json::from_str::<&serde_json::value::RawValue>(input);
        assert_eq!(ours.is_ok(), reference.is_ok(), "input: {input:?}");
        if let (Ok(ours), Ok(reference)) = (ours, reference) {
            assert_eq!(ours.get(), reference.get(), "input: {input:?}");
        }
    }
}
