use blazingly_json::Value;
use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::hint::black_box;

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

fn parse_value(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("parse_value");
    group.throughput(Throughput::Bytes(MCP_CALL.len() as u64));
    group.bench_function("blazingly_json/mcp", |bench| {
        bench.iter(|| blazingly_json::from_slice::<Value>(black_box(MCP_CALL)).unwrap());
    });
    group.bench_function("serde_json/mcp", |bench| {
        bench.iter(|| serde_json::from_slice::<serde_json::Value>(black_box(MCP_CALL)).unwrap());
    });
    group.throughput(Throughput::Bytes(JWT_CLAIMS.len() as u64));
    group.bench_function("blazingly_json/jwt", |bench| {
        bench.iter(|| blazingly_json::from_slice::<Value>(black_box(JWT_CLAIMS)).unwrap());
    });
    group.bench_function("serde_json/jwt", |bench| {
        bench.iter(|| serde_json::from_slice::<serde_json::Value>(black_box(JWT_CLAIMS)).unwrap());
    });
    group.finish();
}

fn parse_typed(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("parse_typed_mcp");
    group.throughput(Throughput::Bytes(MCP_CALL.len() as u64));
    group.bench_function("blazingly_json", |bench| {
        bench.iter(|| blazingly_json::from_slice::<ToolCall>(black_box(MCP_CALL)).unwrap());
    });
    group.bench_function("serde_json", |bench| {
        bench.iter(|| serde_json::from_slice::<ToolCall>(black_box(MCP_CALL)).unwrap());
    });
    group.finish();
}

fn serialize_typed(criterion: &mut Criterion) {
    let value: ToolCall = serde_json::from_slice(MCP_CALL).unwrap();
    let mut group = criterion.benchmark_group("serialize_typed_mcp");
    group.throughput(Throughput::Bytes(MCP_CALL.len() as u64));
    group.bench_function("blazingly_json", |bench| {
        bench.iter(|| blazingly_json::to_vec(black_box(&value)).unwrap());
    });
    group.bench_function("serde_json", |bench| {
        bench.iter(|| serde_json::to_vec(black_box(&value)).unwrap());
    });
    group.finish();
}

fn parse_project_payloads(criterion: &mut Criterion) {
    let graph = serde_json::to_vec(&graph_snapshot()).unwrap();
    let mut group = criterion.benchmark_group("parse_typed_projects");

    group.throughput(Throughput::Bytes(graph.len() as u64));
    group.bench_function("blazingly_json/weavatrix_graph", |bench| {
        bench.iter(|| {
            blazingly_json::from_slice::<GraphSnapshot>(black_box(graph.as_slice())).unwrap()
        });
    });
    group.bench_function("serde_json/weavatrix_graph", |bench| {
        bench
            .iter(|| serde_json::from_slice::<GraphSnapshot>(black_box(graph.as_slice())).unwrap());
    });

    group.throughput(Throughput::Bytes(CARGO_JSONL.len() as u64));
    group.bench_function("blazingly_json/cargo_jsonl", |bench| {
        bench.iter(|| blazingly_json::from_slice::<CargoArtifact>(black_box(CARGO_JSONL)).unwrap());
    });
    group.bench_function("serde_json/cargo_jsonl", |bench| {
        bench.iter(|| serde_json::from_slice::<CargoArtifact>(black_box(CARGO_JSONL)).unwrap());
    });
    group.finish();
}

fn serialize_graph(criterion: &mut Criterion) {
    let graph = graph_snapshot();
    let encoded_size = serde_json::to_vec(&graph).unwrap().len();
    let mut group = criterion.benchmark_group("serialize_typed_graph");
    group.throughput(Throughput::Bytes(encoded_size as u64));
    group.bench_function("blazingly_json", |bench| {
        bench.iter(|| blazingly_json::to_vec(black_box(&graph)).unwrap());
    });
    group.bench_function("serde_json", |bench| {
        bench.iter(|| serde_json::to_vec(black_box(&graph)).unwrap());
    });
    group.finish();
}

criterion_group!(
    benches,
    parse_value,
    parse_typed,
    serialize_typed,
    parse_project_payloads,
    serialize_graph
);
criterion_main!(benches);
