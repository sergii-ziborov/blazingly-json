use blazingly_json::{RawJson, Value};
use serde::Deserialize;
use serde_json::value::RawValue;
use std::hint::black_box;
use std::time::{Duration, Instant};

#[derive(Debug, Deserialize)]
struct FastRequest<'a> {
    #[serde(default, borrow)]
    id: Option<RawJson<'a>>,
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

#[derive(Debug, Deserialize)]
struct ReferenceRequest<'a> {
    #[serde(default, borrow)]
    id: Option<&'a RawValue>,
    method: &'a str,
    #[serde(default, borrow)]
    params: Option<ReferenceParams<'a>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReferenceParams<'a> {
    #[serde(default)]
    protocol_version: Option<&'a str>,
    #[serde(default)]
    name: Option<&'a str>,
    #[serde(default, borrow)]
    arguments: Option<&'a RawValue>,
}

const PING: &[u8] = br#"{"jsonrpc":"2.0","id":17,"method":"ping"}"#;
const INITIALIZE: &[u8] = br#"{"jsonrpc":"2.0","id":"init-1","method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"Codex","version":"1.0"}}}"#;
const TOOL_CALL: &[u8] = br#"{"jsonrpc":"2.0","id":"req-7","method":"tools/call","params":{"name":"query_graph","arguments":{"query":"entry points","limit":20,"include_source":true}}}"#;

fn dispatch_fast(input: &[u8]) {
    let request: FastRequest<'_> = blazingly_json::from_slice(input).unwrap();
    black_box(request.id.map(RawJson::get));
    match request.method {
        "initialize" => {
            black_box(request.params.and_then(|params| params.protocol_version));
        }
        "tools/call" => {
            let params = request.params.unwrap();
            black_box(params.name);
            let arguments = params.arguments.unwrap().deserialize::<Value>().unwrap();
            black_box(arguments);
        }
        _ => {}
    }
}

fn dispatch_reference_raw(input: &[u8]) {
    let request: ReferenceRequest<'_> = serde_json::from_slice(input).unwrap();
    black_box(request.id.map(RawValue::get));
    match request.method {
        "initialize" => {
            black_box(request.params.and_then(|params| params.protocol_version));
        }
        "tools/call" => {
            let params = request.params.unwrap();
            black_box(params.name);
            let arguments =
                serde_json::from_str::<serde_json::Value>(params.arguments.unwrap().get()).unwrap();
            black_box(arguments);
        }
        _ => {}
    }
}

fn dispatch_current_mcport(input: &[u8]) {
    let request = serde_json::from_slice::<serde_json::Value>(input).unwrap();
    black_box(request.get("id").cloned());
    let method = request.get("method").and_then(serde_json::Value::as_str);
    match method {
        Some("initialize") => {
            black_box(
                request
                    .pointer("/params/protocolVersion")
                    .and_then(serde_json::Value::as_str),
            );
        }
        Some("tools/call") => {
            black_box(
                request
                    .pointer("/params/name")
                    .and_then(serde_json::Value::as_str),
            );
            black_box(
                request
                    .pointer("/params/arguments")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({})),
            );
        }
        _ => {}
    }
}

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
    const ITERATIONS: u32 = 50_000;
    const ROUNDS: u32 = 21;

    dispatch_fast(input);
    dispatch_reference_raw(input);
    dispatch_current_mcport(input);

    let mut fast = Vec::with_capacity(ROUNDS as usize);
    let mut raw = Vec::with_capacity(ROUNDS as usize);
    let mut current = Vec::with_capacity(ROUNDS as usize);
    for round in 0..ROUNDS {
        let mut ours = || dispatch_fast(black_box(input));
        let mut reference_raw = || dispatch_reference_raw(black_box(input));
        let mut reference_current = || dispatch_current_mcport(black_box(input));
        let times = match round % 3 {
            0 => [
                batch(ITERATIONS, &mut ours),
                batch(ITERATIONS, &mut reference_raw),
                batch(ITERATIONS, &mut reference_current),
            ],
            1 => [
                batch(ITERATIONS, &mut reference_current),
                batch(ITERATIONS, &mut ours),
                batch(ITERATIONS, &mut reference_raw),
            ],
            _ => [
                batch(ITERATIONS, &mut reference_raw),
                batch(ITERATIONS, &mut reference_current),
                batch(ITERATIONS, &mut ours),
            ],
        };
        match round % 3 {
            0 => {
                fast.push(times[0]);
                raw.push(times[1]);
                current.push(times[2]);
            }
            1 => {
                current.push(times[0]);
                fast.push(times[1]);
                raw.push(times[2]);
            }
            _ => {
                raw.push(times[0]);
                current.push(times[1]);
                fast.push(times[2]);
            }
        }
    }

    let per_iteration =
        |duration: Duration| duration.as_secs_f64() * 1_000_000_000.0 / f64::from(ITERATIONS);
    let mut fast = fast.into_iter().map(per_iteration).collect::<Vec<_>>();
    let mut raw = raw.into_iter().map(per_iteration).collect::<Vec<_>>();
    let mut current = current.into_iter().map(per_iteration).collect::<Vec<_>>();
    let fast = median(&mut fast);
    let raw = median(&mut raw);
    let current = median(&mut current);

    println!(
        "{name:<12} fast={fast:>9.2} ns serde-raw={raw:>9.2} ns current-mcport={current:>9.2} ns current/fast={:>5.2}x",
        current / fast
    );
}

fn main() {
    compare("ping", PING);
    compare("initialize", INITIALIZE);
    compare("tools/call", TOOL_CALL);
}
