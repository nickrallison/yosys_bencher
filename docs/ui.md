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

The default workflow is intentionally small:

1. Choose a metric.
2. Use the whole suite or select inputs.
3. Compare all recorded commits, an inclusive commit range, or selected commits.
4. Choose **One line per input** or **Suite average**.
5. Click **Update chart**.

Each input starts with a mean line plus standard deviation and min–max bars from
its repeated measurements. The two bar types can be switched off independently.
**Suite average** combines the selected inputs into one mean line; it does not
mix different inputs into repeat-variability bars. Hover gives the commit message
and numeric values; click a point for full input, commit, measurement, and artifact
details. Commit search matches hashes, messages, and authors. The basic picker
keeps recorded commit order regardless of checkbox click order.

**Advanced options** is closed by default and rendered only when opened. It
retains the less-used controls rather than deleting or commenting out their
implementations. They are intentionally out of the everyday path: most users want
to find a regression and its affected inputs without choosing statistical policies.
An **Active** badge identifies nondefault analysis settings. **Reset advanced
analysis** restores the usual means, full input coverage, and latest completed
measurements while keeping the metric, selected inputs, commits, and workload
version.

Advanced options contains workload versions and version sources, explicit runs
and environments, normalization, custom grouping and reducers, coverage policies,
filters, cohorts, custom commit order, and axis settings. Other analysis views are
also retained there:

- Changes: compare commits, thresholds, and per-input regressions.
- Scaling: plot a declared input parameter on the X axis.
- Variation: examine repeats, different runs, or differences between inputs.
- Coverage: inspect failed, missing, unsupported, and unfinished measurements.

The metric picker shows derived measurements first, followed by submeasurement
groups. Open a group to see its immediate children; **Back** moves up one level
and **All** returns to the top. The selected metric stays visible above the picker;
click it to return to its group. Browsing groups does not change the selection.
MCM uses matching **Baseline → Synth** and **Optimized → Synth** groups for area
and cell count. **Optimized → MCM stats** and **Reference → MCM stats** contain
their operation counts and depth, alongside their respective timing metrics.
The same groups apply to existing stored results.
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

Controls edit a draft. **Update chart** applies it; the recipe and results
continue to describe the applied plan until then. Plot presentation changes apply
immediately. Refresh reloads discovery and analysis after a runner writes data.

History and Scaling keep the selected estimate as the line. Spread bars use only
repeated measurements of the same input at the same revision within one run.
Select one input or choose **One line per input**; combined-input lines have
no spread bars. Use **Variation → Across inputs** to inspect workload differences,
or **Variation → Between runs** to inspect differences between runs. Variation
uses its explicitly selected population. **Standard deviation (±1 SD)** adds thick
bars around the population mean; **Range (min–max)** adds thin whiskers at the
observed extremes. Both can be
enabled together or hidden without changing the estimate. Hover shows the count,
mean, minimum, maximum, and SD. These are descriptive ranges, not confidence intervals.

The advanced **Custom grouping and averages → Inputs → Sample standard deviation**
reduction still plots SD across inputs itself. Its **Use mean** shortcut restores
the mean; apply that analysis change with **Update chart**. Missing observations are excluded from spread,
and bars need at least two valid values. Repetition spread is unavailable for
normalized values, pooled runs, run-summary reductions, and formulas calculated after averaging.
On logarithmic axes, intervals with nonpositive endpoints are omitted.

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
