use super::*;
use std::fs;

pub(crate) const VERSION: u32 = 1;

/// Restore the checkout even when a build fails or unwinds.
struct RestoreSource {
    path: PathBuf,
    original: String,
    restored: bool,
}
impl Drop for RestoreSource {
    fn drop(&mut self) {
        if !self.restored
            && let Err(error) = fs::write(&self.path, &self.original)
        {
            log::error!("cannot restore {}: {error}", self.path.display());
        }
    }
}

pub(crate) fn build<T>(source: &Path, build: impl FnOnce() -> Result<T>) -> Result<T> {
    let path = source.join("kernel/register.cc");
    let original = fs::read_to_string(&path)?;
    let patched = instrument(&original)?;
    let mut restore = RestoreSource {
        path,
        original,
        restored: false,
    };
    fs::write(&restore.path, patched)?;
    let result = build();
    fs::write(&restore.path, &restore.original)?;
    restore.restored = true;
    result
}

fn instrument(source: &str) -> Result<String> {
    let mut source = source.to_owned();
    for (needle, replacement) in [
        ("YOSYS_NAMESPACE_BEGIN\n", format!("YOSYS_NAMESPACE_BEGIN\n{}\n", include_str!("call_trace.inc"))),
        ("state.begin_ns = PerformanceTimer::query();", "yosys_bencher_call_trace.enter(pass_name);\n\tstate.begin_ns = PerformanceTimer::query();".into()),
        ("int64_t time_ns = PerformanceTimer::query() - state.begin_ns;", "yosys_bencher_call_trace.leave();\n\tint64_t time_ns = PerformanceTimer::query() - state.begin_ns;".into()),
    ] {
        if source.matches(needle).count() != 1 {
            return Err(ExperimentError::Unsupported(format!(
                "cannot instrument Yosys pass calls: expected one {needle:?} in kernel/register.cc"
            )));
        }
        source = source.replacen(needle, &replacement, 1);
    }
    Ok(source)
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str = include_str!("../../../tests/fixtures/yosys-register.cc");

    #[test]
    fn native_trace_records_nested_and_repeated_calls_and_restores_source() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("kernel")).unwrap();
        let source = directory.path().join("kernel/register.cc");
        fs::write(&source, SOURCE).unwrap();
        let executable = directory.path().join("trace-test");
        build(directory.path(), || {
            let output = Command::new("c++")
                .arg("-std=c++17")
                .arg(&source)
                .arg("-o")
                .arg(&executable)
                .output()?;
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            Ok(())
        })
        .unwrap();
        assert_eq!(fs::read_to_string(&source).unwrap(), SOURCE);
        let trace = directory.path().join("calls.json");
        assert!(
            Command::new(&executable)
                .env("YOSYS_BENCH_CALL_TRACE", &trace)
                .status()
                .unwrap()
                .success()
        );
        let timings = YosysTimings::parse(br#"{"generator":"test","total_ns":0,"passes":{}}"#)
            .unwrap()
            .with_call_trace(&fs::read(&trace).unwrap())
            .unwrap();
        let calls = timings.calls.unwrap();
        assert_eq!(calls[0].name, "synth");
        assert_eq!(
            calls[0]
                .children
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            ["opt", "opt"]
        );
        assert!(calls[0].self_runtime > Duration::ZERO);
        assert_eq!(
            calls[0].runtime,
            calls[0].self_runtime
                + calls[0]
                    .children
                    .iter()
                    .map(|c| c.runtime)
                    .sum::<Duration>()
        );
        assert_eq!(calls[1].name, "plugin.\"pass%");
        // Diagnostic executions do not collect call traces.
        fs::remove_file(&trace).unwrap();
        assert!(
            Command::new(&executable)
                .env_remove("YOSYS_BENCH_CALL_TRACE")
                .status()
                .unwrap()
                .success()
        );
        assert!(!trace.exists());
    }

    #[test]
    fn failed_builds_restore_source_and_unknown_timer_layouts_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("kernel")).unwrap();
        let source = directory.path().join("kernel/register.cc");
        fs::write(&source, SOURCE).unwrap();
        let result: Result<()> = build(directory.path(), || {
            Err(ExperimentError::Config("build failed".into()))
        });
        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&source).unwrap(), SOURCE);
        assert!(instrument("unknown timer implementation").is_err());
        assert!(instrument(&SOURCE.repeat(2)).is_err());
    }
}
