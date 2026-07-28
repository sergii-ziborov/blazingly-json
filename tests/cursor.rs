use blazingly_json::{Cursor, RawJson, Value};
use std::borrow::Cow;

#[test]
fn cursor_routes_nested_mcp_fields_without_a_dom() {
    let input = r#"{"jsonrpc":"2.0","id":"req-7","method":"tools/call","ignored":[1,2,3],"params":{"name":"query_graph","arguments":{"query":"entry points","limit":20}}}"#;
    let mut id = None;
    let mut method = None;
    let mut name = None;
    let mut arguments = None;

    let mut cursor = Cursor::from_str(input);
    cursor
        .object(|request| {
            while let Some(field) = request.next_field()? {
                match field.name() {
                    "id" => id = Some(field.raw()?),
                    "method" => method = Some(field.string()?),
                    "params" => field.object(|params| {
                        while let Some(field) = params.next_field()? {
                            match field.name() {
                                "name" => name = Some(field.string()?),
                                "arguments" => arguments = Some(field.raw()?),
                                _ => field.skip()?,
                            }
                        }
                        Ok(())
                    })?,
                    _ => field.skip()?,
                }
            }
            Ok(())
        })
        .unwrap();
    cursor.end().unwrap();

    assert_eq!(id.map(RawJson::get), Some(r#""req-7""#));
    assert_eq!(method, Some(Cow::Borrowed("tools/call")));
    assert_eq!(name, Some(Cow::Borrowed("query_graph")));
    let arguments = arguments.unwrap();
    assert_eq!(
        arguments.deserialize::<Value>().unwrap()["limit"].as_u64(),
        Some(20)
    );
}

#[test]
fn cursor_validates_unvisited_fields_and_trailing_input() {
    let mut malformed = Cursor::from_str(r#"{"wanted":1,"ignored":[1,]}"#);
    assert!(malformed.object(|_| Ok(())).is_err());

    let mut trailing = Cursor::from_str(r#"{"wanted":1} false"#);
    trailing.object(|_| Ok(())).unwrap();
    assert!(trailing.end().is_err());
}
