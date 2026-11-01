# Storage and analysis backend

The backend is implemented through the UI data boundary. The frontend can reuse
V1's workspace and charts; no frontend is included here.

- `store::SqliteStore` registers workloads and incrementally writes typed results.
- `analysis::Backend` provides discovery, selection, analysis, and inspection.
- `yosys-bench-ui-types` contains shared serializable types. It builds for native
  and WebAssembly targets without SQLite or benchmark tools.

## Running and querying

`Experiment::run` now writes to `output/results.sqlite`. Override that path with
`YOSYS_BENCH_DATABASE`. `run_stored` accepts an existing store and an explicit
`ExperimentSpec`. The MCM command still writes its JSON report as well.

```sh
cargo run --quiet --bin run_mcm
cargo run --bin results -- list
cargo run --bin results -- describe 1
cargo run --bin results -- runs 1
cargo run --bin results -- analyze plan.json
```

Both `run_mcm` and `build_yosys` show elapsed time, overall progress, and the
current revision/build phase. Measurement progress also includes the input,
repetition, and subprocess label. Redirected output uses plain stage lines without
terminal escape sequences. Use `cargo run --quiet` to silence Cargo's own output.

Build stdout and stderr are saved under each build directory in a unique
`logs-*/` directory (`configure.log`, `build.log`, and `install.log`). Failed builds
report their log path and collection continues. Benchmark process logs remain in
the attempt artifacts. MCM prints a short summary and report/database locations;
the full JSON is written to `results.json` instead of printed to the terminal.

Experiment IDs come from `list`; metric paths and input/run fields come from
`describe`. The results command accepts `--database PATH` before its command.
Use `-` instead of a JSON filename to read stdin. `results --help` lists the
inspection and artifact commands.

A minimal history plan is:

```json
{
  "selection": { "experiment": 1 },
  "metric": "optimized.runtime"
}
```

Omitted settings use History, the latest registered suite version, compatible
minor-version supersets, all recorded revisions and target inputs, latest
completed work items, arithmetic means, and strict input coverage. Multiple
machine/configuration partitions require an explicit choice. `describe` and
`runs` expose the metadata needed to make it.

A normalization setting can reference a revision outside the displayed range:

```json
{
  "selection": {
    "experiment": 1,
    "revisions": { "mode": "only", "ids": ["current-commit-id"] }
  },
  "metric": "optimized.runtime",
  "normalization": { "revision": "baseline-commit-id", "kind": "ratio" }
}
```

The other operation values are `changes`, `scaling`, `variation`, and `coverage`.
Their settings are defined in the shared `analysis::Operation` enum. Changes
supports an explicit pair, adjacent selected revisions, or previous measured
revisions within each series. An explicit pair supplies its own revision order.
Scaling requires an X parameter and a choice about reducing remaining varying
parameters. Variation names within-run, between-run, or across-input populations.
Coverage can omit the metric.

## Selection and estimates

Input matching uses typed equality and persistent IDs. Reordered inputs reuse
the same version; supersets advance its minor version. Invocation input order is
stored separately from the immutable definition's display order. A removed or
changed input advances the major version. Re-registering a historical definition
reuses its existing version.

Typed parameter filters and optional free-text search apply to the whole target
input set before paging. Explicit exclusions remain part of that predicate.
Run filters use the typed fields returned by `describe`, including environment
fields and `option:/...` JSON-pointer paths into execution configuration.

Latest selection uses invocation registration order with work-item IDs as the
stable ordering key, separately for each revision/input and configuration
partition. Completed failures remain selected. Newer unfinished work is reported;
an older completed work item can remain selected while that work is in progress.
If no completed item exists, normal history shows a gap. Explicit run selection
can inspect partial observations.

At least one valid repetition produces an input estimate. Failed repetitions stay
visible and do not enter the estimator. Strict coverage requires a usable estimate
for every selected input, rather than success for every planned repetition.
Sample SD requires at least two valid independent observations. Reused reference
observations carry run-scoped identities and do not acquire extra sample weight.

Selecting multiple runs requires separate series, a declared reduction of per-run
summaries, or explicit pooling. Environment/configuration partitions remain
separate. Formula metadata distinguishes per-repetition, per-input, and suite-level
composition, including mean ratios versus a ratio of reduced operands.

## Failures and inspection

Build failures are recorded once per revision and execution continues with the
next revision. Measurement/setup failures become failed attempts and execution
continues. No automatic retries or statistical decisions are made. A storage
failure returns an error, preserving committed observations; interruption marking
is attempted when the database remains writable. `interrupt_run` provides explicit
recovery after a process exits unexpectedly.

Analysis returns stable row/point identities and a snapshot containing its plan
and database generation. Inspection uses that snapshot and the selected row ID.
Any subsequent database write expires it; refresh analysis before inspecting it.
This avoids keeping transactions or result caches alive between browser requests.

Inspection provides estimates, raw observations, attempts, run configuration,
diagnostics, and artifact IDs. Artifact paths stay on the server; fetch their
contents by ID. Attempts distinguish pending, discarded, and retained artifacts;
individual references report whether their files still exist.

`StoreError` covers local errors. `ApiError::from(&error)` maps them to shared
error values, including control-specific validation errors and expired snapshots,
for a future Dioxus server-function wrapper.

## Adding a procedure

Implement `Procedure` and `StoredProcedure`, declare `BenchmarkIn` fields and
parameter values, and supply metrics through `BenchmarkOut`. The existing metric
macro remains available for simple scalar outputs. Override `analysis_metrics`
when directions, domains, reductions, or formula stages need more detail.

Register the adapter beside the procedure:

```rust
inventory::submit! {
    yosys_bench::analysis::AdapterRegistration::for_procedure::<MyProcedure>()
}
```

Registration constructs a generic decoder, not a suite or tool session. Linked
registrations are discovered automatically and conflicting contracts are rejected.
MCM, synthesis, and timing procedures are registered already. Experiments whose
adapter is unavailable remain visible in discovery.

## Initial scope

This uses one local SQLite database with a busy timeout and WAL readers/writer
coordination. Schema version 1 is created automatically; newer versions are
rejected. There is no V1 importer, automatic crash recovery, or persistent result
replay. The larger acceptance fixture, UI, metric relationships, adaptive sampling,
and statistical decision methods remain deferred.
