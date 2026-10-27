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
| Add an experiment type                  | Implement a procedure and suite with typed, serializable inputs and outputs; register the common typed storage adapter                    |
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
