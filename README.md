# TODO

- Revert README later
- remove unecessary comments
- Adding invariants, even though functions can fail, most should not when used "correctly"

# Experiment model

- `Procedure` defines execution, its input/output types, runtime tools, and a
  stable name. The resolved `ProcedureId` carries that name and the computed suite version.
- `BenchmarkIn` identifies one workload. It supports equality, hashing, and
  serialization. Tested revisions, executable locations, workspaces, and artifact
  policy are separate runtime values.
- `BenchmarkOut` retains one successful execution's observations. Equal inputs
  may produce different outputs or a mix of successes and failures.
- `Suite` supplies a procedure, generates inputs, and binds the built tool to
  the procedure's runtime tools.

Call `Suite::definition()` afresh for each experiment invocation. It generates
the actual inputs and rejects duplicates. Use those inputs across tested
revisions and record them with the results. Previous definitions describe what
ran; they are not a generator cache. Generator settings may be retained by a
concrete suite as provenance, but comparison uses actual inputs.

`SuiteDefinition::compare` compares procedure names and input membership,
ignoring input order and assigned versions. `with_previous` resolves one shared
suite/procedure version: keep it for equal membership, bump minor for a strict
superset, and bump major (resetting minor) otherwise. New suites start at 1.0.
Major version bumps print a warning about the incompatible suite change.
`Experiment::run_with_previous` accepts a previous recorded definition; the store
will supply it once implemented. Procedures have no manual version constants.
Source-only changes to commands, timing boundaries, or parsing still need
procedure-definition tracking; membership comparison cannot detect those changes.

`YosysBenchmark<P>` adapts a `YosysProcedure` to `Procedure`. Its inputs contain
a design and configuration; the installed `Yosys` is passed separately. Designs
contain an ordered file list, include-only headers, frontend options (language,
defines, include paths, extra arguments), an optional top, and parameter overrides.
Source files retain their logical paths and contents. Moving a design root does
not change identity; changing contents, logical paths, order, or options does.
Cell libraries likewise retain their contents.

Results are grouped by tested revision, then suite procedure/version, then input.
Each input retains an ordered list of `BenchmarkAttempt` values with a start
time, duration, and success output or failure. Attempts are never deduplicated
by their outcome. Benchmarks can retain phase repetitions inside their output;
these remain distinct from attempts of the entire procedure.

The `store` module is a deprecated interface sketch. Its types and generics
illustrate the intended relationships; the API and serialized format will change
when the actual store is implemented.

Experiment orchestration, process timing, and concrete MCM execution are still
unimplemented.

# Features

- Confidence interval testing per benchmark, rerun until that threshold
  - How to measure this accross benchmarks, v1.0 = [1,2,3], v1.1 = [4,1,5,2,6,3], how can we compare the subset of v1.1 corresponding to v1.0
  - How to get a threshold for the first time tests are ran, can / should we backtest? recursively?
- Metrics Analyzing
  - Confidence intervals need to know which are random / what variables they can change with
  - Display needs to know what metrics matter to the user
  - Defining Custom metrics, like time relative to another tool (MCM)
