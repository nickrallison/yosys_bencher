#!/bin/sh
printf 'paper\n' >> "${0%/*}/events"
printf '%s\n' "$*" >> "$0.calls"
printf '// Cost: 1 adds/subtracts 2 shifts 0 negations\n// Depth: 2\n'
