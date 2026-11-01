#!/bin/sh
set -eu

dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' 0
trap 'exit 1' HUP INT TERM

curl -fL https://spiral.ece.cmu.edu/mcm/dl/synth-jan-14-2009.tar.gz \
    -o "$tmp/synth.tar.gz"
tar -xzf "$tmp/synth.tar.gz" -C "$tmp"
patch -d "$tmp/synth-jan-14-2009" -p1 < "$dir/synth.patch"
mkdir -p "$dir/acm"
cp -R "$tmp/synth-jan-14-2009/." "$dir/acm/"
