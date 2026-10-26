#!/usr/bin/env bash
# Run manually; the test harness and benchmark never invoke this script.
set -euo pipefail

mcm_tests=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
mcm_tools="$mcm_tests/../target/test-tools/mcm"
mkdir -p "$mcm_tools"
mcm_scratch=$(mktemp -d "$mcm_tools/.setup-XXXXXX")
trap 'rm -rf -- "$mcm_scratch"' EXIT

if [[ ! -f "$mcm_tools/NangateOpenCellLibrary_typical.lib" ]]; then
    curl -fL 'https://raw.githubusercontent.com/The-OpenROAD-Project/OpenROAD-flow-scripts/master/flow/platforms/nangate45/lib/NangateOpenCellLibrary_typical.lib' \
        -o "$mcm_scratch/cells.lib"
    mv "$mcm_scratch/cells.lib" "$mcm_tools/NangateOpenCellLibrary_typical.lib"
fi

if [[ ! -x "$mcm_tools/acm" ]]; then
    curl -fL 'https://spiral.ece.cmu.edu/mcm/dl/synth-jan-14-2009.tar.gz' \
        -o "$mcm_scratch/synth.tar.gz"
    tar -xzf "$mcm_scratch/synth.tar.gz" -C "$mcm_scratch"
    patch -d "$mcm_scratch/synth-jan-14-2009" -p1 < "$mcm_tests/support/hcub.patch"
    make -C "$mcm_scratch/synth-jan-14-2009" acm \
        CXX="${CXX:-c++}" CXXFLAGS="${CXXFLAGS:--O3 -std=gnu++11}"
    install -m 755 "$mcm_scratch/synth-jan-14-2009/acm" "$mcm_tools/acm"
fi

printf 'MCM test fixtures ready in %s\n' "$mcm_tools"
