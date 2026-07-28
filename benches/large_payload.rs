use blazingly_json::Value;
use serde::Deserialize;
use std::hint::black_box;
use std::io::Write;
use std::time::{Duration, Instant};

#[derive(Debug, Deserialize)]
struct LargeRecord {
    id: u64,
    kind: String,
    path: String,
    score: f64,
}

fn million_u64_json() -> Vec<u8> {
    let mut json = Vec::with_capacity(7_000_001);
    json.push(b'[');
    for value in 0..1_000_000_u64 {
        if value != 0 {
            json.push(b',');
        }
        write!(json, "{value}").unwrap();
    }
    json.push(b']');
    json
}

fn million_record_json() -> Vec<u8> {
    let mut json = Vec::with_capacity(88_000_001);
    json.push(b'[');
    for id in 0..1_000_000_u64 {
        if id != 0 {
            json.push(b',');
        }
        write!(
            json,
            r#"{{"id":{id},"kind":"function","path":"src/module_{}/symbol_{id}.rs","score":0.99}}"#,
            id % 128
        )
        .unwrap();
    }
    json.push(b']');
    json
}

fn mcp_request(id: u64) -> Vec<u8> {
    format!(
        r#"{{"jsonrpc":"2.0","id":{id},"method":"tools/call","params":{{"name":"query_graph","arguments":{{"query":"entry points","limit":20,"include_source":true}}}}}}"#
    )
    .into_bytes()
}

fn batch(iterations: u32, task: &mut impl FnMut()) -> Duration {
    let started = Instant::now();
    for _ in 0..iterations {
        task();
    }
    started.elapsed()
}

fn median(samples: &mut [Duration]) -> Duration {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

fn bounded_f64(value: usize) -> f64 {
    f64::from(u32::try_from(value).expect("benchmark payload fits in u32"))
}

fn compare(
    name: &str,
    bytes_per_iteration: usize,
    iterations: u32,
    rounds: u32,
    mut ours: impl FnMut(),
    mut reference: impl FnMut(),
) {
    ours();
    reference();

    let mut ours_samples = Vec::with_capacity(rounds as usize);
    let mut reference_samples = Vec::with_capacity(rounds as usize);
    for round in 0..rounds {
        let (ours_time, reference_time) = if round % 2 == 0 {
            (
                batch(iterations, &mut ours),
                batch(iterations, &mut reference),
            )
        } else {
            let reference_time = batch(iterations, &mut reference);
            let ours_time = batch(iterations, &mut ours);
            (ours_time, reference_time)
        };
        ours_samples.push(ours_time);
        reference_samples.push(reference_time);
    }

    let ours_time = median(&mut ours_samples);
    let reference_time = median(&mut reference_samples);
    let total_bytes = bounded_f64(bytes_per_iteration) * f64::from(iterations);
    let ours_mib = total_bytes / ours_time.as_secs_f64() / (1024.0 * 1024.0);
    let reference_mib = total_bytes / reference_time.as_secs_f64() / (1024.0 * 1024.0);
    let advantage = (reference_time.as_secs_f64() / ours_time.as_secs_f64() - 1.0) * 100.0;
    println!("{name:<30} {ours_mib:>10.1} MiB/s {reference_mib:>10.1} MiB/s {advantage:>+8.2}%");
}

fn main() {
    let integers = million_u64_json();
    println!(
        "million u64 payload: {:.2} MiB",
        bounded_f64(integers.len()) / 1_048_576.0
    );
    compare(
        "1,000,000 typed u64",
        integers.len(),
        1,
        9,
        || {
            let values =
                blazingly_json::from_slice::<Vec<u64>>(black_box(integers.as_slice())).unwrap();
            black_box(values.len());
        },
        || {
            let values =
                serde_json::from_slice::<Vec<u64>>(black_box(integers.as_slice())).unwrap();
            black_box(values.len());
        },
    );

    let records = million_record_json();
    println!(
        "million record payload: {:.2} MiB",
        bounded_f64(records.len()) / 1_048_576.0
    );
    compare(
        "1,000,000 typed records",
        records.len(),
        1,
        5,
        || {
            let values =
                blazingly_json::from_slice::<Vec<LargeRecord>>(black_box(records.as_slice()))
                    .unwrap();
            black_box(
                values
                    .first()
                    .map(|value| (value.id, value.kind.len(), value.path.len(), value.score)),
            );
        },
        || {
            let values =
                serde_json::from_slice::<Vec<LargeRecord>>(black_box(records.as_slice())).unwrap();
            black_box(
                values
                    .first()
                    .map(|value| (value.id, value.kind.len(), value.path.len(), value.score)),
            );
        },
    );

    let requests = (0..1_000_u64).map(mcp_request).collect::<Vec<_>>();
    let request_bytes = requests.iter().map(Vec::len).sum::<usize>();
    println!("MCP cycle: 1,000 varied requests, repeated to 1,000,000");
    compare(
        "1,000,000 MCP Value parses",
        request_bytes,
        1_000,
        5,
        || {
            for request in &requests {
                black_box(
                    blazingly_json::from_slice::<Value>(black_box(request.as_slice())).unwrap(),
                );
            }
        },
        || {
            for request in &requests {
                black_box(
                    serde_json::from_slice::<serde_json::Value>(black_box(request.as_slice()))
                        .unwrap(),
                );
            }
        },
    );
}
