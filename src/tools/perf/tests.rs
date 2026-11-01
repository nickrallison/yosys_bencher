use std::os::unix::fs::PermissionsExt;

use super::*;

fn tool(path: &Path, script: &str) {
    fs::write(path, script).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn fixture() -> (tempfile::TempDir, PerfProfiler) {
    let root = tempfile::Builder::new()
        .prefix("perf with spaces ")
        .tempdir()
        .unwrap();
    let path = |name| root.path().join(name);
    let profiler = PerfProfiler {
        perf: path("perf"),
        perl: path("perl"),
        addr2line: path("addr2line"),
        stackcollapse: path("collapse.pl"),
        flamegraph: path("flamegraph.pl"),
    };
    tool(
        &profiler.perf,
        r#"#!/bin/sh
case "$1" in
record)
    shift
    while [ "$1" != -- ]; do
        if [ "$1" = -o ]; then shift; data=$1; fi
        shift
    done
    shift
    printf 'raw profile\n' > "$data"
    "$@" ;;
script)
    [ "${DEBUGINFOD_URLS-unset}" = '' ] || exit 9
    printf 'yosys;main;design.cc:12 1\n' ;;
esac
"#,
    );
    tool(&profiler.perl, "#!/bin/sh\nexec \"$@\"\n");
    tool(
        &profiler.stackcollapse,
        "#!/bin/sh\n[ \"$1\" = --srcline ] || exit 8\ncat\n",
    );
    tool(
        &profiler.flamegraph,
        "#!/bin/sh\nprintf '<svg>'\ncat\nprintf '</svg>'\n",
    );
    (root, profiler)
}

#[test]
fn addr2line_resolution_preserves_the_symlink_name_that_selects_its_mode() {
    let root = tempfile::tempdir().unwrap();
    let binary = root.path().join("llvm-symbolizer");
    let link = root.path().join("llvm-addr2line-16");
    tool(&binary, "#!/bin/sh\n[ \"${0##*/}\" = llvm-addr2line-16 ]\n");
    std::os::unix::fs::symlink(binary, &link).unwrap();
    assert!(
        Command::new(resolve(&link).unwrap())
            .status()
            .unwrap()
            .success()
    );
}

#[test]
fn yosys_diagnostics_retain_flamegraphs_when_other_artifacts_are_disabled() {
    use crate::{
        experiment::{ArtifactPolicy, RunContext},
        tools::yosys::{
            Yosys, YosysJob, YosysRunner,
            design::{Design, SourceFile},
        },
    };
    let (root, profiler) = fixture();
    let yosys = root.path().join("yosys");
    tool(
        &yosys,
        "#!/bin/sh\n[ \"$*\" = '-Q -T -s run.ys' ] || exit 7\n[ -f run.ys ] && [ -f design.v ] && [ -d .yosys-logs ]\n",
    );
    let workspace = root.path().join("artifacts");
    let mut context = RunContext::new(workspace.clone(), ArtifactPolicy::new(false, false, false));
    let runner = YosysRunner::new(&Yosys::new(yosys)).unwrap();
    let job = YosysJob::new(Design {
        files: vec![SourceFile::new("design.v", "module top; endmodule")],
        top: Some("top".into()),
        ..Default::default()
    })
    .unwrap();
    let job = runner.prepare(&job, &mut context).unwrap();
    runner.diagnose(&job, "disabled", &mut context);
    assert!(!workspace.join("perf").exists());
    let mut context = context.with_perf(profiler);
    runner.diagnose(&job, "yosys", &mut context);
    assert!(
        context.diagnostics().is_empty(),
        "{:?}",
        context.diagnostics()
    );
    let directory = workspace.join("perf/yosys");
    assert!(directory.join("perf.data").is_file());
    assert!(directory.join("run.ys").is_file());
    assert!(
        fs::read_to_string(directory.join("flamegraph.svg"))
            .unwrap()
            .contains("design.cc:12")
    );
    assert!(!workspace.join("scripts").exists());
    assert!(!workspace.join("logs").exists());
}

#[test]
fn pipeline_failures_keep_raw_data_and_report_the_failing_stage() {
    for stage in ["record", "script", "collapse", "flamegraph"] {
        let (root, profiler) = fixture();
        let path = match stage {
            "record" | "script" => &profiler.perf,
            "collapse" => &profiler.stackcollapse,
            _ => &profiler.flamegraph,
        };
        let source = fs::read_to_string(path).unwrap();
        let failure = if stage == "record" || stage == "script" {
            format!(
                "#!/bin/sh\nif [ \"$1\" = {stage} ]; then echo deliberate-failure >&2; exit 5; fi\n{}",
                source.strip_prefix("#!/bin/sh\n").unwrap()
            )
        } else {
            "#!/bin/sh\necho deliberate-failure >&2\nexit 5\n".into()
        };
        tool(path, &failure);
        let directory = root.path().join("profile");
        let error = profiler
            .record(&Command::new("true"), &directory)
            .unwrap_err();
        assert!(
            matches!(error, ExperimentError::Profiling(detail) if detail.contains(stage) && detail.contains("deliberate-failure"))
        );
        assert!(directory.join(format!("{stage}.stderr.log")).is_file());
        if stage != "record" {
            assert!(directory.join("perf.data").is_file());
        }
        assert!(!directory.join("flamegraph.svg").exists() || stage == "flamegraph");
    }
}

#[test]
fn empty_stacks_keep_the_recording_and_do_not_produce_an_empty_svg() {
    let (root, profiler) = fixture();
    tool(&profiler.stackcollapse, "#!/bin/sh\nexit 0\n");
    let directory = root.path().join("profile");
    assert!(matches!(profiler.record(&Command::new("true"), &directory),
        Err(ExperimentError::Profiling(detail)) if detail.contains("no sampled stacks")));
    assert!(directory.join("perf.data").is_file());
    assert!(!directory.join("flamegraph.svg").exists());
}

#[test]
#[ignore = "requires perf permissions, LLVM addr2line, FlameGraph tools, and a C compiler"]
fn native_flamegraph_contains_source_lines() {
    let profiler = PerfProfiler::new("/usr/bin/llvm-addr2line-16").unwrap();
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("spin.c");
    fs::write(&source, "volatile unsigned long value; int main(void) { for (unsigned long i=0;i<200000000;i++) value += i; return 0; }\n").unwrap();
    let binary = root.path().join("spin");
    assert!(
        Command::new("cc")
            .args(["-O2", "-g"])
            .arg(source)
            .arg("-o")
            .arg(&binary)
            .status()
            .unwrap()
            .success()
    );
    let directory = root.path().join("profile");
    profiler.record(&Command::new(binary), &directory).unwrap();
    let svg = fs::read_to_string(directory.join("flamegraph.svg")).unwrap();
    assert!(svg.contains("<svg"));
    assert!(svg.contains("spin.c:1"));
}
