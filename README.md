# Plan

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
