# Local benchmark snapshot

These numbers are a development snapshot, not a universal performance claim.
Every comparison parses or serializes the same Rust type and payload in the
same Criterion process.

## Environment

- CPU: Intel Core Ultra 7 255U, 12 cores / 14 logical processors
- OS/toolchain host: Windows, `x86_64-pc-windows-gnu`
- Rust: 1.97.1
- reference: `serde_json` 1.0.151 with default features
- Criterion: 50 samples, 1 second warm-up, 3 second measurement

## Median latency

Positive relative values mean `blazingly-json` was faster in this local run.

| Workload | blazingly-json | serde_json | Relative |
| --- | ---: | ---: | ---: |
| MCP mutable `Value` parse | 1.485 µs | 1.873 µs | +20.7% |
| JWT mutable `Value` parse | 2.310 µs | 2.234 µs | -3.4% |
| MCP typed parse | 1.199 µs | 1.203 µs | +0.3% |
| MCP typed encode | 415 ns | 410 ns | -1.2% |
| Weavatrix-like typed graph parse | 65.04 µs | 63.78 µs | -2.0% |
| Cargo artifact JSONL typed parse | 1.374 µs | 1.549 µs | +11.3% |
| Weavatrix-like typed graph encode | 17.64 µs | 20.39 µs | +13.5% |

The current evidence supports a narrow conclusion: the implementation is
already competitive on the audited payload classes and wins clearly on some,
but it is not yet uniformly faster. More repetitions, Linux measurements,
allocation counts, profiling, fuzzing, and real consumer fixtures are required
before any dependency replacement decision.

## Reproduce

```text
cargo bench --bench serde_json_comparison -- --warm-up-time 1 --measurement-time 3 --sample-size 50
```
