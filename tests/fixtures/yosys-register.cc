#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <string>
#include <vector>

#define YOSYS_NAMESPACE_BEGIN namespace Yosys {
namespace Yosys {
struct PerformanceTimer {
    static int64_t query() { static int64_t tick = 0; return ++tick * 1000; }
};
struct Pass {
    std::string pass_name;
    struct pre_post_exec_state_t { int64_t begin_ns; };
    pre_post_exec_state_t pre_execute();
    void post_execute(pre_post_exec_state_t state);
};
}

YOSYS_NAMESPACE_BEGIN
Pass::pre_post_exec_state_t Pass::pre_execute()
{
    pre_post_exec_state_t state;
    state.begin_ns = PerformanceTimer::query();
    return state;
}
void Pass::post_execute(pre_post_exec_state_t state)
{
    int64_t time_ns = PerformanceTimer::query() - state.begin_ns;
    (void)time_ns;
}
}

int main() {
    Yosys::Pass synth{"synth"}, opt{"opt"}, plugin{"plugin.\"pass%"};
    auto parent = synth.pre_execute();
    auto child = opt.pre_execute();
    opt.post_execute(child);
    child = opt.pre_execute();
    opt.post_execute(child);
    synth.post_execute(parent);
    child = plugin.pre_execute();
    plugin.post_execute(child);
}
