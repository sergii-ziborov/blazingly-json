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

`RawJson` remains the small sized wrapper used by `JsonCursor`. The compatible DST
surface is now provided separately by `blazingly_json::value::RawValue`.

## Compatible `RawValue`

The paired `raw_value_comparison` harness compares the same borrowed, boxed,
owned-string, and verbatim-output operations with
`serde_json::value::RawValue`. Three consecutive local runs produced:

| Workload | Speedup range |
| --- | ---: |
| small MCP borrowed parse | 1.22-1.59x |
| small MCP boxed parse | 1.21-1.59x |
| small MCP `from_string` | 1.34-1.54x |
| small MCP verbatim `to_vec` | 1.69-2.36x |
| 1,000,000-number borrowed parse | 1.42-1.79x |
| 1,000,000-number boxed parse | 1.19-1.55x |
| compact flat protocol-record array | 1.62-2.07x |
| large preallocated writer output | approximately 1.00x |

The parser uses strict recognizers for compact number arrays and compact flat
protocol records. Any mismatch takes the fully validating general parser.
Large raw serialization is a memory copy and remains bandwidth-bound.

The allocation harness reports zero allocations for borrowed parsing, one
exact-size allocation for boxed parsing, and zero allocations when
`from_string` can reuse its input buffer. On the same machine serde_json used
one additional temporary 8-byte allocation on the measured small raw paths.
Both borrowed implementations used zero allocations for the 6.57 MiB
million-number payload. Materializing that payload as `Vec<u64>` instead
retained 8 MiB after 18 reallocations.

## Canonical typed MCP path

For a generated compact `tools/call` layout, the canonical scanners recognize
the complete shape, borrow three fields, parse one `u64` and one boolean, and
fall back to the strict `JsonCursor` on any mismatch. The `&str` path reuses
UTF-8 validation already performed by a line reader. `CanonicalBytesScanner`
matches structural ASCII directly and validates captured ASCII strings while
scanning. Across three consecutive 24-round runs:

| Path | Median range | Speedup over typed serde_json |
| --- | ---: | ---: |
| canonical byte recognizer with inline ASCII validation | 45.94-50.47 ns | 7.35-7.62x |
| canonical typed recognizer, prevalidated `&str` | 47.63-51.81 ns | 7.03-7.43x |
| strict order-independent `JsonCursor` | 261.82-284.37 ns | 1.31-1.35x |
| `serde_json` typed derive | 342.85-384.78 ns | 1.00x |

Both canonical typed paths perform zero allocations and clear the 2-3x target.
The new byte path also removes the former 10-26% full-input UTF-8 penalty. Its
scope is deliberately narrow: exact field order, compact separators, plain
strings, and a known schema. Whitespace, reordered fields, escapes, non-ASCII
canonical identifiers, other valid JSON shapes, or unknown tool schemas take
the strict fallback. This result is evidence for generated MCP tool codecs,
not a claim that the general JSON parser is 7x faster than `serde_json`.

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
cargo bench --bench raw_value_comparison
cargo bench --bench raw_value_allocations
```
