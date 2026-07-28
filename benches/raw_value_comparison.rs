use blazingly_json::value::RawValue;
use std::fmt::Write;
use std::hint::black_box;
use std::time::{Duration, Instant};

const MCP_CALL: &str = r#"{"jsonrpc":"2.0","id":"req-7","method":"tools/call","params":{"name":"query_graph","arguments":{ "query": "entry points", "limit": 20, "include_source": true }}}"#;

fn million_u64_json() -> String {
    let mut json = String::with_capacity(7_000_001);
    json.push('[');
    for value in 0..1_000_000_u64 {
        if value != 0 {
            json.push(',');
        }
        write!(json, "{value}").unwrap();
    }
    json.push(']');
    json
}

fn protocol_record_json() -> String {
    let mut json = String::with_capacity(8_000_001);
    json.push('[');
    for id in 0..100_000_u64 {
        if id != 0 {
            json.push(',');
        }
        write!(
            json,
            r#"{{"id":{id},"method":"tools/call","ok":true,"name":"query_graph"}}"#
        )
        .unwrap();
    }
    json.push(']');
    json
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

fn compare(name: &str, iterations: u32, mut ours: impl FnMut(), mut reference: impl FnMut()) {
    let warmup = iterations.div_ceil(10).max(3);
    batch(warmup, &mut ours);
    batch(warmup, &mut reference);

    let mut ours_samples = Vec::with_capacity(21);
    let mut reference_samples = Vec::with_capacity(21);
    for round in 0..21 {
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
        ours_samples.push(ours_time.as_secs_f64() * 1e9 / f64::from(iterations));
        reference_samples.push(reference_time.as_secs_f64() * 1e9 / f64::from(iterations));
    }

    let ours_ns = median(&mut ours_samples);
    let reference_ns = median(&mut reference_samples);
    let speedup = reference_ns / ours_ns;
    println!("{name:<30} {ours_ns:>14.2} ns {reference_ns:>14.2} ns {speedup:>8.2}x");
}

fn compare_small(ours_small: &RawValue, reference_small: &serde_json::value::RawValue) {
    compare(
        "small borrowed parse",
        100_000,
        || {
            let raw = blazingly_json::from_str::<&RawValue>(black_box(MCP_CALL)).unwrap();
            black_box(raw.get());
        },
        || {
            let raw =
                serde_json::from_str::<&serde_json::value::RawValue>(black_box(MCP_CALL)).unwrap();
            black_box(raw.get());
        },
    );
    compare(
        "small boxed parse",
        20_000,
        || {
            let raw = blazingly_json::from_str::<Box<RawValue>>(black_box(MCP_CALL)).unwrap();
            black_box(raw.get());
        },
        || {
            let raw = serde_json::from_str::<Box<serde_json::value::RawValue>>(black_box(MCP_CALL))
                .unwrap();
            black_box(raw.get());
        },
    );
    compare(
        "small from_string",
        20_000,
        || {
            black_box(RawValue::from_string(black_box(MCP_CALL.to_owned())).unwrap());
        },
        || {
            black_box(
                serde_json::value::RawValue::from_string(black_box(MCP_CALL.to_owned())).unwrap(),
            );
        },
    );
    compare(
        "small verbatim serialize",
        100_000,
        || {
            black_box(blazingly_json::to_vec(black_box(ours_small)).unwrap());
        },
        || {
            black_box(serde_json::to_vec(black_box(reference_small)).unwrap());
        },
    );
    compare(
        "small direct raw clone",
        100_000,
        || {
            black_box(ours_small.to_vec());
        },
        || {
            black_box(serde_json::to_vec(black_box(reference_small)).unwrap());
        },
    );
}

fn compare_large(
    large: &str,
    ours_large: &RawValue,
    reference_large: &serde_json::value::RawValue,
    records: &str,
) {
    let mut ours_output = Vec::with_capacity(large.len());
    let mut reference_output = Vec::with_capacity(large.len());

    compare(
        "large borrowed parse",
        3,
        || {
            let raw = blazingly_json::from_str::<&RawValue>(black_box(large)).unwrap();
            black_box(raw.get().len());
        },
        || {
            let raw =
                serde_json::from_str::<&serde_json::value::RawValue>(black_box(large)).unwrap();
            black_box(raw.get().len());
        },
    );
    compare(
        "large boxed parse",
        3,
        || {
            let raw = blazingly_json::from_str::<Box<RawValue>>(black_box(large)).unwrap();
            black_box(raw.get().len());
        },
        || {
            let raw =
                serde_json::from_str::<Box<serde_json::value::RawValue>>(black_box(large)).unwrap();
            black_box(raw.get().len());
        },
    );
    compare(
        "large verbatim serialize",
        5,
        || {
            black_box(blazingly_json::to_vec(black_box(ours_large)).unwrap());
        },
        || {
            black_box(serde_json::to_vec(black_box(reference_large)).unwrap());
        },
    );
    compare(
        "large writer serialize",
        10,
        || {
            ours_output.clear();
            blazingly_json::to_writer(&mut ours_output, black_box(ours_large)).unwrap();
            black_box(ours_output.len());
        },
        || {
            reference_output.clear();
            serde_json::to_writer(&mut reference_output, black_box(reference_large)).unwrap();
            black_box(reference_output.len());
        },
    );
    compare(
        "large direct raw clone",
        5,
        || {
            black_box(ours_large.to_vec());
        },
        || {
            black_box(serde_json::to_vec(black_box(reference_large)).unwrap());
        },
    );
    compare(
        "record array borrowed parse",
        3,
        || {
            let raw = blazingly_json::from_str::<&RawValue>(black_box(records)).unwrap();
            black_box(raw.get().len());
        },
        || {
            let raw =
                serde_json::from_str::<&serde_json::value::RawValue>(black_box(records)).unwrap();
            black_box(raw.get().len());
        },
    );
}

fn main() {
    let ours_small: &RawValue = blazingly_json::from_str(MCP_CALL).unwrap();
    let reference_small: &serde_json::value::RawValue = serde_json::from_str(MCP_CALL).unwrap();
    let large = million_u64_json();
    let ours_large: &RawValue = blazingly_json::from_str(&large).unwrap();
    let reference_large: &serde_json::value::RawValue = serde_json::from_str(&large).unwrap();
    let records = protocol_record_json();
    let large_len = u32::try_from(large.len()).expect("benchmark payload fits in u32");

    println!(
        "RawValue comparison; large payload = {:.2} MiB / 1,000,000 values",
        f64::from(large_len) / 1_048_576.0
    );
    println!(
        "{:<30} {:>17} {:>17} {:>9}",
        "workload", "blazingly-json", "serde_json", "speedup"
    );

    compare_small(ours_small, reference_small);
    compare_large(&large, ours_large, reference_large, &records);
}
