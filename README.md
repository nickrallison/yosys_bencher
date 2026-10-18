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
