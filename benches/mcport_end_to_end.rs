use blazingly_json::{json, RawJson, Value};
use serde::de::{Deserializer, Visitor};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::hint::black_box;
use std::marker::PhantomData;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(untagged)]
enum RequestId<'a> {
    String(&'a str),
    Unsigned(u64),
    Signed(i64),
    Null,
}

struct RequestIdVisitor<'a>(PhantomData<&'a str>);

impl<'de: 'a, 'a> Visitor<'de> for RequestIdVisitor<'a> {
    type Value = RequestId<'a>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON-RPC string, integer, or null id")
    }

    fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E> {
        Ok(RequestId::String(value))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(RequestId::Unsigned(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(RequestId::Signed(value))
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(RequestId::Null)
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for RequestId<'a> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(RequestIdVisitor(PhantomData))
    }
}

#[derive(Debug, Deserialize)]
struct FastRequest<'a> {
    #[serde(default, borrow)]
    id: Option<RequestId<'a>>,
    method: &'a str,
    #[serde(default, borrow)]
    params: Option<FastParams<'a>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FastParams<'a> {
    #[serde(default)]
    protocol_version: Option<&'a str>,
    #[serde(default)]
    name: Option<&'a str>,
    #[serde(default, borrow)]
    arguments: Option<RawJson<'a>>,
}

#[derive(Serialize)]
struct Response<'a, T> {
    jsonrpc: &'static str,
    id: RequestId<'a>,
    result: T,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InitializeResult<'a> {
    protocol_version: &'a str,
    capabilities: Capabilities,
    server_info: ServerInfo<'static>,
    instructions: &'static str,
}

#[derive(Serialize)]
struct Capabilities {
    tools: ToolsCapability,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolsCapability {
    list_changed: bool,
}

#[derive(Serialize)]
struct ServerInfo<'a> {
    name: &'a str,
    version: &'a str,
}

#[derive(Serialize)]
struct ToolList<'a> {
    tools: &'a Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolResult<'a> {
    content: [TextContent<'a>; 1],
    is_error: bool,
    structured_content: &'a Value,
}

#[derive(Serialize)]
struct TextContent<'a> {
    r#type: &'static str,
    text: &'a str,
}

const PING: &[u8] = br#"{"jsonrpc":"2.0","id":17,"method":"ping"}"#;
const INITIALIZE: &[u8] = br#"{"jsonrpc":"2.0","id":"init-1","method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"Codex","version":"1.0"}}}"#;
const TOOLS_LIST: &[u8] = br#"{"jsonrpc":"2.0","id":18,"method":"tools/list"}"#;
const TOOL_CALL: &[u8] = br#"{"jsonrpc":"2.0","id":"req-7","method":"tools/call","params":{"name":"query_graph","arguments":{"query":"entry points","limit":20,"include_source":true}}}"#;

fn catalog() -> Value {
    json!([{
        "name": "query_graph",
        "description": "Query the code graph.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "limit": {"type": "integer"}
            }
        }
    }])
}

fn current_catalog() -> serde_json::Value {
    serde_json::json!([{
        "name": "query_graph",
        "description": "Query the code graph.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "limit": {"type": "integer"}
            }
        }
    }])
}

fn current_mcport(input: &[u8]) -> Vec<u8> {
    let request = serde_json::from_slice::<serde_json::Value>(input).unwrap();
    let id = request
        .get("id")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let method = request
        .get("method")
        .and_then(serde_json::Value::as_str)
        .unwrap();
    let result = match method {
        "ping" => serde_json::json!({}),
        "initialize" => serde_json::json!({
            "protocolVersion": request
                .pointer("/params/protocolVersion")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("2025-06-18"),
            "capabilities": {"tools": {"listChanged": false}},
            "serverInfo": {"name": "bench", "version": "1.0.0"},
            "instructions": "Benchmark server."
        }),
        "tools/list" => serde_json::json!({"tools": current_catalog()}),
        "tools/call" => {
            let value = request
                .pointer("/params/arguments")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            let text = serde_json::to_string_pretty(&value).unwrap();
            serde_json::json!({
                "content": [{"type": "text", "text": text}],
                "isError": false,
                "structuredContent": value
            })
        }
        _ => unreachable!(),
    };
    serde_json::to_vec(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    }))
    .unwrap()
}

fn fast_mcport(input: &[u8]) -> Vec<u8> {
    let request = blazingly_json::from_slice::<FastRequest<'_>>(input).unwrap();
    let id = request.id.unwrap_or(RequestId::Null);
    match request.method {
        "ping" => blazingly_json::to_vec(&Response {
            jsonrpc: "2.0",
            id,
            result: Empty {},
        })
        .unwrap(),
        "initialize" => {
            let version = request
                .params
                .and_then(|params| params.protocol_version)
                .unwrap_or("2025-06-18");
            blazingly_json::to_vec(&Response {
                jsonrpc: "2.0",
                id,
                result: InitializeResult {
                    protocol_version: version,
                    capabilities: Capabilities {
                        tools: ToolsCapability {
                            list_changed: false,
                        },
                    },
                    server_info: ServerInfo {
                        name: "bench",
                        version: "1.0.0",
                    },
                    instructions: "Benchmark server.",
                },
            })
            .unwrap()
        }
        "tools/list" => {
            let catalog = catalog();
            blazingly_json::to_vec(&Response {
                jsonrpc: "2.0",
                id,
                result: ToolList { tools: &catalog },
            })
            .unwrap()
        }
        "tools/call" => {
            let params = request.params.unwrap();
            black_box(params.name);
            let value = params.arguments.unwrap().deserialize::<Value>().unwrap();
            let text = blazingly_json::to_string_pretty(&value).unwrap();
            blazingly_json::to_vec(&Response {
                jsonrpc: "2.0",
                id,
                result: ToolResult {
                    content: [TextContent {
                        r#type: "text",
                        text: &text,
                    }],
                    is_error: false,
                    structured_content: &value,
                },
            })
            .unwrap()
        }
        _ => unreachable!(),
    }
}

#[derive(Serialize)]
struct Empty {}

fn batch(iterations: u32, task: &mut impl FnMut()) -> Duration {
    let started = Instant::now();
    for _ in 0..iterations {
        task();
    }
    started.elapsed()
}

fn median(samples: &mut [f64]) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[samples.len() / 2]
}

fn compare(name: &str, input: &[u8]) {
    const ITERATIONS: u32 = 20_000;
    const ROUNDS: u32 = 21;

    let ours = fast_mcport(input);
    let current = current_mcport(input);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&ours).unwrap(),
        serde_json::from_slice::<serde_json::Value>(&current).unwrap()
    );

    let mut ours_samples = Vec::with_capacity(ROUNDS as usize);
    let mut current_samples = Vec::with_capacity(ROUNDS as usize);
    for round in 0..ROUNDS {
        let mut ours = || {
            black_box(fast_mcport(black_box(input)));
        };
        let mut current = || {
            black_box(current_mcport(black_box(input)));
        };
        let (ours_time, current_time) = if round % 2 == 0 {
            (
                batch(ITERATIONS, &mut ours),
                batch(ITERATIONS, &mut current),
            )
        } else {
            let current_time = batch(ITERATIONS, &mut current);
            let ours_time = batch(ITERATIONS, &mut ours);
            (ours_time, current_time)
        };
        ours_samples.push(ours_time.as_secs_f64() * 1e9 / f64::from(ITERATIONS));
        current_samples.push(current_time.as_secs_f64() * 1e9 / f64::from(ITERATIONS));
    }
    let ours = median(&mut ours_samples);
    let current = median(&mut current_samples);
    println!(
        "{name:<12} fast={ours:>9.2} ns current-mcport={current:>9.2} ns current/fast={:>5.2}x",
        current / ours
    );
}

fn main() {
    compare("ping", PING);
    compare("initialize", INITIALIZE);
    compare("tools/list", TOOLS_LIST);
    compare("tools/call", TOOL_CALL);
}
