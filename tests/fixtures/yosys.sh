#!/bin/sh
script=
timing=
while [ "$#" -gt 0 ]; do
    case "$1" in
        -s) script=$2; shift ;;
        --perffile) timing=$2; shift ;;
    esac
    shift
done
has_mcm=0
synth=0
diagnostic=0
log=
emit() {
    printf '%s\n' "$1"
    if [ -n "$log" ]; then printf '%s\n' "$1" >> "$log"; fi
}
while IFS= read -r line; do
    log=
    case "$line" in
        'tee '*)
            line=${line#tee }; mode=${line%% *}; line=${line#* }
            log=${line%% *}; line=${line#* }
            if [ "$mode" = '-o' ]; then : > "$log"; fi ;;
    esac
    case "$line" in
        mcm*)
            has_mcm=1
            emit 'mcm: 2 constant multiplier(s) -> 1 adder(s), depth 2, cost 0' ;;
        synth*) synth=1; emit 'synthesis complete' ;;
        abc*) emit 'ABC mapping complete' ;;
        dfflibmap*) emit 'DFF mapping complete' ;;
        stat*)
            if [ "$synth" = 0 ]; then
                emit '{"modules":{"\\unoptimized":{"num_cells":1,"num_cells_by_type":{"$add":1}}},"design":{"num_cells":1,"num_cells_by_type":{"$add":1}}}'
            else
                area=40
                if [ "$has_mcm" = 1 ]; then area=20; fi
                emit "{\"modules\":{\"\\\\unoptimized\":{\"num_cells\":5,\"area\":$area,\"num_cells_by_type\":{\"NAND2_X1\":5}}},\"design\":{\"num_cells\":5,\"area\":$area,\"num_cells_by_type\":{\"NAND2_X1\":5}}}"
            fi ;;
        'write_verilog '*)
            diagnostic=1
            path=${line#write_verilog }; path=${path#\"}; path=${path%\"}
            printf 'module unoptimized; endmodule\n' > "$path" ;;
    esac
done < "$script"
if [ "$diagnostic" = 1 ] && [ -f "$0.fail-diagnostics" ]; then
    echo deliberate-diagnostic-error >&2
    exit 9
fi
phase=baseline
if [ "$has_mcm" = 1 ]; then phase=optimized; fi
if [ "$diagnostic" = 1 ]; then phase="$phase-diagnostic"; fi
printf '%s\n' "$phase" >> "${0%/*}/events"
if [ -n "$timing" ]; then
    printf '{"passes":{"mcm":{"runtime_ns":2500000,"num_calls":1}}}\n' > "$timing"
fi
