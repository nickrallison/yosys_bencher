# Storage and UI data design

Status: proposed design, based on the storage and UI discussion. Rust snippets
describe intended boundaries; they are not implemented APIs. Defaults identified
as proposals still need validation during implementation.

## Purpose and scope

Run performance tests across commits and inspect how their results change.
Experiment authors should provide typed inputs, outputs, and metric descriptions.
Their results should appear in the generic UI without benchmark-specific frontend
code. Shared experiment logic should remain reusable, as in MCM and synthesis.

The main boundary is:

```text
Store -> typed results -> metric extraction and analysis -> UI data
```

The Store owns persistence, discovery, identity, and dataset selection. Analysis
owns aggregation and statistical interpretation. The UI owns interaction and
presentation. V1's chart and table presentation is a useful starting point; its
sidebar and aggregation controls need further design.

This document covers the data needed for those responsibilities. Detailed sidebar
design, suite-version comparison screens, saved views, and adaptive sampling are
later work.

## Use cases

One person may fill several roles. The groups describe what they are trying to do.

### UI users

| Use case | Required data or operation |
| --- | --- |
| Find an experiment | Name, description, available versions, creation time, latest measurement time, and clearly labeled result counts |
| Select a workload version | Target version and its inputs; exact-version results or projection from compatible later minor versions |
| View metrics across commits | Explicit revision order, precise commit selection, selected inputs, measurements, and units |
| Narrow a large suite | Searchable input descriptions, typed parameter filters, matching counts, paging, and exclusions |
| Select runs and environments | Run metadata and explicit selection policies so reruns or different machines are not silently combined |
| Change aggregation | Raw observations and separate choices for repetitions, runs when selected, and inputs |
| Split a chart into series | Group by input identity or parameter; preview group values and counts before creating many series |
| Investigate a run | Run configuration, environment, source version, commit details, per-input summaries, individual attempts, diagnostics, and artifacts |
| Explain missing points | Optional display of build failures, failed or unsupported attempts, unfinished work, unmeasured work, and missing metrics |
| Inspect variation at a commit | Sample counts and standard deviation with an explicit population: repetitions within a run or summaries across runs |
| Check what contributes to a point | Included inputs and observations, exclusions, coverage, selected run IDs, and any fallback to older results |
| Compare suite definitions, later | Added, removed, and retained input identities; compatibility and matching rules |

### Experiment authors

| Use case | Required data or operation |
| --- | --- |
| Add an experiment type | Implement a procedure and suite with typed, serializable inputs and outputs; register the common typed storage adapter |
| Reuse execution logic | Compose existing synthesis, tool, measurement, and reference components, as MCM and synthesis already do |
| Make results appear in the UI | Declare output metrics, units, descriptions, and supported reductions through shared interfaces, without benchmark-specific frontend code |
| Make inputs searchable and groupable | Declare input labels and typed parameter fields with supported filter and grouping operations |
| Evolve a suite | Preserve stable input identities; additions produce compatible minor versions, while incompatible changes advance the major version |
| Evolve result formats or metric meaning | Declare payload schemas and measurement contracts so incompatible data is rejected or explicitly migrated |
| Access results in code | Read concrete input and output types through a validated Store handle and reuse the metric interface for analysis |

### Benchmark operators

| Use case | Required data or operation |
| --- | --- |
| Run an experiment across commits | Select the suite, revisions, inputs, repetitions, execution options, and environment; persist the resolved run plan |
| Publish results to local discovery | Register the experiment, definition, and planned run together so results appear in the UI without a separate catalog step |
| Collect more measurements | Create a new run while preserving previous runs, raw repetitions, and provenance |
| Inspect partial or failed execution | Incrementally persisted outcomes, build failures, diagnostics, and distinct unfinished or interrupted states |
| Collect diagnostic artifacts | Choose artifact retention and distinguish diagnostic executions from normal measurements |
| Measure adaptively, later | Configure the metric, baseline, tolerance, and sample budget; retain observations, progress, and stopping reasons |

A dedicated two-commit comparison screen is not an initial requirement. Precise
commit selection and run inspection are more useful first. Reproducibility is a
system property: a response should identify the records used, even before saved
views exist.

## Persistent identities

| Entity | Meaning |
| --- | --- |
| Experiment | An ongoing named collection, such as MCM performance, scoped to its source repository and procedure family |
| Suite definition | An immutable, versioned workload definition with frozen inputs |
| Input | A stable identity for the complete benchmark configuration |
| Revision | A commit identity within a repository, with descriptive commit metadata |
| Run | One invocation against a definition, selected revisions and inputs, execution options, and environment |
| Work item | One `(run, revision, input)` execution, containing its repetitions and completion state |
| Attempt | One execution of one repetition, with its own identity, outcome, and provenance |

Use opaque IDs internally. Names and timestamps describe records; they do not
identify a dataset. The current Rust `Experiment<S>` represents an execution
configuration, while “experiment” here means the persistent collection. Naming
can be reconciled when the driver is implemented.

The experiment picker first selects an experiment, then a version. The proposed
default is its latest registered version. Show whether that version has results;
do not silently substitute a different workload because a newer version is empty.
Use `last_measured_at`, separate from any future last-viewed preference. Define
counts explicitly, for example successful attempts and measured revisions.

Input identity must include configuration that affects execution, including
relevant source contents, libraries, options, and reference-tool identity. A
label, path, or position in a vector is insufficient. Reordering inputs must
preserve their IDs. Changing their meaning must not reuse an ID.

Initially, storage-assigned IDs can be reused by comparing decoded inputs within
a procedure family using their typed equality. A persistent content hash is an
alternative only with a defined canonical encoding and collision checks. Rust's
ordinary `Hash` implementation is not the persistent identity format.

## Suite versions and compatibility

Minor versions extend a workload; incompatible changes advance the major version.
Within one procedure and measurement contract:

```text
1.1 = {A, B}
1.2 = {A, B, C}
1.3 = {A, B, C, D}

view 1.1 using compatible supersets:
    accept sources 1.1, 1.2, 1.3
    retain only A and B
```

For target `a.c`, eligible source versions are `a.b` where `b >= c`. This is a
projection of a later workload onto an earlier one, not a Rust type downcast.
Filtering and grouping evaluate against the target definition, so the example
always selects from two inputs, regardless of the source version.

```rust
enum VersionSources {
    Exact,
    CompatibleSupersets,
}

struct VersionSelection {
    target: Version,
    sources: VersionSources,
}
```

Proposed UI default: compatible supersets, with the policy visible. Preserve the
actual source definition and version on every selected work item. When eligible
versions overlap at the same revision/input, apply the explicit run-selection
policy below; do not count both as extra repetitions.

Registration establishes one authoritative definition lineage. Equal input sets
reuse a version, strict supersets advance the minor version, and incompatible
sets advance the major version. An existing version cannot acquire a different
input set. Preserve definition order for display and actual execution order in
the run plan; neither order is input identity.

Workload compatibility does not establish payload or measurement compatibility.
Persist separate identifiers for:

- The procedure family and its measurement contract, including metric semantics.
- Input and output payload schemas, with explicit decoder support.
- The workload's major/minor version.

These can be simple explicit identifiers; they do not require several automatic
versioning systems. Changed measurement meaning requires an explicit compatibility
decision. Compatible workload versions must not bridge incompatible measurement
contracts merely because their input sets overlap.

## Registration and incremental writes

Use one SQLite database initially. Discovery metadata belongs in that database,
with no separate catalog database pointing at per-experiment databases. Listing
experiments can query the experiment and definition records directly.

The future driver should call one operation that registers or resolves the
experiment and definition and creates the planned run:

```rust
let run = store.register_run::<McmProcedure>(
    &experiment,
    &definition,
    &plan,
)?;

let attempt = run.start_attempt(&work_item, sample_index)?;
run.finish_attempt(attempt, &record, &artifacts)?;
run.finish_work_item(&work_item)?;
run.finish(&completion)?;
```

Registration commits the experiment, definition, input memberships, and run plan
together. This prevents callers from forgetting a catalog update. Ordinary
database transactions and constraints are enough. A conflicting or stale
registration returns `Err`; no automatic retry, merge, distributed coordination,
or multithreaded runner is required.

Persist the selected revisions and inputs before execution, then attempt starts
and terminal outcomes incrementally. Record build/setup failures at revision
scope. A run can finish with failed or unsupported measurements; “finished” does
not mean “all measurements succeeded.” Keep unfinished and interrupted work
distinct from terminal failures. Do not infer interruption solely from an old
timestamp.

An attempt belongs to one repetition within a work item. If retries are added,
link them to that repetition and distinguish them from new samples. Starting or
finishing records must validate ownership and state; an accidental duplicate
finish must not create another observation or replace a recorded result.

Diagnostics and artifact references are persisted with the outcome. Artifact
contents can live outside SQLite. Inspection should distinguish retained,
discarded, and unavailable artifacts. Diagnostic executions that change timing
need a separate purpose and an optional link to their source attempt; normal
history queries exclude them.

## Typed storage and runtime dispatch

Persist benchmark-specific inputs and outputs as versioned serialized payloads.
Deserialize directly into their concrete Rust types and obtain metrics through
`BenchmarkOut`. There is no need to store only a generic metric tree or rebuild
benchmark objects from one.

A typed handle associates the experiment with a procedure, rather than allowing
arbitrary unrelated input/output type parameters:

```rust
trait StoredProcedure: Procedure {
    const STORAGE_KEY: &'static str;
    const INPUT_SCHEMA: u32;
    const OUTPUT_SCHEMA: u32;
    const MEASUREMENT_CONTRACT: &'static str;
}

let experiment = store.experiment::<McmProcedure>(experiment_id)?;
let frame: ResultFrame<McmInput, McmOutput> = experiment.results(&selection)?;
```

Opening the handle validates stored identities and decoder support. Unsupported
schemas produce a clear error. Read-only access must not initialize tools, build
Yosys, or call `Suite::begin`.

The browser does not need `McmOutput` or a duplicate rendering implementation.
The server selects a typed adapter once, then uses common analysis and presentation
code. A small registry can map a persistent procedure/schema identity to that
adapter:

```rust
// Illustrative object-safe boundary; request/response types are shared DTOs.
trait ExperimentAdapter {
    fn describe(&self, store: &SqliteStore, id: ExperimentId)
        -> Result<ExperimentDescription, StoreError>;

    fn analyze(&self, store: &SqliteStore, id: ExperimentId, plan: &AnalysisPlan)
        -> Result<AnalysisResponse, StoreError>;
}

// Common implementation: decode P::Input/P::Output, then run generic analysis.
struct TypedAdapter<P>(std::marker::PhantomData<P>);

// Registration creates the common adapter; no experiment-specific UI methods.
registry.register::<McmProcedure>()?;
```

The registry can hold `Box<dyn ExperimentAdapter>`. `TypedAdapter<P>` implements
the operations generically. Reject duplicate registrations, and report stored
experiments with unavailable adapters as unsupported rather than hiding them.
Keep discovery metadata readable without decoding outputs. A fixed enum or match
is also adequate for a small executable; a dynamic plugin loader is unnecessary.

The proposed “parent type with metric information” is useful as a common view.
It does not require downcasting. The current `BenchmarkOut` is not dyn-compatible:
it has `Clone` and Serde bounds and a static `list_metrics` method. If dynamic
access to already decoded outputs is useful, expose a smaller object-safe view:

```rust
trait MetricView {
    fn descriptors(&self) -> Vec<MetricDescriptor>;
    fn metric(&self, name: &str) -> Option<MetricValue>;
}

impl<T: BenchmarkOut> MetricView for T {
    fn descriptors(&self) -> Vec<MetricDescriptor> {
        T::list_metrics()
    }

    fn metric(&self, name: &str) -> Option<MetricValue> {
        self.get_metric(name)
    }
}
```

Consumers can borrow `&dyn MetricView` without knowing the concrete output. This
does not tell storage how to deserialize bytes, so typed decoders still need a
dispatch mechanism. Prefer keeping a concrete frame inside the adapter; erase
types only where runtime selection requires it. Concrete downcasts should be
reserved for genuinely type-specific operations, outside generic UI rendering.

## Store read surface

Metadata queries must not load measurement payloads. The conceptual operations
are:

| Operation | Returns |
| --- | --- |
| List experiments | Discovery summaries and availability information |
| Describe experiment and versions | Definitions, compatibility, counts, and supported contracts |
| List revisions | Ordered commit metadata, including revisions without measurements |
| Describe/search inputs | Parameter descriptors, paged descriptions, and matching counts |
| Describe groups | Group values and counts for the selected target inputs |
| List runs | Configuration, environment, purpose, completion state, and timestamps |
| Query typed results | Selected dimensions, work items, raw outcomes, and provenance |
| Inspect run/work item/attempt | Detailed outcomes, diagnostics, commit information, and artifact references |

Input descriptors need stable field paths, labels, value types, and permitted
filter/group operations. `BenchmarkIn` does not currently provide this interface.
Prefer explicit experiment metadata or a reusable derive/helper over inferring
semantics from arbitrary JSON. Modest suites can be filtered on the server using
typed inputs; persisted parameter projections are a later optimization if needed.

Input selection supports all target inputs, explicit IDs, or a typed predicate
with exclusions. Search and pagination must not change selection semantics.
“Select all matching” selects the whole matching set, not the visible page. Group
counts use the same selection, including missing parameter values explicitly.

A result selection specifies:

```text
experiment and target version
source-version policy
revision order and selection: all, range, or explicit IDs
input selection
execution environment and measurement contract
run-selection policy
```

Revision order is explicit and retained even when no measurements exist. Do not
infer it from timestamps or SQL row order. Ranges refer to that order. Resolve Git
references to commit IDs and preserve repository/history identity; combining runs
with inconsistent revision histories requires an explicit order or an error.

The typed `ResultFrame<I, O>` contains ordered revisions, selected typed inputs
keyed by stable ID, source definitions, selected runs/work items, attempts with
`AttemptOutcome<O>`, and execution/coverage information. It retains empty
dimensions. A sparse representation is sufficient; the Store need not allocate
every possible revision/input/sample combination.

## Selecting runs and repetitions

Supported policies should include explicit run IDs, latest completed work item
per revision/input, and all matching runs. Multiple runs remain separate in the
returned frame. Environment selection is explicit; the proposed default is one
environment, with mixed environments requiring a deliberate choice.

For the proposed normal history policy:

1. Restrict candidates by experiment, version compatibility, environment,
   measurement contract, and measurement purpose.
2. For each revision/input, choose the latest completed work item using a recorded
   ordering key with a stable ID tie-breaker.
3. Retain all its repetitions and their outcomes. Completion is independent of
   measurement success; a failed newer execution must not disappear in favor of
   an older successful one.
4. Report newer unfinished candidates and any fallback to an older completed
   work item. If none is complete, retain the pending/missing state.

A work item can be complete while the rest of its run remains unfinished. This
supports incremental collection without mixing sample 0 from yesterday with sample
1 from today. Because selection is per input, a suite point may contain several
run IDs; expose that provenance. Explicit run selection supports inspecting a
single invocation instead.

Retries are not repetitions. If retry support is introduced, choose one terminal
attempt per repetition using a declared policy and retain the other attempts for
inspection. Never add retries to the sample count implicitly.

## Metrics, analysis, and UI responses

`BenchmarkOut` remains the source of metric descriptors and values. Shared metric
metadata should also describe stable nested paths, supported reductions, and
appropriate defaults. The existing `random` flag alone does not establish valid
aggregation or uncertainty calculations.

Keep absent optional metrics distinct from failed attempts, unsupported inputs,
pending work, and exclusions. Nested `MetricValue::Samples` also need a declared
meaning: a collection of phases or components is not automatically a set of
independent benchmark repetitions.

An analysis plan contains the dataset selection, metric paths, repetition
reduction, treatment of multiple runs, input reduction, series grouping, and
coverage policy. Uncertainty options can be added later. Axis scale, limits,
colors, and panel layout remain presentation settings.

The reduction order is explicit:

```text
select target inputs and source work items
-> extract a metric from each valid repetition
-> summarize repetitions within each revision/input/run
-> summarize runs, if multiple runs were explicitly selected
-> combine input estimates within each series and revision
-> return points, units, coverage, and provenance
```

Give each input equal weight by default when computing a suite mean. More
repetitions improve that input's estimate; they must not increase its workload
weight. For multiple runs, require a declared choice such as reducing per-run
summaries or explicitly pooling observations. Preserve that choice in the plan.

Computed metrics should expose their formulas through shared analysis metadata
when aggregation order matters. For example, the mean of per-input ratios and
the ratio of suite means are different calculations. Ratios require matched
contributors and explicit handling of absent values or invalid denominators.

Supported coverage policies should include available inputs at each revision,
a fixed common input set across selected revisions, and strict coverage with gaps
for missing required measurements. Proposed history default: strict input
coverage against the selected target workload. This preserves workload meaning;
an available-input view remains useful when gaps would otherwise dominate.

Every point includes its estimate or gap, included and selected input counts,
valid repetition/run counts, exclusion reasons, and references to the selected
records. A compact response may defer detailed contributor lists to a follow-up
inspection query. Hiding missing-result details is a display choice and must not
change the calculation.

Serializable plans and resolved record IDs make responses deterministic. “Latest”
is a live selection and can change as data arrives; retain its resolved selection
when reproducing a particular response. Version analysis semantics as well if
exact replay is later required. Saved-view UX is deferred.

## Variation and future adaptive sampling

The UI must name the population behind a standard deviation:

| Quantity | Population |
| --- | --- |
| Within-run standard deviation | Repetitions of one input at one revision in one run |
| Between-run standard deviation | Per-run estimates for the same input and revision |
| Across-input standard deviation | Different workloads' estimates; workload spread rather than repeatability |
| Suite uncertainty interval | Uncertainty in the chosen suite estimator under a specified statistical model |

Proposed initial display: per-input mean, sample standard deviation, and count at
a selected commit, optionally plotted across commits. Sample standard deviation
is unavailable with fewer than two valid observations. A standard-deviation band
must be labeled as such; it is not a confidence interval.

Do not average input standard deviations to create a suite uncertainty band.
Retain the input/run structure for a later estimator. Cached reference observations
can be reused across revisions; identify each observation by a persistent ID or
run-scoped identity so repeated uses are not counted as independent measurements.
The current reference cache IDs are session-local and need that scope preserved.

Adaptive sampling belongs to the runner. Storage should accept incremental,
unequal repetition counts and record the policy, baseline, budget, and stopping
reason. The eventual decision may be meaningful change, within tolerance, or
inconclusive at the budget. Selecting a statistically valid stopping method and
suite-level uncertainty estimator is deferred. A completed work item must not
require every input to have the same fixed sample count.

## Database shape and performance

The initial relational model is approximately:

```text
experiments
suite_definitions
inputs
suite_inputs             definition membership and display order
revisions                repository-scoped commit metadata
runs                     definition, options, environment, purpose, state
run_revisions            frozen selected revision order and setup/build state
run_inputs               frozen selected input order
work_items               run/revision/input completion and sampling state
attempts                 repetition, timestamps, outcome, typed output payload
artifact_references      retained artifact locations and availability
```

This is a logical model, not a required table-per-entity migration. Reference
observations can initially remain in typed payloads if their identity and scope
survive decoding; normalization is optional.

Use relational fields and indexes for joins, selection, statuses, ordering, and
timestamps. Store benchmark payloads as serialized typed data, initially JSON.
Push experiment, definition, input, revision, and run selection into SQL before
loading and decoding outputs. Load large diagnostics and artifacts only for
inspection. Multi-query reads should use a consistent database snapshot.

No serialization performance penalty has been measured. Typed decoding avoids a
mandatory intermediate generic tree. Measure representative history queries
before adding metric projections, caches, or separate databases. If projections
become necessary, treat them as derived data tied to payload and metric semantics.

## Implementation sequence

1. Define persistent identities, input matching, measurement contracts, and
   immutable suite-version registration.
2. Implement one database, transactional registration, frozen run plans, and
   incremental typed attempt writes.
3. Implement version projection and explicit run selection, returning dimensions
   and missing-state information with raw observations.
4. Add generic adapter dispatch, metric/input descriptions, filtering, grouping,
   and analysis responses.
5. Add run inspection and clearly scoped standard-deviation displays.
6. Design suite comparison, saved views, and adaptive measurement separately.

The defaults still requiring product decisions are source-version selection,
environment grouping, latest-work-item ordering, multiple-run aggregation, and
coverage. Their proposed behavior is documented above rather than hidden in SQL.
The statistical stopping rule and suite uncertainty model remain open.

Useful acceptance cases include reordered inputs retaining identity; later minor
versions projecting onto an earlier target; overlapping runs retaining complete
repetition groups; newer failures remaining visible; interrupted work preserving
completed observations; and input paging leaving the selected workload unchanged.

## Existing code anchors

- [Typed procedures and outputs](../src/experiment/benchmark.rs): `Procedure`,
  `BenchmarkIn`, and `BenchmarkOut`.
- [Suite comparison](../src/experiment/suite.rs): containment-based versioning;
  persistence still needs to establish the authoritative lineage.
- [Attempt and report types](../src/experiment/result.rs): typed outcomes and
  repetitions; `input_index` currently refers to local definition order.
- [Execution](../src/experiment/mod.rs): currently collects a report in memory;
  incremental Store writes and frozen persisted plans are future work.
- [Metrics](../src/metrics.rs): nested descriptors and generic metric values.
- [Reference cache](../src/experiment/reference/cache.rs): observations reused
  within a session.
- [Store sketch](../src/store/mod.rs): explicitly temporary; replace it with the
  persistent model rather than treating its current identifiers as a contract.
- [Analysis module](../src/analysis/mod.rs): a placeholder for the analysis boundary.

V1 references are in the sibling `benchmarks` checkout:
[analysis pipeline](../../benchmarks/src/analysis/pipeline.rs),
[generic controls](../../benchmarks/webapp/src/controls.rs),
[catalog discovery](../../benchmarks/webapp/src/providers/mod.rs), and
[typed row decoding](../../benchmarks/src/store/sqlite/rows.rs).
