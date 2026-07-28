# Local benchmark snapshot

These are local development measurements, not universal performance claims.
Each paired harness alternates engine order and reports medians. The benchmark
process was fixed to one logical processor at high priority because unpinned
runs on this hybrid CPU migrated between unlike cores.

## Environment

- CPU: Intel Core Ultra 7 255U, 12 cores / 14 logical processors
- OS/toolchain host: Windows, `x86_64-pc-windows-gnu`
- Rust: 1.97.1
- reference: `serde_json` 1.0.151
- process: logical processor 0, high priority

## Compatible API

The owned/typed compatible path is modestly faster, not multiple times faster:

| Workload | blazingly-json | serde_json | Relative |
| --- | ---: | ---: | ---: |
| MCP mutable `Value` parse | 2.195 us | 2.313 us | +5.37% |
| JWT mutable `Value` parse | 2.714 us | 2.753 us | +1.41% |
| MCP typed parse | 1.391 us | 1.423 us | +2.33% |
| MCP typed encode | 420.75 ns | 427.20 ns | +1.53% |
| Weavatrix-like graph parse | 77.72 us | 80.00 us | +2.92% |
| Weavatrix-like graph encode | 22.13 us | 24.70 us | +11.64% |
| Cargo artifact JSONL parse | 1.650 us | 1.850 us | +12.13% |

At larger scale the difference narrows:

| Workload | blazingly-json | serde_json | Relative |
| --- | ---: | ---: | ---: |
| 1,000,000 typed `u64` values, 6.57 MiB | 254.9 MiB/s | 249.8 MiB/s | +2.01% |
| 1,000,000 typed records, 80.98 MiB | 150.1 MiB/s | 147.6 MiB/s | +1.70% |
| 1,000,000 varied MCP `Value` parses | 69.7 MiB/s | 68.0 MiB/s | +2.48% |

The million-record parse makes exactly 2,000,001 allocations and retains
100.27 MiB with either engine. This is the important boundary: a compatible
owned result has the same strings, vectors, and ownership costs, so parser
micro-optimization cannot produce a 2-3x memory reduction.

Minimal stripped Windows executables are also effectively tied:
990,208 bytes for `blazingly-json` and 989,184 bytes for `serde_json`.

## Borrowed MCP fast path

`RawJson` changes the architecture instead of only changing the parser. The
request envelope borrows routing fields, defers `arguments`, and avoids a
complete request DOM.

Against the current `mcport` `Value` + clone dispatch:

| Request | Fast path | Current mcport | Speedup |
| --- | ---: | ---: | ---: |
| ping | 156.17 ns | 756.43 ns | 4.84x |
| initialize | 416.74 ns | 2,770.33 ns | 6.65x |
| tools/call | 1,117.02 ns | 3,551.38 ns | 3.18x |

Allocation measurements per request:

| Request | Fast path | Current mcport |
| --- | ---: | ---: |
| ping | 0 allocations / 0 bytes | 8 allocations / 670 bytes |
| tools/call | 5 allocations / 668 bytes | 27 allocations / 2,702 bytes |

The envelope-only `serde_json::RawValue` implementation remains 4-6% faster
than the current `RawJson` implementation in this same harness. The 3-6x
product gain therefore comes from the low-allocation protocol design and
direct response serialization, not from claiming that every primitive parser
operation beats every competitor.

## Canonical typed MCP path

For a generated compact `tools/call` layout, `CanonicalScanner` recognizes the
complete byte shape, borrows three strings, parses one `u64` and one boolean,
and falls back to the strict `Cursor` on any mismatch. Across three consecutive
24-round runs:

| Path | Median range | Relative to canonical |
| --- | ---: | ---: |
| canonical typed recognizer | 105.36-127.66 ns | 1.00x |
| strict order-independent `Cursor` | 390.25-485.27 ns | 3.46-4.00x slower |
| `serde_json` typed derive | 491.99-688.82 ns | 4.45-5.67x slower |

The canonical typed path performs zero allocations. This is the first parser
result in the project that clears the 2-3x target, but its scope is deliberately
narrow: exact field order, compact separators, plain strings, and a known
schema. Whitespace, reordered fields, escapes, other valid JSON shapes, or
unknown tool schemas take the strict fallback. This result is evidence for
generated MCP tool codecs, not a claim that the general JSON parser is 5x
faster than `serde_json`.

## mcport end to end

The full model includes request parsing, method dispatch, MCP result
construction, structured-content text generation, and response encoding.
Outputs are parsed and compared semantically before timing.

| Request | Fast path | Current mcport | Speedup |
| --- | ---: | ---: | ---: |
| ping | 344.93 ns | 1,425.04 ns | 4.13x |
| initialize | 1,014.87 ns | 6,683.85 ns | 6.59x |
| tools/list | 2,524.99 ns | 7,987.72 ns | 3.16x |
| tools/call | 2,103.43 ns | 7,298.68 ns | 3.47x |

An isolated integration spike in the real `mcport` source passes its original
tests, a new stream-vs-dispatch semantic differential test, documentation
tests, and strict Clippy. See [mcport-integration.md](mcport-integration.md).

## Reproduce

Build a harness and run its newest executable on one logical processor:

```text
cargo bench --bench mcport_end_to_end --no-run
```

```powershell
$exe = (Get-ChildItem target\release\deps -Filter 'mcport_end_to_end-*.exe' |
  Sort-Object LastWriteTime -Descending |
  Select-Object -First 1).FullName
$process = Start-Process -FilePath $exe -NoNewWindow -PassThru
$process.ProcessorAffinity = [IntPtr]1
$process.PriorityClass = 'High'
$process.WaitForExit()
```

Other committed harnesses:

```text
cargo bench --bench paired_comparison
cargo bench --bench large_payload
cargo bench --bench allocation_comparison
cargo bench --bench mcp_fast_path
cargo bench --bench mcp_allocations
```
