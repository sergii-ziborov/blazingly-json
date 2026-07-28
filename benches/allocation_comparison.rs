use serde::Deserialize;
use stats_alloc::{Region, Stats, StatsAlloc, INSTRUMENTED_SYSTEM};
use std::alloc::System;
use std::hint::black_box;
use std::io::Write;

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

#[derive(Debug, Deserialize)]
struct LargeRecord {
    id: u64,
    kind: String,
    path: String,
    score: f64,
}

fn record_json(count: u64) -> Vec<u8> {
    let count = usize::try_from(count).expect("benchmark record count fits usize");
    let mut json = Vec::with_capacity(count * 88 + 1);
    json.push(b'[');
    for id in 0..u64::try_from(count).expect("benchmark record count fits u64") {
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

fn live_bytes(stats: Stats) -> i64 {
    i64::try_from(stats.bytes_allocated).expect("allocated bytes fit i64")
        - i64::try_from(stats.bytes_deallocated).expect("deallocated bytes fit i64")
}

fn bounded_f64(value: usize) -> f64 {
    f64::from(u32::try_from(value).expect("benchmark value fits u32"))
}

fn bounded_i64_f64(value: i64) -> f64 {
    f64::from(i32::try_from(value).expect("benchmark value fits i32"))
}

fn report(name: &str, stats: Stats) {
    println!(
        "{name:<18} allocations={:<10} reallocations={:<8} allocated={:>10.2} MiB live={:>10.2} MiB",
        stats.allocations,
        stats.reallocations,
        bounded_f64(stats.bytes_allocated) / 1_048_576.0,
        bounded_i64_f64(live_bytes(stats)) / 1_048_576.0,
    );
}

fn main() {
    let input = record_json(1_000_000);
    println!("input={:.2} MiB", bounded_f64(input.len()) / 1_048_576.0);

    let ours_region = Region::new(GLOBAL);
    let ours = blazingly_json::from_slice::<Vec<LargeRecord>>(black_box(input.as_slice())).unwrap();
    black_box(
        ours.first()
            .map(|value| (value.id, value.kind.len(), value.path.len(), value.score)),
    );
    report("blazingly-json", ours_region.change());
    drop(ours);

    let reference_region = Region::new(GLOBAL);
    let reference =
        serde_json::from_slice::<Vec<LargeRecord>>(black_box(input.as_slice())).unwrap();
    black_box(
        reference
            .first()
            .map(|value| (value.id, value.kind.len(), value.path.len(), value.score)),
    );
    report("serde_json", reference_region.change());
    drop(reference);
}
