# Results UI

The Dioxus fullstack app in `webapp/` reuses V1's theme, chart cards, and local
Plotly bridge. It reads V2's SQLite results through the registered typed adapters.
It does not need Yosys or benchmark executables to browse existing results.

## Run

Install the Dioxus 0.7 CLI (`dx`) and the `wasm32-unknown-unknown` Rust target, then:

```sh
cd webapp
dx serve --web --fullstack true
```

Open the address printed by `dx`. The default database is the repository's
`output/results.sqlite`, independent of the working directory. To use another:

```sh
YOSYS_BENCH_DATABASE=/absolute/path/results.sqlite dx serve --web --fullstack true
```

The webapp has its own Cargo workspace and lockfile. Build both targets with
`dx build --web --fullstack true`. All chart assets are bundled locally.

## Workspace

Choose an experiment and workload version above the workspace. The sidebar starts
with the analysis mode and metric, followed by shared data and run controls:

- History: revisions, grouping, reductions, and optional per-input normalization.
- Changes: adjacent revisions, previous measured revisions, or an explicit pair;
  practical thresholds, rankings, and matched suite summaries.
- Scaling: a declared input parameter on the X axis, with explicit handling of
  other varying parameters.
- Variation: within-run repetitions, between-run estimates, or workload spread;
  counts, quantiles, and descriptive sample SD.
- Coverage: execution states and optional metric availability against the selected
  workload, including failed builds and unfinished work.

The metric picker shows derived measurements first, followed by submeasurement
groups. Open a group to see its immediate children; **Back** moves up one level
and **All** returns to the top. The selected metric stays visible above the picker;
click it to return to its group. Browsing groups does not change the selection.
For new provider-built Yosys measurements, open **Timings → Calls → synth** to
select its inclusive CPU **runtime** or its **self runtime**, excluding subpasses.
Open a subpass to select its own timing. Calls appear in execution order;
repeated siblings appear as **opt**, **opt (2)**, and so on. The hierarchy keeps
the same pass under different parents separate. In MCM these sit inside
**Baseline** and **Optimized**.
**Timings → Passes → pass name** retains aggregate self CPU time and call count
across all invocations, including historical flat reports. Existing measurements
need a new benchmark run to gain the call tree.
**Timings → Total** is Yosys's internal CPU total; **Runtime** is elapsed wall time.

Input search and typed filters operate across all pages. Inputs can be selected
individually or by a predicate with exclusions. Revision selection preserves
explicit order. Environment/configuration partitions stay separate; selecting
multiple runs requires a deliberate treatment. Group previews show cardinality
before analysis is applied.

Controls edit a draft. **Update analysis** applies it; the recipe and results
continue to describe the applied plan until then. Plot presentation changes apply
immediately. Refresh reloads discovery and analysis after a runner writes data.

Select a chart point or a table's Inspect button to see contributing estimates,
raw observations, attempts, diagnostics, run configuration, and retained artifacts.
Artifact names are direct links that open in a new tab. SVG flamegraphs render
in the browser with their search and zoom controls. Artifacts can also be
previewed as text or downloaded. Inspection expires after a
database write; Refresh results creates a new snapshot. Direct run/work-item
panels show their current status and are labelled accordingly.

Calculation filters change estimates. Display filters only hide result values.
Log axes omit nonpositive values from the plot while tables retain them. SD bars
are descriptive spread, not confidence intervals. Coverage bars count selected
input/environment slots; the table retains individual run and attempt counts.

## Boundaries and checks

`webapp/src/api.rs` wraps discovery, input paging, grouping, analysis, inspection,
and artifact access in typed Dioxus server functions. Only the native server links
SQLite and benchmark adapters; the browser uses `yosys-bench-ui-types`.
`StoredProcedure::DEFAULT_METRIC` optionally chooses the initial metric; otherwise
the first declared metric is used. Registry discovery validates that default.

```sh
cargo test --workspace --all-targets
cargo test --manifest-path webapp/Cargo.toml --features server
node --test webapp/assets/plotly_bridge.test.cjs
```

UI tests render all five modes against typed in-memory results. Bridge tests cover
numeric axes, inspection IDs, log omissions, and coverage aggregation. Saved views,
V1 import, metric relationships, adaptive sampling, and statistical decisions
remain outside this initial UI.
