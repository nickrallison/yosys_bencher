# yosys_bencher

Storage, typed adapter registration, and the five initial analysis modes are
implemented, together with a V1-style Dioxus UI. See [UI usage](docs/ui.md) for
launching the workspace and [backend usage](docs/backend.md) for running benchmarks
and querying results.

```sh
cargo run --bin results -- list
cargo run --bin results -- analyze plan.json
```

see run_mcm for an example of how to actually run an experiment

```sh
cargo run --bin run_mcm
```

The database defaults to `output/results.sqlite`; set `YOSYS_BENCH_DATABASE` to
choose another path. Start the UI with `cd webapp` and
`dx serve --web --fullstack true`.
To share the UI on your tailnet, run `./scripts/serve`.
