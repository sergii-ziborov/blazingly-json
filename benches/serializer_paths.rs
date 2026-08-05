//! The two writer paths a REST response actually spends its time in.
//!
//! `keys` isolates the per-field cost: many short field names, tiny values, so
//! what is measured is the `,"name":` emission and almost nothing else.
//! `strings` isolates the escape scan by holding the field count fixed and
//! growing one string, which is the only axis a word-at-a-time scanner can pay
//! on. `response` is the shape that decides whether either matters — a list of
//! rows, the thing a list endpoint returns.
//!
//! Run against a baseline to compare:
//!
//! ```console
//! cargo bench --bench serializer_paths -- --save-baseline before
//! cargo bench --bench serializer_paths -- --baseline before
//! ```
//!
//! What this file measured when the word-at-a-time scan replaced the per-byte
//! table lookup in `write_string`. Three independent A/B comparisons of the
//! same two commits, on a loaded Windows host, one of them recorded back to
//! back with its baseline:
//!
//! | cell               | run 1  | run 2  | run 3  |
//! |--------------------|--------|--------|--------|
//! | strings/plain/4096 | −66.3% | −71.1% | −71.1% |
//! | strings/plain/1024 | −53.0% | −70.4% | −51.7% |
//! | strings/plain/190  | −39.8% | −65.5% | −30.1% |
//! | response/200       | −26.7% | −53.8% | **+12.3%** |
//! | response/20        | −12.3% | −46.1% | −40.7% |
//!
//! Read it honestly. The large-string cells agree across all three runs on
//! both direction and rough magnitude, and the mechanism is not in doubt, so
//! the scan is faster there. Everything below a kilobyte does not survive
//! being measured three times — `response/200` swung from −54% to +12% between
//! two runs of identical code, which is not a property of the code.
//!
//! So this file reports one result and one blocker. The result: word-at-a-time
//! escape scanning is a large win on long strings and no loss on short ones.
//! The blocker: this host cannot rank anything smaller, and until a quiet host
//! runs these, no number under about 30% from this machine should be published
//! or used to decide what to build.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use serde::Serialize;
use std::hint::black_box;

/// Twelve short field names and values with no escaping work of their own, so
/// the measurement is dominated by key emission.
#[derive(Serialize)]
struct WideRow {
    id: u64,
    tenant_id: u64,
    created_at: u64,
    updated_at: u64,
    version: u32,
    active: bool,
    archived: bool,
    priority: u8,
    score: f64,
    parent_id: u64,
    sequence: u64,
    revision: u32,
}

impl WideRow {
    fn new(id: u64) -> Self {
        Self {
            id,
            tenant_id: 42,
            created_at: 1_754_400_000,
            updated_at: 1_754_400_600,
            version: 7,
            active: true,
            archived: false,
            priority: 3,
            score: 0.875,
            parent_id: 0,
            sequence: id,
            revision: 2,
        }
    }
}

/// One string field, so growing it grows only the escape scan.
#[derive(Serialize)]
struct TextRow {
    id: u64,
    body: String,
}

/// What a list endpoint returns.
#[derive(Serialize)]
struct TaskRow {
    id: u64,
    title: String,
    status: &'static str,
    assignee: String,
    created_at: u64,
    done: bool,
}

fn tasks(count: usize) -> Vec<TaskRow> {
    (0..count)
        .map(|index| TaskRow {
            id: index as u64,
            title: format!("Review the deployment checklist for release {index}"),
            status: "in_progress",
            assignee: "sergii.ziborov@example.com".to_owned(),
            created_at: 1_754_400_000 + index as u64,
            done: index % 3 == 0,
        })
        .collect()
}

fn keys(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("ser/keys");
    for count in [1_usize, 20, 200] {
        let rows = (0..count)
            .map(|i| WideRow::new(i as u64))
            .collect::<Vec<_>>();
        group.throughput(Throughput::Elements((count * 12) as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), &rows, |b, rows| {
            b.iter(|| blazingly_json::to_vec(black_box(rows)).expect("encodes"));
        });
    }
    group.finish();
}

fn strings(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("ser/strings");
    for length in [4_usize, 16, 64, 190, 1024, 4096] {
        let plain = TextRow {
            id: 1,
            body: "a".repeat(length),
        };
        // Every eighth byte needs escaping: the case a word-at-a-time scan
        // cannot win on, included so a regression there is visible.
        let dense = TextRow {
            id: 1,
            body: (0..length)
                .map(|i| if i % 8 == 0 { '"' } else { 'a' })
                .collect(),
        };
        group.throughput(Throughput::Bytes(length as u64));
        group.bench_with_input(BenchmarkId::new("plain", length), &plain, |b, row| {
            b.iter(|| blazingly_json::to_vec(black_box(row)).expect("encodes"));
        });
        group.bench_with_input(BenchmarkId::new("escaped", length), &dense, |b, row| {
            b.iter(|| blazingly_json::to_vec(black_box(row)).expect("encodes"));
        });
    }
    group.finish();
}

fn response(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("ser/response");
    for count in [1_usize, 20, 200] {
        let rows = tasks(count);
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), &rows, |b, rows| {
            b.iter(|| blazingly_json::to_vec(black_box(rows)).expect("encodes"));
        });
    }
    group.finish();
}

criterion_group!(benches, keys, strings, response);
criterion_main!(benches);
