# Plan

See the [storage and UI data design](docs/storage-design.md) for the proposed
Store API, use cases, version selection, and typed analysis boundary.

## Use cases

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

### Experiment authors (subject to change but illustrative of design goals)

| Use case                                | Required data or operation                                                                                                                |
| --------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| Add an experiment type                  | Implement a procedure and suite with typed, serializable inputs and outputs; register the generic read adapter beside the procedure       |
| Reuse execution logic                   | Compose existing synthesis, tool, measurement, and reference components, as MCM and synthesis already do                                  |
| Make results appear in the UI           | Declare output metrics, units, descriptions, and supported reductions through shared interfaces, without benchmark-specific frontend code |
| Make inputs searchable and groupable    | Declare input labels and typed parameter fields with supported filter and grouping operations                                             |
| Evolve a suite                          | Preserve stable input identities; additions produce compatible minor versions, while incompatible changes advance the major version       |
| Evolve result formats or metric meaning | Declare payload schemas and measurement contracts so incompatible data is rejected or explicitly migrated                                 |
| Access results in code                  | Read concrete input and output types through a validated Store handle and reuse the metric interface for analysis                         |

### Benchmark operators (subject to change but illustrative of design goals)

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

| Use case                                             | Required data or operation                                                                                                                                |
| ---------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Read relative performance over time                  | Normalize each input to a selected baseline revision before combining inputs; show matched coverage and handle zero or absent baselines explicitly        |
| Find where a trend changed                           | Absolute or relative changes between adjacent selected revisions, with input matching and commit details; indicate gaps between measured commits          |
| Find the inputs behind a change                      | Rank per-input changes and, where the suite estimator permits it, their contributions to the aggregate; allow opening the raw measurements                |
| Distinguish broad changes from isolated regressions  | Counts and distributions of improved, unchanged, and worsened inputs using a user-selected practical threshold and the metric's improvement direction     |
| See what the average hides                           | Median, selected percentiles, and worst inputs across per-input estimates at each revision; keep workload spread separate from repetition noise           |
| See how performance scales with input parameters     | Group or plot estimates against width, coefficient count, design size, or other declared parameters, while controlling the remaining parameters           |
| Check whether a trend is caused by changing coverage | Recompute using a fixed common input set and show membership differences, failures, and missing metrics alongside the original view                       |
| Explore relationships between metrics                | Plot metrics such as runtime and area for the same input/revision/run estimates; retain grouping and provenance rather than correlating unrelated samples |
| Inspect time versus result-quality tradeoffs         | View runtime alongside area or another quality metric; later identify choices that improve one metric without worsening the others                        |

### Experiment authors

| Use case                                                   | Required data or operation                                                                                                                                               |
| ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Evaluate an optimization against its baseline or reference | Extract baseline, optimized, and reference metrics from typed outputs; declare the ratio or savings formula and preserve reused reference observation identities         |
| Identify which phase became expensive                      | Phase-level measurements with stable identities and units; distinguish inclusive timings from disjoint phases before summing them                                        |
| Define an experiment-specific efficiency metric            | Expose computed metrics, such as area saved per unit runtime, through `BenchmarkOut`; specify valid domains, units, and reduction order so generic analysis can use them |

### Benchmark operators

| Use case                                                      | Required data or operation                                                                                                                                    |
| ------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Find measurements that need more repetitions                  | Per-input counts, standard deviation, and raw distributions within each run; mark insufficient samples rather than treating them as stable                    |
| Distinguish repetition noise from changes between invocations | Compare within-run variation with per-run estimates at the same revision/input, grouped by environment and execution options                                  |
| Spot drift during collection                                  | Measurement timestamps and execution order within a fixed revision/input/configuration; inspect trends or outliers without automatically discarding them      |
| Track reliability across commits                              | Separate counts of success, failure, unsupported, unfinished, and unmeasured work against a fixed selected workload                                           |
| Allocate further measurements, later                          | Use missing coverage and a defined uncertainty target to suggest revision/input work items within a sample budget; retain the scheduling policy and decisions |

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

## Relationships

### Experiment

- Experiment has commits and has a Suite
- A suite has benchmarks
  - a suite is versioned according to it's benchmarks
  - supersets are a minor bump, otherwise its a major bump
- a benchmark has input parameters, and an output result
  - Most benchmarks should inherit from a generic SynthBenchmark
    - but can add to it / change the synth script being run
- Benchmark::Input should likely be a Yosys Path, but can also include other parts
  - coefficients for MCM
- Benchmark::Output should be "Outputable"
  - stored in DB, given to UI gracefully, analyzed, filtered, ...

# Features

- Confidence interval testing per benchmark, rerun until that threshold
  - How to measure this accross benchmarks, v1.0 = [1,2,3], v1.1 = [4,1,5,2,6,3], how can we compare the subset of v1.1 corresponding to v1.0
  - How to get a threshold for the first time tests are ran, can / should we backtest? recursively?
- Metrics Analyzing
  - Confidence intervals need to know which are random / what variables they can change with
  - Display needs to know what metrics matter to the user
  - Defining Custom metrics, like time relative to another tool (MCM)
