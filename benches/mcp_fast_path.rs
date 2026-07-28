use blazingly_json::{CanonicalScanner, Cursor, RawJson, Value};
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

#[derive(Debug, Deserialize)]
struct ReferenceTypedRequest<'a> {
    #[serde(borrow)]
    id: &'a RawValue,
    method: &'a str,
    #[serde(borrow)]
    params: ReferenceTypedParams<'a>,
}

#[derive(Debug, Deserialize)]
struct ReferenceTypedParams<'a> {
    name: &'a str,
    #[serde(borrow)]
    arguments: ReferenceArguments<'a>,
}

#[derive(Debug, Deserialize)]
struct ReferenceArguments<'a> {
    query: &'a str,
    limit: u64,
    include_source: bool,
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

fn dispatch_cursor(input: &[u8]) {
    let mut id = None;
    let mut method = None;
    let mut protocol_version = None;
    let mut name = None;
    let mut arguments = None;
    let mut cursor = Cursor::from_slice(input);
    cursor
        .object(|request| {
            while let Some(field) = request.next_field()? {
                match field.name() {
                    "id" => id = Some(field.raw()?),
                    "method" => method = Some(field.string()?),
                    "params" => field.object(|params| {
                        while let Some(field) = params.next_field()? {
                            match field.name() {
                                "protocolVersion" => protocol_version = Some(field.string()?),
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

    black_box(id.map(RawJson::get));
    match method.as_deref() {
        Some("initialize") => {
            black_box(protocol_version);
        }
        Some("tools/call") => {
            black_box(name);
            let arguments = arguments.unwrap().deserialize::<Value>().unwrap();
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

fn dispatch_cursor_typed(input: &[u8]) {
    let mut id = None;
    let mut method = None;
    let mut name = None;
    let mut query = None;
    let mut limit = None;
    let mut include_source = None;
    let mut cursor = Cursor::from_slice(input);
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
                                "arguments" => field.object(|arguments| {
                                    while let Some(field) = arguments.next_field()? {
                                        match field.name() {
                                            "query" => query = Some(field.string()?),
                                            "limit" => limit = Some(field.deserialize::<u64>()?),
                                            "include_source" => {
                                                include_source = Some(field.deserialize::<bool>()?);
                                            }
                                            _ => field.skip()?,
                                        }
                                    }
                                    Ok(())
                                })?,
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
    black_box((id, method, name, query, limit, include_source));
}

fn dispatch_reference_typed(input: &[u8]) {
    let request = serde_json::from_slice::<ReferenceTypedRequest<'_>>(input).unwrap();
    black_box((
        request.id.get(),
        request.method,
        request.params.name,
        request.params.arguments.query,
        request.params.arguments.limit,
        request.params.arguments.include_source,
    ));
}

fn dispatch_canonical_typed(input: &[u8]) -> bool {
    let Some((id, name, query, limit, include_source)) = (|| {
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
        scanner
            .is_finished()
            .then_some((id, name, query, limit, include_source))
    })() else {
        dispatch_cursor_typed(input);
        return false;
    };

    black_box((id, name, query, limit, include_source));
    true
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
    const ROUNDS: u32 = 24;

    dispatch_cursor(input);
    dispatch_fast(input);
    dispatch_reference_raw(input);
    dispatch_current_mcport(input);

    let mut cursor = Vec::with_capacity(ROUNDS as usize);
    let mut fast = Vec::with_capacity(ROUNDS as usize);
    let mut raw = Vec::with_capacity(ROUNDS as usize);
    let mut current = Vec::with_capacity(ROUNDS as usize);
    for round in 0..ROUNDS {
        let mut cursor_path = || dispatch_cursor(black_box(input));
        let mut ours = || dispatch_fast(black_box(input));
        let mut reference_raw = || dispatch_reference_raw(black_box(input));
        let mut reference_current = || dispatch_current_mcport(black_box(input));
        let mut times = [Duration::default(); 4];
        match round % 4 {
            0 => {
                times[0] = batch(ITERATIONS, &mut cursor_path);
                times[1] = batch(ITERATIONS, &mut ours);
                times[2] = batch(ITERATIONS, &mut reference_raw);
                times[3] = batch(ITERATIONS, &mut reference_current);
            }
            1 => {
                times[3] = batch(ITERATIONS, &mut reference_current);
                times[0] = batch(ITERATIONS, &mut cursor_path);
                times[1] = batch(ITERATIONS, &mut ours);
                times[2] = batch(ITERATIONS, &mut reference_raw);
            }
            2 => {
                times[2] = batch(ITERATIONS, &mut reference_raw);
                times[3] = batch(ITERATIONS, &mut reference_current);
                times[0] = batch(ITERATIONS, &mut cursor_path);
                times[1] = batch(ITERATIONS, &mut ours);
            }
            _ => {
                times[1] = batch(ITERATIONS, &mut ours);
                times[2] = batch(ITERATIONS, &mut reference_raw);
                times[3] = batch(ITERATIONS, &mut reference_current);
                times[0] = batch(ITERATIONS, &mut cursor_path);
            }
        }
        cursor.push(times[0]);
        fast.push(times[1]);
        raw.push(times[2]);
        current.push(times[3]);
    }

    let per_iteration =
        |duration: Duration| duration.as_secs_f64() * 1_000_000_000.0 / f64::from(ITERATIONS);
    let mut cursor = cursor.into_iter().map(per_iteration).collect::<Vec<_>>();
    let mut fast = fast.into_iter().map(per_iteration).collect::<Vec<_>>();
    let mut raw = raw.into_iter().map(per_iteration).collect::<Vec<_>>();
    let mut current = current.into_iter().map(per_iteration).collect::<Vec<_>>();
    let cursor = median(&mut cursor);
    let fast = median(&mut fast);
    let raw = median(&mut raw);
    let current = median(&mut current);

    println!(
        "{name:<12} cursor={cursor:>9.2} ns raw-derive={fast:>9.2} ns serde-raw={raw:>9.2} ns current={current:>9.2} ns serde/cursor={:>5.2}x current/cursor={:>5.2}x",
        raw / cursor,
        current / cursor
    );
}

fn compare_typed_tool() {
    const ITERATIONS: u32 = 50_000;
    const ROUNDS: u32 = 24;
    dispatch_cursor_typed(TOOL_CALL);
    dispatch_reference_typed(TOOL_CALL);
    assert!(dispatch_canonical_typed(TOOL_CALL));

    let mut cursor_samples = Vec::with_capacity(ROUNDS as usize);
    let mut reference_samples = Vec::with_capacity(ROUNDS as usize);
    let mut canonical_samples = Vec::with_capacity(ROUNDS as usize);
    for round in 0..ROUNDS {
        let mut cursor = || dispatch_cursor_typed(black_box(TOOL_CALL));
        let mut reference = || dispatch_reference_typed(black_box(TOOL_CALL));
        let mut canonical = || {
            assert!(dispatch_canonical_typed(black_box(TOOL_CALL)));
        };
        let mut times = [Duration::default(); 3];
        for offset in 0..3 {
            let slot = (round as usize + offset) % 3;
            times[slot] = match slot {
                0 => batch(ITERATIONS, &mut canonical),
                1 => batch(ITERATIONS, &mut cursor),
                _ => batch(ITERATIONS, &mut reference),
            };
        }
        canonical_samples.push(times[0].as_secs_f64() * 1e9 / f64::from(ITERATIONS));
        cursor_samples.push(times[1].as_secs_f64() * 1e9 / f64::from(ITERATIONS));
        reference_samples.push(times[2].as_secs_f64() * 1e9 / f64::from(ITERATIONS));
    }
    let canonical = median(&mut canonical_samples);
    let cursor = median(&mut cursor_samples);
    let reference = median(&mut reference_samples);
    println!(
        "{:<12} canonical={canonical:>9.2} ns cursor={cursor:>9.2} ns serde-typed={reference:>9.2} ns serde/canonical={:>5.2}x cursor/canonical={:>5.2}x",
        "typed/call",
        reference / canonical,
        cursor / canonical
    );
}

fn main() {
    compare("ping", PING);
    compare("initialize", INITIALIZE);
    compare("tools/call", TOOL_CALL);
    compare_typed_tool();
}
