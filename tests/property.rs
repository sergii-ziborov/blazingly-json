use blazingly_json::{Value, from_str, to_string};
use proptest::prelude::*;

fn leaf() -> impl Strategy<Value = serde_json::Value> {
    prop_oneof![
        Just(serde_json::Value::Null),
        any::<bool>().prop_map(serde_json::Value::Bool),
        any::<i64>().prop_map(|value| serde_json::Value::Number(value.into())),
        any::<u64>().prop_map(|value| serde_json::Value::Number(value.into())),
        prop::collection::vec(any::<char>(), 0..32)
            .prop_map(|characters| characters.into_iter().collect())
            .prop_map(serde_json::Value::String),
    ]
}

fn json_value() -> impl Strategy<Value = serde_json::Value> {
    leaf().prop_recursive(5, 128, 8, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..8).prop_map(serde_json::Value::Array),
            prop::collection::btree_map("[a-zA-Z_][a-zA-Z0-9_]{0,12}", inner, 0..8)
                .prop_map(|map| serde_json::Value::Object(map.into_iter().collect())),
        ]
    })
}

proptest! {
    #[test]
    fn supported_values_round_trip_against_serde_json(reference in json_value()) {
        let encoded = serde_json::to_string(&reference).unwrap();
        let ours: Value = from_str(&encoded).unwrap();
        let reencoded = to_string(&ours).unwrap();
        let final_value: serde_json::Value = serde_json::from_str(&reencoded).unwrap();
        prop_assert_eq!(final_value, reference);
    }
}
