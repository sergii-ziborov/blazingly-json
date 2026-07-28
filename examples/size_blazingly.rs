use serde::Deserialize;

#[derive(Deserialize)]
struct Request {
    jsonrpc: String,
    id: u64,
    method: String,
}

fn main() {
    let request: Request =
        blazingly_json::from_str(r#"{"jsonrpc":"2.0","id":7,"method":"tools/call"}"#).unwrap();
    println!("{}:{}:{}", request.jsonrpc, request.id, request.method);
}
