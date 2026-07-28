use blazingly_json::Value;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::hint::black_box;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ToolCall {
    jsonrpc: String,
    id: String,
    method: String,
    params: ToolParams,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ToolParams {
    name: String,
    arguments: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct GraphSnapshot {
    version: u64,
    project: String,
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct GraphNode {
    id: u64,
    kind: String,
    path: String,
    labels: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct GraphEdge {
    source: u64,
    target: u64,
    kind: String,
    confidence: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CargoArtifact {
    reason: String,
    package_id: String,
    target: CargoTarget,
    profile: CargoProfile,
    executable: Option<String>,
    fresh: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CargoTarget {
    name: String,
    kind: Vec<String>,
    crate_types: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CargoProfile {
    opt_level: String,
    debuginfo: u64,
    test: bool,
}

const MCP_CALL: &[u8] = br#"{"jsonrpc":"2.0","id":"req-7","method":"tools/call","params":{"name":"query_graph","arguments":{"query":"entry points","limit":"20","include_source":"true"}}}"#;
const JWT_CLAIMS: &[u8] = br#"{"sub":"user-123","iss":"blazingly","aud":["api","mcp"],"exp":1924992000,"iat":1924988400,"scope":"graph:read tools:call","sid":"session-456","dat":{"theme":"dark","locale":"he-IL"}}"#;
const CARGO_JSONL: &[u8] = br#"{"reason":"compiler-artifact","package_id":"path+file:///workspace/demo#0.1.0","target":{"name":"demo","kind":["bin"],"crate_types":["bin"]},"profile":{"opt_level":"3","debuginfo":0,"test":false},"executable":"C:\\workspace\\target\\release\\demo.exe","fresh":false}"#;

fn graph_snapshot() -> GraphSnapshot {
    let nodes = (0..64)
        .map(|id| GraphNode {
            id,
            kind: if id % 3 == 0 { "function" } else { "module" }.to_owned(),
            path: format!("src/module_{}/symbol_{id}.rs", id % 12),
            labels: vec!["rust".to_owned(), "production".to_owned()],
        })
        .collect();
    let edges = (0..96)
        .map(|id| GraphEdge {
            source: id % 64,
            target: (id * 7 + 3) % 64,
            kind: if id % 2 == 0 { "calls" } else { "imports" }.to_owned(),
            confidence: 0.99,
        })
        .collect();
    GraphSnapshot {
        version: 1,
        project: "weavatrix-rust".to_owned(),
        nodes,
        edges,
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

fn compare(name: &str, iterations: u32, mut ours: impl FnMut(), mut reference: impl FnMut()) {
    let warmup = iterations.div_ceil(10).max(50);
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
    let advantage = (reference_ns / ours_ns - 1.0) * 100.0;
    println!("{name:<28} {ours_ns:>12.2} ns {reference_ns:>12.2} ns {advantage:>+8.2}%");
}

fn main() {
    let tool: ToolCall = serde_json::from_slice(MCP_CALL).unwrap();
    let graph = graph_snapshot();
    let graph_json = serde_json::to_vec(&graph).unwrap();

    println!(
        "{:<28} {:>15} {:>15} {:>9}",
        "workload", "blazingly-json", "serde_json", "relative"
    );

    compare(
        "MCP Value parse",
        20_000,
        || {
            black_box(blazingly_json::from_slice::<Value>(black_box(MCP_CALL)).unwrap());
        },
        || {
            black_box(serde_json::from_slice::<serde_json::Value>(black_box(MCP_CALL)).unwrap());
        },
    );
    compare(
        "JWT Value parse",
        20_000,
        || {
            black_box(blazingly_json::from_slice::<Value>(black_box(JWT_CLAIMS)).unwrap());
        },
        || {
            black_box(serde_json::from_slice::<serde_json::Value>(black_box(JWT_CLAIMS)).unwrap());
        },
    );
    compare(
        "MCP typed parse",
        20_000,
        || {
            black_box(blazingly_json::from_slice::<ToolCall>(black_box(MCP_CALL)).unwrap());
        },
        || {
            black_box(serde_json::from_slice::<ToolCall>(black_box(MCP_CALL)).unwrap());
        },
    );
    compare(
        "MCP typed encode",
        20_000,
        || {
            black_box(blazingly_json::to_vec(black_box(&tool)).unwrap());
        },
        || {
            black_box(serde_json::to_vec(black_box(&tool)).unwrap());
        },
    );
    compare(
        "Weavatrix graph parse",
        300,
        || {
            black_box(
                blazingly_json::from_slice::<GraphSnapshot>(black_box(graph_json.as_slice()))
                    .unwrap(),
            );
        },
        || {
            black_box(
                serde_json::from_slice::<GraphSnapshot>(black_box(graph_json.as_slice())).unwrap(),
            );
        },
    );
    compare(
        "Weavatrix graph encode",
        500,
        || {
            black_box(blazingly_json::to_vec(black_box(&graph)).unwrap());
        },
        || {
            black_box(serde_json::to_vec(black_box(&graph)).unwrap());
        },
    );
    compare(
        "Cargo artifact parse",
        20_000,
        || {
            black_box(blazingly_json::from_slice::<CargoArtifact>(black_box(CARGO_JSONL)).unwrap());
        },
        || {
            black_box(serde_json::from_slice::<CargoArtifact>(black_box(CARGO_JSONL)).unwrap());
        },
    );
}
