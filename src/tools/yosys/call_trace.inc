// Included inside the Yosys namespace in kernel/register.cc by the benchmark builder.
namespace {
struct YosysBenchCallTrace {
    struct Call {
        std::string name;
        int parent;
        int64_t begin_ns, runtime_ns;
    };
    const char *path = getenv("YOSYS_BENCH_CALL_TRACE");
    std::vector<Call> calls;
    std::vector<int> stack;

    void enter(const std::string &name) {
        if (!path) return;
        int index = calls.size();
        calls.push_back({name, stack.empty() ? -1 : stack.back(), 0, -1});
        stack.push_back(index);
        calls[index].begin_ns = PerformanceTimer::query();
    }

    void leave() {
        if (!path) return;
        int64_t end_ns = PerformanceTimer::query();
        Call &call = calls[stack.back()];
        call.runtime_ns = end_ns - call.begin_ns;
        stack.pop_back();
    }

    static void string(FILE *file, const std::string &value) {
        fputc('"', file);
        for (unsigned char ch : value) {
            if (ch == '"' || ch == '\\') fprintf(file, "\\%c", ch);
            else if (ch < 32) fprintf(file, "\\u%04x", ch);
            else fputc(ch, file);
        }
        fputc('"', file);
    }

    ~YosysBenchCallTrace() {
        if (!path) return;
        FILE *file = fopen(path, "w");
        if (!file) return; // The runner rejects a missing or incomplete trace.
        fputs("{\"calls\":[", file);
        for (size_t index = 0; index < calls.size(); ++index) {
            const Call &call = calls[index];
            if (index) fputc(',', file);
            fputs("{\"name\":", file);
            string(file, call.name);
            fputs(",\"parent\":", file);
            if (call.parent < 0) fputs("null", file);
            else fprintf(file, "%d", call.parent);
            fprintf(file, ",\"runtime_ns\":%lld}", (long long)call.runtime_ns);
        }
        fputs("]}\n", file);
        fclose(file);
    }
} yosys_bench_call_trace;
}
