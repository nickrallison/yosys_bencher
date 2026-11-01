# Perf flamegraphs

`RunOptions::perf` enables extra diagnostic Yosys invocations using V1's
recording and rendering pipeline. `run_mcm` enables it by default. Perf counters
(`RunOptions::perf_counters`) are separate from flamegraph recordings.

Each attempt retains `perf/<job>/perf.data` and `perf/<job>/flamegraph.svg`,
along with the job's sources, script, command log, and stderr from each stage.
MCM profiles the baseline and optimized synthesis jobs. Generic Yosys benchmarks
profile their Yosys job. These diagnostic executions do not add timing samples.
Recordings are retained even when ordinary artifact retention is disabled.
Profiling failures appear in attempt diagnostics; measurements remain available.

The pipeline uses user-space CPU-clock samples at 999 Hz with DWARF stacks:

```text
perf record → perf script → stackcollapse-perf.pl --srcline → flamegraph.pl
```

Requirements: Linux perf permissions, perf with DWARF unwinding, Perl,
`stackcollapse-perf.pl` and `flamegraph.pl` on PATH, and LLVM addr2line.
`run_mcm` resolves `llvm-addr2line` from PATH. Yosys builds use `RelWithDebInfo`
for source information. Symbolization disables debuginfod and preserves source
lines. Every stage checks its exit status; empty sampled stacks are reported
instead of producing an empty SVG.

In the results UI, inspect an attempt and click its `flamegraph.svg` artifact.
The SVG opens directly in a new tab, including FlameGraph's search and zoom
controls. Existing attempts need a retained SVG to offer that link.

Verify the real pipeline without building Yosys:

```sh
cargo test --lib tools::perf::tests::native_flamegraph_contains_source_lines -- --ignored
```

This check requires `/usr/bin/llvm-addr2line-16` and a C compiler.
