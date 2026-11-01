# Storage and UI data design

Status: the initial backend and five-mode UI are implemented.
See [backend usage](backend.md) for the concrete API, defaults, and scope. Rust
snippets below describe the design boundaries; exact method signatures are in
code. See [UI usage](ui.md) for the workspace, sidebar, and Plotly integration.
Explicitly deferred analyses remain future work.

Selected approaches: `rusqlite` for database access and `inventory` for distributed
adapter registration. Shared browser-safe values live in `crates/ui-types`.

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

This document covers the data and analysis contracts needed for those
responsibilities, including proposed sidebar modes. Detailed visual layout,
suite-version comparison screens, saved views, and adaptive sampling are later
work.

## Use cases

One person may fill several roles. The groups describe what they are trying to do.

### UI users

| Use case                          | Required data or operation                                                                                                           |
| --------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| Find an experiment                | Name, description, available versions, creation time, latest measurement time, and clearly labeled result counts                     |
| Select a workload version         | Target version and its inputs; exact-version results or projection from compatible later minor versions                              |
| View metrics across commits       | Explicit revision order, precise commit selection, selected inputs, measurements, and units                                          |
| Narrow a large suite              | Searchable input descriptions, typed parameter filters, matching counts, paging, and exclusions                                      |
| Select runs and environments      | Run metadata and explicit selection policies so reruns or different machines are not silently combined                               |
| Change aggregation                | Raw observations and separate choices for repetitions, runs when selected, and inputs                                                |
| Split a chart into series         | Group by input identity or parameter; preview group values and counts before creating many series                                    |
| Investigate a run                 | Run configuration, environment, source version, commit details, per-input summaries, individual attempts, diagnostics, and artifacts |
| Explain missing points            | Optional display of build failures, failed or unsupported attempts, unfinished work, unmeasured work, and missing metrics            |
| Inspect variation at a commit     | Sample counts and standard deviation with an explicit population: repetitions within a run or summaries across runs                  |
| Check what contributes to a point | Included inputs and observations, exclusions, coverage, selected run IDs, and any fallback to older results                          |
| Compare suite definitions, later  | Added, removed, and retained input identities; compatibility and matching rules                                                      |

### Experiment authors

| Use case                                | Required data or operation                                                                                                                |
| --------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| Add an experiment type                  | Implement a procedure and suite with typed, serializable inputs and outputs; register the generic read adapter beside the procedure        |
| Reuse execution logic                   | Compose existing synthesis, tool, measurement, and reference components, as MCM and synthesis already do                                  |
| Make results appear in the UI           | Declare output metrics, units, descriptions, and supported reductions through shared interfaces, without benchmark-specific frontend code |
| Make inputs searchable and groupable    | Declare input labels and typed parameter fields with supported filter and grouping operations                                             |
| Evolve a suite                          | Preserve stable input identities; additions produce compatible minor versions, while incompatible changes advance the major version       |
| Evolve result formats or metric meaning | Declare payload schemas and measurement contracts so incompatible data is rejected or explicitly migrated                                 |
| Access results in code                  | Read concrete input and output types through a validated Store handle and reuse the metric interface for analysis                         |

### Benchmark operators

| Use case                            | Required data or operation                                                                                                |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| Run an experiment across commits    | Select the suite, revisions, inputs, repetitions, execution options, and environment; persist the resolved run plan       |
| Publish results to local discovery  | Register the experiment, definition, and planned run together so results appear in the UI without a separate catalog step |
| Collect more measurements           | Create a new run while preserving previous runs, raw repetitions, and provenance                                          |
| Inspect partial or failed execution | Incrementally persisted outcomes, build failures, diagnostics, and distinct unfinished or interrupted states              |
| Collect diagnostic artifacts        | Choose artifact retention and distinguish diagnostic executions from normal measurements                                  |
| Measure adaptively, later           | Configure the metric, baseline, tolerance, and sample budget; retain observations, progress, and stopping reasons         |

A dedicated two-commit comparison screen is not an initial requirement. Precise
commit selection and run inspection are more useful first. Reproducibility is a
system property: a response should identify the records used, even before saved
views exist.

## Analysis use cases

These are candidate workflows, not commitments to implement every analysis.
They use the same typed results and metric interfaces as the history view.
Comparisons can be overlays, filters, and inspection tables within that view;
they do not require a separate two-commit comparison screen.

### UI users

| Use case | Required data or operation |
| --- | --- |
| Read relative performance over time | Normalize each input to a selected baseline revision before combining inputs; show matched coverage and handle zero or absent baselines explicitly |
| Find where a trend changed | Absolute or relative changes between adjacent selected revisions, with input matching and commit details; indicate gaps between measured commits |
| Find the inputs behind a change | Rank per-input changes and, where the suite estimator permits it, their contributions to the aggregate; allow opening the raw measurements |
| Distinguish broad changes from isolated regressions | Counts and distributions of improved, unchanged, and worsened inputs using a user-selected practical threshold and the metric's improvement direction |
| See what the average hides | Median, selected percentiles, and worst inputs across per-input estimates at each revision; keep workload spread separate from repetition noise |
| See how performance scales with input parameters | Group or plot estimates against width, coefficient count, design size, or other declared parameters, while controlling the remaining parameters |
| Check whether a trend is caused by changing coverage | Recompute using a fixed common input set and show membership differences, failures, and missing metrics alongside the original view |
| Explore relationships between metrics | Plot metrics such as runtime and area for the same input/revision/run estimates; retain grouping and provenance rather than correlating unrelated samples |
| Inspect time versus result-quality tradeoffs | View runtime alongside area or another quality metric; later identify choices that improve one metric without worsening the others |

### Experiment authors

| Use case | Required data or operation |
| --- | --- |
| Evaluate an optimization against its baseline or reference | Extract baseline, optimized, and reference metrics from typed outputs; declare the ratio or savings formula and preserve reused reference observation identities |
| Identify which phase became expensive | Phase-level measurements with stable identities and units; distinguish inclusive timings from disjoint phases before summing them |
| Define an experiment-specific efficiency metric | Expose computed metrics, such as area saved per unit runtime, through `BenchmarkOut`; specify valid domains, units, and reduction order so generic analysis can use them |

### Benchmark operators

| Use case | Required data or operation |
| --- | --- |
| Find measurements that need more repetitions | Per-input counts, standard deviation, and raw distributions within each run; mark insufficient samples rather than treating them as stable |
| Distinguish repetition noise from changes between invocations | Compare within-run variation with per-run estimates at the same revision/input, grouped by environment and execution options |
| Spot drift during collection | Measurement timestamps and execution order within a fixed revision/input/configuration; inspect trends or outliers without automatically discarding them |
| Track reliability across commits | Separate counts of success, failure, unsupported, unfinished, and unmeasured work against a fixed selected workload |
| Allocate further measurements, later | Use missing coverage and a defined uncertainty target to suggest revision/input work items within a sample budget; retain the scheduling policy and decisions |

The most useful first additions are normalized histories, per-input change
rankings, parameter scaling, coverage checks, and within-run variation. They help
explain an existing chart without requiring a new statistical decision system.
Automatic change detection, confidence-based decisions, and suite uncertainty
intervals remain later work.

Analysis needs a few shared capabilities beyond scalar extraction: metric
direction where meaningful, normalization and formula rules, input parameter
descriptors, and observation provenance. These belong in experiment/analysis
interfaces, not per-experiment UI code. A practical change threshold is a display
or analysis choice; it does not by itself establish statistical significance.

### Proposed analysis modes

Start with History, Changes, Scaling, Variation, and Coverage in the analysis
selector. These are modes within the existing workspace, not separate pages or
benchmark-specific providers. Metric relationships can follow later.

Keep experiment/version, revisions, inputs, environment/configuration, and run
selection shared. Each mode exposes only its relevant controls: normalization in
History, endpoints and thresholds in Changes, an X parameter in Scaling, and a
named sample population in Variation. Coverage must work without selecting a
scalar metric. Metric availability can be an additional coverage layer.

Preserve compatible dataset choices when switching modes. Validate mode-specific
settings and explain any required changes rather than silently changing the
selected workload. Keep the current recipe visible, including coverage policy,
run treatment, and any normalization or calculation exclusions. Plot styling
remains separate from analysis settings.

## Persistent identities

| Entity           | Meaning                                                                                                    |
| ---------------- | ---------------------------------------------------------------------------------------------------------- |
| Experiment       | An ongoing named collection, such as MCM performance, scoped to its source repository and procedure family |
| Suite definition | An immutable, versioned workload definition with frozen inputs                                             |
| Input            | A stable identity for the complete benchmark configuration                                                 |
| Revision         | A commit identity within a repository, with descriptive commit metadata                                    |
| Run              | One invocation against a definition, selected revisions and inputs, execution options, and environment     |
| Work item        | One `(run, revision, input)` execution, containing its repetitions and completion state                    |
| Attempt          | One execution of one repetition, with its own identity, outcome, and provenance                            |

Use opaque IDs internally. Names and timestamps describe records; they do not
identify a dataset. **The current Rust `Experiment<S>` represents an execution
configuration, while “experiment” here means the persistent collection. Naming
can be reconciled when the driver is implemented.**

The experiment picker first selects an experiment, then a version. The proposed
default is its latest registered version. Show whether that version has results;
do not silently substitute a different workload because a newer version is empty.
Also show `last_measured_at` when choosing experiment or at least in metadata.
Define counts explicitly, for example successful attempts and measured revisions.

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

## Database access library

The database library sits below the Store. It maps relational rows and executes
queries; the Store still enforces suite compatibility, run selection, payload
schema identity, and typed decoding.

Decision: use [rusqlite](https://docs.rs/rusqlite/latest/rusqlite/). Its direct,
synchronous SQLite API fits the initial Store. Keep SQL, parameter binding, row
mapping, migrations, and connection handling inside `SqliteStore`; public methods
return domain types rather than database rows.

All procedures use the common relational schema, with benchmark-specific data in
versioned payloads. Serde decoding and Store validation check that a payload
matches the expected `P::Output` and measurement contract. New procedures do not
need their own SQL tables or row models.

Alternatives considered:

| Option | Benefit | Fit and cost for this project |
| --- | --- | --- |
| [rusqlite](https://docs.rs/rusqlite/latest/rusqlite/) | Direct SQLite access with prepared statements and row decoding | Small synchronous implementation; write SQL and row mapping explicitly, with query errors generally checked at runtime |
| [Diesel](https://diesel.rs/guides/getting-started.html) | Typed query construction, row derives, and migration tooling | Useful if relational compile-time checks justify schema/derive machinery; runtime-built selections still need deliberate query construction |
| [SQLx](https://docs.rs/sqlx/latest/sqlx/) | SQL-oriented async API and optional compile-time checked query macros | An alternative if the server benefits from async database access; checked macros need a schema database or prepared offline metadata, and do not check arbitrary runtime-built SQL |
| [SeaQuery](https://docs.rs/sea-query/latest/sea_query/) with a driver | Compose dynamic SQL and parameter bindings without a full ORM | An optional layer if building selection queries by hand becomes awkward; still needs execution and row decoding |
| [SeaORM](https://github.com/SeaQL/sea-orm) | Entity models, relationships, filtering, and pagination | Worth considering if model-oriented application code becomes the main workload; more abstraction than this Store currently requires |

The SQLx macro requirements are documented in its
[query macro reference](https://docs.rs/sqlx/latest/sqlx/macro.query.html).

The decision is based on fit and implementation simplicity. No performance
advantage over the alternatives has been measured in this project.

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

// Distributed registrations supply factories for these adapters; see below.
```

The registry can hold `Box<dyn ExperimentAdapter>`. `TypedAdapter<P>` implements
the operations generically. Reject conflicting registrations, and report stored
experiments with unavailable adapters as unsupported rather than hiding them.
Keep discovery metadata readable without decoding outputs.

### Register adapters beside their procedures

A concrete decoder must be instantiated for each supported procedure/schema
combination. Its implementation is generic; a separate manual
`registry.register::<McmProcedure>()` call is not required. Rust does not provide
a runtime enumeration of every type implementing a trait, so those concrete
types must enter the executable through an explicit registration.

Decision: use distributed registration through
[inventory](https://docs.rs/inventory/latest/inventory/) or
[linkme](https://docs.rs/linkme/latest/linkme/). Each procedure module registers a
descriptor containing its persistent identities and a factory for the generic
`TypedAdapter<P>`. The runner and UI server discover these descriptors from linked
code. Authors maintain the registration beside the procedure rather than updating
a central catalogue or a separate list in each executable.

```rust
// Illustrative inventory form, beside McmProcedure.
// AdapterRegistration and its const constructor are proposed shared helpers.
inventory::submit! {
    AdapterRegistration::for_procedure::<McmProcedure>()
}
```

With `inventory`, the shared registry declares `inventory::collect!` once and
iterates submitted descriptors. With `linkme`, descriptors are entries in a
shared distributed slice. Choose one crate during implementation; the descriptor
and typed adapter boundaries are the same in either case.

Registration instantiates a generic factory using `P::Input` and `P::Output`.
It must validate the required `StoredProcedure` and input-description bounds at
compile time. Creating or discovering descriptors does not construct suites,
initialize tools, or execute benchmarks. Typed suite configuration remains the
runner's responsibility. Multiple suites using one procedure share its registered
decoder; older schemas can have read-only registrations.

The procedure's module or crate must be included in each executable that needs
its decoder. Implementing a trait alone does not register a type. Discovery must
sort descriptors by stable keys and reject conflicting identities, so linker or
iteration order cannot affect behavior. Missing decoders remain visible as
unsupported stored experiments.

Registering an adapter only when a benchmark runs is insufficient: a fresh UI
process must also decode historical results without running any experiment.
Persisting a procedure key or Rust type name does not supply executable decoder
code. The distributed registrations supply that code independently of run
execution.

### Generic metric access

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

| Operation                        | Returns                                                                     |
| -------------------------------- | --------------------------------------------------------------------------- |
| List experiments                 | Discovery summaries and availability information                            |
| Describe experiment and versions | Definitions, compatibility, counts, and supported contracts                 |
| List revisions                   | Ordered commit metadata, including revisions without measurements           |
| Describe/search inputs           | Parameter descriptors, paged descriptions, and matching counts              |
| Describe groups                  | Group values and counts for the selected target inputs                      |
| List runs                        | Configuration, environment, purpose, completion state, and timestamps       |
| Query typed results              | Selected dimensions, work items, raw outcomes, and provenance               |
| Inspect run/work item/attempt    | Detailed outcomes, diagnostics, commit information, and artifact references |

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
execution-option filters and comparison partitions
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
per revision/input within each retained environment/configuration partition, and
all matching runs. Multiple runs remain separate in the returned frame.
Environment selection is explicit; the proposed default is one environment,
with mixed environments requiring a deliberate choice.

For the proposed normal history policy:

1. Restrict candidates by experiment, version compatibility, environment,
   execution-option filters, measurement contract, and measurement purpose.
2. Within each retained partition, for each revision/input, choose the latest
   completed work item using a recorded ordering key with a stable ID tie-breaker.
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

### Capabilities and plan validation

`BenchmarkOut` remains the source of metric descriptors and values. Shared
descriptors need the following information so the generic UI can offer valid
controls:

| Descriptor | Required information |
| --- | --- |
| Metric | Stable nested path, label, description, value type, unit, valid domain, and improvement direction: lower, higher, or unspecified |
| Reduction | Supported repetition, run, and input reductions; defaults; minimum counts; domain requirements; and resulting units |
| Formula | Operand paths, evaluation stage, matching rules, valid denominators, resulting units, and any meaningful reference value |
| Dimension | Stable field path, label, type, ordering, and supported filtering, grouping, and X-axis roles |
| Operation | Required metrics/dimensions, supported transformations, defaults, and result shape |

The existing `random` flag alone does not establish valid aggregation or
uncertainty calculations. Reductions are capabilities per axis, not one universal
menu. For example, count returns inputs or observations rather than the metric's
unit; an optional geometric mean for positive normalized ratios requires strictly
positive contributors. Specify percentile interpolation and use sample standard
deviation for repetition estimates as described below.

Validate the complete plan on the server, including combinations of operation,
metric, formula, reduction, and grouping. Return structured errors associated with
the relevant control. Descriptions should let the UI explain unavailable choices;
selection-dependent constraints can be checked during analysis. Do not silently
substitute a reduction or discard invalid values. Domain failures retain a reason
and participate in the chosen coverage policy.

Keep absent optional metrics distinct from failed attempts, unsupported inputs,
pending work, and exclusions. Nested `MetricValue::Samples` also need a declared
meaning: a collection of phases or components is not automatically a set of
independent benchmark repetitions.

### Operations and result shapes

An analysis plan contains the dataset selection, operation, metric paths,
repetition reduction, treatment of multiple runs, input reduction, dimensions,
normalization/comparison settings, staged filters, and coverage policy.
Uncertainty options can be added later. Selecting the X dimension affects the
calculation; axis scale, limits, colors, and panel layout remain presentation
settings.

Operations share typed extraction and estimation stages, but must not all be
forced into commit/value points:

| Operation | Required settings | Result shape |
| --- | --- | --- |
| History | Metric, revision order, optional baseline normalization, input estimator, and series dimensions | Ordered estimates or gaps with units and coverage |
| Changes | Explicit revision pair or adjacent-revision policy, metric, change formula, threshold, and ranking | Per-input rows with endpoint estimates and changes, matched/unmatched counts, classifications, and optional aggregate contributions |
| Scaling | Metric, typed X parameter, selected revisions, series dimensions, and treatment of remaining parameters | Typed X values and estimates, preserving distinct revisions and parameter groups |
| Variation | Metric, revision/input selection, within-run or between-run population, and requested summaries | Counts and summary statistics with paged observations or distributions |
| Coverage | Selected workload and revisions, run policy, optional metric | Status counts and revision/input cells, including cells with no scalar measurement |
| Metric relationships, later | Two metrics, common estimate keys, and series dimensions | Paired numeric values with joint coverage and provenance |

Use shared serializable result types with typed values and columns, stable row or
point IDs, units, and inspection references. Sorting numeric change rows must not
depend on formatted strings. Return stable sort keys, a declared tie-breaker, and
matched totals when paging rankings. Distribution responses identify their
population and quantile or histogram conventions. Relationship points join on
input, revision, and retained run/configuration dimensions; independently reduced
or unrelated samples must not be paired by position.

### Estimation and formula order

The common estimation order is explicit. Operations may retain intermediate
dimensions rather than reducing them:

```text
resolve target inputs, revisions, partitions, and source work items
-> extract metric operands from valid repetitions
-> apply declared repetition-level formulas and calculation filters
-> summarize repetitions within each revision/input/run
-> summarize runs within each partition, if explicitly requested
-> evaluate declared estimate-level formulas
-> apply per-input-estimate calculation filters
-> normalize against revision baselines or compute per-input changes, if requested
-> resolve any fixed cohort and coverage requirements
-> combine input estimates within each retained group, if requested
-> evaluate declared suite-level formulas over matched operands
-> return operation-specific results, units, coverage, and provenance
```

Give each input equal weight by default when computing a suite mean. More
repetitions improve that input's estimate; they must not increase its workload
weight. For multiple runs, require a declared choice such as reducing per-run
summaries or explicitly pooling observations. Preserve that choice in the plan.

Computed metrics expose their evaluation stage through shared metadata. A scalar
computed in `get_metric` for each output is a repetition-level formula; averaging
it cannot implement a ratio of reduced operands. A formula requiring a later
stage must expose its operands and composition rule. Mean per-input ratios and
ratio of suite means are separate named calculations. The latter reduces both
operands over the same contributors before division. Formula evaluation must
preserve observation identities, including reused references.

### Baselines and changes

A revision baseline is separate from a baseline or reference tool embedded in an
output. History normalization selects an explicit baseline revision, which may be
outside the displayed range but must be resolved and included in provenance.
Proposed default: apply the same environment/configuration partitions and
run-selection policy at baseline and displayed revisions. An override must be
explicit and visible. Workload and measurement compatibility rules still apply.

Normalize each input estimate against its matched baseline before combining
inputs. Offer ratio or percent change only where the formula's domain permits it.
For positive baselines, ratio is `current / baseline` and percent change is
`100 * (current - baseline) / baseline`. Absolute change is `current - baseline`.
Zero, absent, or invalid denominators produce an explained unavailable value;
there is no implicit epsilon or fallback revision. Signed metrics need an
explicit relative-change convention or use absolute change. Record whether the
formula measures change or improvement; lower-is-better metrics reverse the
interpretation of the change sign. A change in an already percentage-valued
metric must distinguish percentage points from relative percent change.

Changes accepts explicit endpoint revisions or adjacent revisions in the selected
order. Proposed default: adjacent selected revisions; a missing endpoint creates
a gap. A separate previous-measured policy may skip gaps, but must identify the
actual endpoints and skipped revisions. Neither policy claims that a change was
introduced by one commit when intermediate commits were not measured.

Match endpoints by stable input identity within compatible measurement and
environment/configuration partitions. Report baseline-only, current-only, and
invalid pairs separately. Aggregate endpoint comparisons use the same matched
contributors on both sides, subject to strict/common/available coverage settings.
Thresholds declare absolute units or relative percent and classify valid pairs
as improved, within threshold, or worsened only when direction is meaningful.
They are practical thresholds, not significance tests.

Rankings declare the change measure, direction, limit, and stable ID tie-breaker.
Largest per-input regression and largest contribution to aggregate change are
different rankings. Return contributions only for estimators with a defined
decomposition. For an equal-weight arithmetic mean on a fixed set, each input's
absolute contribution is its delta divided by the matched input count. Nonlinear
estimators require their own rule or report contribution as unsupported.

### Filtering, dimensions, and cohorts

Keep three filter roles explicit:

- Dataset predicates select inputs and run metadata before measurement analysis.
- Calculation filters exclude observations or estimates at a declared stage and
  change the calculation. Retain excluded identities, counts, and reasons.
- Display filters hide returned rows or series without changing estimates or
  coverage denominators. Top-N display is not an input reduction.

Initially support typed input/run predicates, outcome or per-input-estimate
calculation filters, and display filters for results. Arbitrary raw-repetition
filters are later work; invalid-domain handling is always explicit. Reject a
filter whose field or stage is unsupported. V1's outcome and measurement filters
must not become ambiguous generic predicates in V2. Outcome filters declare
whether they test individual attempts or a work-item outcome summary.

A query such as "show inputs regressing by more than 5%" may define a fixed
cohort from explicit comparison endpoints and then plot that cohort's history.
Resolve its input IDs before applying it across revisions. A per-revision value
filter instead creates changing membership; label that behavior and apply the
coverage policy to the exclusions. Calculation exclusions do not silently shrink
the selected workload or turn strict coverage into available-input coverage.

Dimensions include input identity and declared parameters, revision, run,
environment, and execution options. Resolve run selection within retained
environment/configuration partitions so the latest run on one machine cannot
replace another machine's series. Partition before run reduction; combining
partitions requires an explicit plan choice. Group previews return typed values,
missing values, and counts for the same selection used by analysis.

For scaling, remaining varying parameters must be fixed by selection, retained
as series dimensions, or explicitly reduced. Proposed default: require a choice
rather than silently average them. Preserve parameter ordering and missing-value
handling, and preview series counts before producing high-cardinality plots.

### Coverage and inspection

Supported coverage policies should include available inputs at each revision,
a fixed common input set across selected revisions, and strict coverage with gaps
for missing required measurements. Proposed history default: strict input
coverage against the selected target workload. This preserves workload meaning;
an available-input view remains useful when gaps would otherwise dominate.

Define common-set membership per retained series/partition after required metric
extraction and calculation filters. Normalized histories also require a valid
baseline for each contributor; multi-metric operations require joint metric
availability. Return selected and eligible counts and membership differences so
a common-set result cannot conceal lost inputs. An empty common set is an
explained empty result.

Every point includes its estimate or gap, included and selected input counts,
valid repetition/run counts, exclusion reasons, and references to the selected
records. A compact response may defer detailed contributor lists to a follow-up
inspection query. Hiding missing-result details is a display choice and must not
change the calculation.

Coverage mode uses the fixed selected revision/input workload as its denominator
within each partition, including unmeasured work. Report execution state, attempt
outcomes, and metric availability separately: one completed work item can contain
both successful and failed repetitions. Distinguish revision-level build failures,
unsupported, unfinished/interrupted, and unmeasured work. Label whether a count
refers to inputs, work items, runs, or attempts; do not present overlapping counts
as one success-rate denominator. Multiple-run views declare their counting unit.

Inspection proceeds from result row/point to contributing input estimates, run
summaries, attempts, diagnostics, and artifacts. Requests carry the analysis
result identity, row/point identity, paging, and sorting. Use stable domain IDs
rather than chart-array positions. Excluded and unmatched contributors must also
be inspectable.

Serializable plans and resolved selections make responses reproducible. "Latest"
is live and can change as data arrives; a drilldown must reuse the original
resolved records and observed states instead of resolving it again. Record IDs
alone are insufficient for unfinished attempts that can later complete. Retain
the result's resolved values/states for inspection, or detect a changed record
version and return an explicit expired-result response requiring refresh. Do not
hold a database transaction open across browser requests. Persistent replay and
saved-view UX are deferred; exact replay would also need versioned analysis
semantics.

## Variation and future adaptive sampling

The UI must name the population behind a standard deviation:

| Quantity                        | Population                                                                    |
| ------------------------------- | ----------------------------------------------------------------------------- |
| Within-run standard deviation   | Repetitions of one input at one revision in one run                           |
| Between-run standard deviation  | Per-run estimates for the same input and revision                             |
| Across-input standard deviation | Different workloads' estimates; workload spread rather than repeatability     |
| Suite uncertainty interval      | Uncertainty in the chosen suite estimator under a specified statistical model |

Proposed initial display: per-input mean, sample standard deviation, and count at
a selected commit, optionally plotted across commits. Sample standard deviation
is unavailable with fewer than two valid observations. A standard-deviation band
must be labeled as such; it is not a confidence interval.

Variation responses also provide raw observations through paging, with run and
attempt IDs, timestamps, repetition indices, and execution order. Return requested
median, quantiles, and extrema from the same named population; any histogram
declares its bins and counts. Between-run distributions contain per-run estimates,
not pooled repetitions. Across-input distributions contain per-input estimates,
not repetition noise. Mark insufficient samples explicitly. Outliers remain
visible unless a supported calculation filter deliberately excludes them.

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
2. Implement one SQLite database through `rusqlite`, transactional registration,
   frozen run plans, and incremental typed attempt writes.
3. Implement version projection and explicit run selection, returning dimensions
   and missing-state information with raw observations.
4. Define shared operation/result types, capability descriptors, and complete
   plan validation. Add distributed adapter registration using `inventory` or
   `linkme`, generic dispatch, typed dimension descriptions, and staged filters.
5. Implement History and Changes with explicit formula stages, revision
   baselines, matched coverage, and per-input rankings. Add Scaling with explicit
   treatment of remaining parameters.
6. Add Coverage and Variation, scoped counts and distributions, and inspection
   using stable result identities and resolved record states. Connect the five
   proposed sidebar modes to the shared dataset controls.
7. Design metric relationships, suite comparison, saved views, and adaptive
   measurement separately.

The initial implementation adopts compatible source versions, explicit selection
when multiple environment/configuration partitions exist, latest invocation
registration order, explicit multiple-run treatment, and strict input coverage.
A usable estimate requires one valid repetition; failures remain visible. Baseline
run selection defaults to the history policy. Changes use adjacent selected
revisions, and Scaling requires a choice for remaining varying parameters.
Inspection snapshots expire on any subsequent database write. See
[backend usage](backend.md) for details. The statistical stopping rule and suite
uncertainty model remain open.

Useful acceptance cases include reordered inputs retaining identity; later minor
versions projecting onto an earlier target; overlapping runs retaining complete
repetition groups; newer failures remaining visible; interrupted work preserving
completed observations; and input paging leaving the selected workload unchanged.

Analysis acceptance cases should also cover:

- Mean per-input ratios differing from ratio of suite means, with matched operands.
- A baseline outside the displayed range, and zero or missing baselines producing
  explained unavailable values without changing strict coverage.
- Adjacent-selected comparisons retaining missing endpoints; previous-measured
  comparisons naming skipped revisions.
- A fixed regression cohort retaining the same input IDs throughout its history.
- Display filters leaving estimates and coverage unchanged; calculation filters
  retaining exclusion reasons and stage information.
- Latest-run selection preserving separate environment/configuration series.
- Scaling refusing to silently combine remaining varying parameters.
- Numeric ranking and pagination retaining stable ordering for tied changes.
- Coverage retaining unmeasured cells and distinguishing mixed attempt outcomes
  from work-item completion and missing optional metrics.
- Variation retaining the named population and reporting unavailable sample SD
  for fewer than two observations.
- New results arriving between a chart query and inspection without silently
  changing the chart's contributors or observed states.

## Existing code anchors

- [Typed procedures and outputs](../src/experiment/benchmark.rs): `Procedure`,
  `BenchmarkIn`, and `BenchmarkOut`.
- [Suite comparison](../src/experiment/suite.rs): containment-based versioning;
  persistence still needs to establish the authoritative lineage.
- [Attempt and report types](../src/experiment/result.rs): typed outcomes and
  repetitions; `input_index` currently refers to local definition order.
- [Execution](../src/experiment/mod.rs): currently collects a report in memory;
  incremental Store writes and frozen persisted plans are implemented.
- [Metrics](../src/metrics.rs): nested descriptors and generic metric values.
- [Reference cache](../src/experiment/reference/cache.rs): observations reused
  within a session.
- [Store sketch](../src/store/mod.rs): the SQLite implementation and persistent read/write model.
- [Analysis module](../src/analysis/mod.rs): generic analysis, registration, and the backend service boundary.

V1 references are in the sibling `benchmarks` checkout:
[analysis pipeline](../../benchmarks/src/analysis/pipeline.rs),
[generic controls](../../benchmarks/webapp/src/controls.rs),
[catalog discovery](../../benchmarks/webapp/src/providers/mod.rs), and
[typed row decoding](../../benchmarks/src/store/sqlite/rows.rs).
