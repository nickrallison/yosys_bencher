# MCM dependencies

`NangateOpenCellLibrary_typical.lib` comes from OpenROAD-flow-scripts:
https://github.com/The-OpenROAD-Project/OpenROAD-flow-scripts/tree/master/flow/platforms/nangate45
Its license is in `NangateOpenCellLibrary.LICENSE`.

`download_acm.sh` downloads the Spiral MCM reference tool from
https://spiral.ece.cmu.edu/mcm/dl/synth-jan-14-2009.tar.gz into the ignored
`acm/` directory. It applies `synth.patch`, copied from the Yosys benchmark,
to add `<algorithm>` and fix the `print_set` argument types for compilation.
The archive includes the tool's GPL license and copyright notice.

Download and build the reference tool from the repository root:

```sh
./benchmarks/mcm/download_acm.sh
make -C benchmarks/mcm/acm acm
```

`run_mcm` uses these assets and an ignored `yosys/` checkout at the repository
root. Its origin is `https://github.com/YosysHQ/yosys.git`. The local checkout
must also contain the `nick/mcm` branch with the benchmark revisions; this
branch is preserved in the prepared checkout. The runner clones it into
`output/mcm/source/` for revision builds. Build output stays in `output/mcm/`.
