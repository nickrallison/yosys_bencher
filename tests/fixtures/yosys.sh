#!/bin/sh
has_mcm=0
synth=0
diagnostic=0
while IFS= read -r line; do
    case "$line" in
        mcm*) has_mcm=1 ;;
        synth*) synth=1 ;;
        'write_verilog '*)
            diagnostic=1
            path=${line#write_verilog }; path=${path#\"}; path=${path%\"}
            printf 'module unoptimized; endmodule\n' > "$path" ;;
    esac
done < "$4"
phase=mcm
area=0
if [ "$synth" = 1 ]; then
    phase=baseline; area=40
    if [ "$has_mcm" = 1 ]; then phase=optimized; area=20; fi
fi
if [ "$diagnostic" = 1 ]; then phase="$phase-diagnostic"; fi
printf '%s\n' "$phase" >> "${0%/*}/events"
printf 'mcm: 2 constant multiplier(s) -> 1 adder(s), depth 2, cost 0\n'
printf '=== unoptimized ===\n1 cells\n1 $add\n'
if [ "$area" != 0 ]; then printf "Chip area for module 'unoptimized': %s\n" "$area"; fi
