#!/bin/sh
report=
while [ "$#" -gt 0 ]; do
    case "$1" in
        -o) report=$2; shift ;;
        --) shift; break ;;
    esac
    shift
done
printf '%s\n' "$1" >> "$0.calls"
cat > "$report" <<'JSON'
{"counter-value":"1.25","unit":"msec","event":"task-clock","event-runtime":1250000,"pcnt-running":100}
{"counter-value":"100.5","unit":"","event":"cycles","event-runtime":625000,"pcnt-running":50}
{"counter-value":"200","unit":"","event":"instructions","event-runtime":1250000,"pcnt-running":100}
{"counter-value":"<not supported>","unit":"","event":"cache-misses","event-runtime":0,"pcnt-running":0}
JSON
"$@"
