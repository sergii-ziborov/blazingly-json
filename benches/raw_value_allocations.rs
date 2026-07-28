use blazingly_json::value::RawValue;
use stats_alloc::{Region, Stats, StatsAlloc, INSTRUMENTED_SYSTEM};
use std::alloc::System;
use std::fmt::Write;
use std::hint::black_box;

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

const MCP_CALL: &str = r#"{"jsonrpc":"2.0","id":"req-7","method":"tools/call","params":{"name":"query_graph","arguments":{"query":"entry points","limit":20,"include_source":true}}}"#;

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

fn report(name: &str, stats: Stats) {
    println!(
        "{name:<32} allocations={:<4} reallocations={:<4} allocated={:<10} deallocated={}",
        stats.allocations, stats.reallocations, stats.bytes_allocated, stats.bytes_deallocated
    );
}

fn main() {
    black_box(blazingly_json::from_str::<&RawValue>(MCP_CALL).unwrap());
    black_box(serde_json::from_str::<&serde_json::value::RawValue>(MCP_CALL).unwrap());
    drop(blazingly_json::from_str::<Box<RawValue>>(MCP_CALL).unwrap());
    drop(serde_json::from_str::<Box<serde_json::value::RawValue>>(MCP_CALL).unwrap());
    drop(RawValue::from_string(MCP_CALL.to_owned()).unwrap());
    drop(serde_json::value::RawValue::from_string(MCP_CALL.to_owned()).unwrap());

    let ours_borrowed = Region::new(GLOBAL);
    let raw = blazingly_json::from_str::<&RawValue>(black_box(MCP_CALL)).unwrap();
    black_box(raw.get());
    report("ours small borrowed", ours_borrowed.change());

    let reference_borrowed = Region::new(GLOBAL);
    let raw = serde_json::from_str::<&serde_json::value::RawValue>(black_box(MCP_CALL)).unwrap();
    black_box(raw.get());
    report("serde small borrowed", reference_borrowed.change());

    let ours_boxed = Region::new(GLOBAL);
    let raw = blazingly_json::from_str::<Box<RawValue>>(black_box(MCP_CALL)).unwrap();
    black_box(raw.get());
    report("ours small boxed", ours_boxed.change());
    drop(raw);

    let reference_boxed = Region::new(GLOBAL);
    let raw =
        serde_json::from_str::<Box<serde_json::value::RawValue>>(black_box(MCP_CALL)).unwrap();
    black_box(raw.get());
    report("serde small boxed", reference_boxed.change());
    drop(raw);

    let mut owned = MCP_CALL.to_owned();
    owned.shrink_to_fit();
    let ours_owned = Region::new(GLOBAL);
    let raw = RawValue::from_string(black_box(owned)).unwrap();
    black_box(raw.get());
    report("ours from_string reuse", ours_owned.change());
    drop(raw);

    let mut owned = MCP_CALL.to_owned();
    owned.shrink_to_fit();
    let reference_owned = Region::new(GLOBAL);
    let raw = serde_json::value::RawValue::from_string(black_box(owned)).unwrap();
    black_box(raw.get());
    report("serde from_string reuse", reference_owned.change());
    drop(raw);

    let large = million_u64_json();
    let ours_large = Region::new(GLOBAL);
    let raw = blazingly_json::from_str::<&RawValue>(black_box(&large)).unwrap();
    black_box(raw.get().len());
    report("ours million borrowed", ours_large.change());

    let reference_large = Region::new(GLOBAL);
    let raw = serde_json::from_str::<&serde_json::value::RawValue>(black_box(&large)).unwrap();
    black_box(raw.get().len());
    report("serde million borrowed", reference_large.change());

    let dom_region = Region::new(GLOBAL);
    let values = serde_json::from_str::<Vec<u64>>(black_box(&large)).unwrap();
    black_box(values.len());
    report("serde million Vec<u64>", dom_region.change());
    drop(values);
}
