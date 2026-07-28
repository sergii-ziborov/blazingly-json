use blazingly_json::{RawJson, Value};
use serde::Deserialize;
use stats_alloc::{Region, Stats, StatsAlloc, INSTRUMENTED_SYSTEM};
use std::alloc::System;
use std::hint::black_box;

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

#[derive(Debug, Deserialize)]
struct FastRequest<'a> {
    #[serde(default, borrow)]
    id: Option<RawJson<'a>>,
    method: &'a str,
    #[serde(default, borrow)]
    params: Option<FastParams<'a>>,
}

#[derive(Debug, Deserialize)]
struct FastParams<'a> {
    #[serde(default)]
    name: Option<&'a str>,
    #[serde(default, borrow)]
    arguments: Option<RawJson<'a>>,
}

const PING: &[u8] = br#"{"jsonrpc":"2.0","id":17,"method":"ping"}"#;
const TOOL_CALL: &[u8] = br#"{"jsonrpc":"2.0","id":"req-7","method":"tools/call","params":{"name":"query_graph","arguments":{"query":"entry points","limit":20,"include_source":true}}}"#;

fn dispatch_fast(input: &[u8]) {
    let request: FastRequest<'_> = blazingly_json::from_slice(input).unwrap();
    black_box(request.id.map(RawJson::get));
    black_box(request.method);
    if let Some(params) = request.params {
        black_box(params.name);
        if let Some(arguments) = params.arguments {
            black_box(arguments.deserialize::<Value>().unwrap());
        }
    }
}

fn dispatch_current_mcport(input: &[u8]) {
    let request = serde_json::from_slice::<serde_json::Value>(input).unwrap();
    black_box(request.get("id").cloned());
    black_box(request.get("method").and_then(serde_json::Value::as_str));
    if let Some(arguments) = request.pointer("/params/arguments") {
        black_box(arguments.clone());
    }
}

fn bounded_f64(value: usize) -> f64 {
    f64::from(u32::try_from(value).expect("benchmark value fits u32"))
}

fn report(name: &str, stats: Stats, iterations: usize) {
    let iterations = bounded_f64(iterations);
    println!(
        "{name:<24} allocations/call={:>6.2} bytes/call={:>8.2}",
        bounded_f64(stats.allocations) / iterations,
        bounded_f64(stats.bytes_allocated) / iterations
    );
}

fn compare(name: &str, input: &[u8]) {
    const ITERATIONS: usize = 100_000;
    println!("{name}:");

    let ours = Region::new(GLOBAL);
    for _ in 0..ITERATIONS {
        dispatch_fast(black_box(input));
    }
    report("blazingly fast path", ours.change(), ITERATIONS);

    let current = Region::new(GLOBAL);
    for _ in 0..ITERATIONS {
        dispatch_current_mcport(black_box(input));
    }
    report("current mcport", current.change(), ITERATIONS);
}

fn main() {
    compare("ping", PING);
    compare("tools/call", TOOL_CALL);
}
