use super::*;
use crate::{
    experiment::{
        ArtifactPolicy, BenchmarkOut, OsProcessExecutor, ProcessCapture, ProcessExecutor,
    },
    metrics::MetricValue,
    test_support::{file, required, tool},
    tools::perf::PreparedPerfStat,
};
use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

#[derive(Debug, Clone, Default)]
struct Processes(Rc<RefCell<Vec<PathBuf>>>);

impl ProcessExecutor for Processes {
    fn execute(
        &self,
        command: &mut Command,
        workspace: &Path,
        perf: Option<&PreparedPerfStat>,
    ) -> Result<ProcessCapture> {
        self.0.borrow_mut().push(command.get_program().into());
        OsProcessExecutor.execute(command, workspace, perf)
    }
}

struct McmTest {
    root: tempfile::TempDir,
    suite: McmSuite,
    yosys: Yosys,
    processes: Processes,
}

impl McmTest {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let yosys = tool("yosys", Some("MCM_YOSYS"));
        let paper = tool("acm", Some("MCM_PAPER"));
        let mut suite = McmSuite::new(
            CoefficientSets::Explicit(vec![vec![3, -11]]),
            required(CellLibrary::from_path(file("MCM_LIBERTY"))),
            PaperTool { executable: paper },
        );
        suite.options.search_budget = 1000;
        Self {
            root,
            suite,
            yosys: Yosys::new(yosys),
            processes: Processes::default(),
        }
    }

    fn run(
        &self,
        session: &Rc<PaperSession>,
        policy: ArtifactPolicy,
    ) -> Result<(McmOutput, tempfile::TempDir)> {
        self.run_sample(session, policy, 0)
    }

    fn run_sample(
        &self,
        session: &Rc<PaperSession>,
        policy: ArtifactPolicy,
        sample_index: u32,
    ) -> Result<(McmOutput, tempfile::TempDir)> {
        let workspace = tempfile::tempdir_in(self.root.path()).unwrap();
        let mut context = RunContext::new(workspace.path().into(), policy)
            .with_sample_index(sample_index)
            .with_executor(self.processes.clone());
        let input = self.suite.inputs(session)?.remove(0);
        let output = McmProcedure.run(
            &input,
            &self.suite.tools(&self.yosys, session)?,
            &mut context,
        )?;
        Ok((output, workspace))
    }
}

fn no_artifacts() -> ArtifactPolicy {
    ArtifactPolicy::new(false, false, false)
}

#[test]
fn comparison_preserves_observations_and_reuses_references_only_within_session() {
    let test = McmTest::new();
    let session = test.suite.begin().unwrap();
    let (first, workspace) = test.run(&session, no_artifacts()).unwrap();
    assert!(
        first
            .baseline
            .measurements()
            .area()
            .is_some_and(|area| area > 0.0)
    );
    assert!(
        first
            .optimized
            .measurements()
            .synthesis
            .area()
            .is_some_and(|area| area > 0.0)
    );
    assert!(first.area_savings().is_some());
    assert!(*first.optimized.runtime() > Duration::ZERO);
    let measurements = first.optimized.measurements();
    assert!(measurements.optimization.cpu_time > Duration::ZERO);
    assert!(measurements.optimization.adds_subs > 0);
    assert_eq!(measurements.optimization.multipliers, 0);
    assert!(
        measurements
            .optimization
            .depth
            .is_some_and(|depth| depth > 0)
    );
    assert!(!measurements.synthesis.cell_count().is_empty());
    assert_eq!(
        *test.processes.0.borrow(),
        [
            test.yosys.path().clone(),
            test.suite.paper.executable.clone(),
            test.yosys.path().clone()
        ]
    );
    assert_eq!(fs::read_dir(workspace.path()).unwrap().count(), 0);
    let (second, _) = test.run(&session, no_artifacts()).unwrap();
    assert_eq!(first.reference.id, second.reference.id);
    assert_eq!(first.reference.measured_at, second.reference.measured_at);
    assert_eq!(
        test.processes
            .0
            .borrow()
            .iter()
            .filter(|program| **program == test.suite.paper.executable)
            .count(),
        1
    );
    let fresh_session = test.suite.begin().unwrap();
    let (fresh, _) = test.run(&fresh_session, no_artifacts()).unwrap();
    assert_eq!(fresh.reference.id, 0);
    assert_eq!(
        test.processes
            .0
            .borrow()
            .iter()
            .filter(|program| **program == test.suite.paper.executable)
            .count(),
        2
    );
    let recorded: McmOutput =
        serde_json::from_str(&serde_json::to_string(&first).unwrap()).unwrap();
    assert_eq!(recorded.reference.id, first.reference.id);
}

#[test]
fn metric_hierarchy_keeps_namespaces_and_optional_values() {
    use crate::metrics::MetricDescriptor;
    use crate::{
        benchmarks::synth::{SynthMeasurements, SynthOutput},
        experiment::Reference,
        tools::yosys::Statistics,
    };
    let synthesis = |area| {
        SynthMeasurements::from_statistics(Statistics {
            area: Some(area),
            cell_count: [("NAND2_X1".into(), 5)].into(),
        })
    };
    let output = McmOutput {
        reference: Reference {
            id: 0,
            executable: PaperToolId::from_bytes(b"paper"),
            measured_at: std::time::UNIX_EPOCH,
            os: "linux".into(),
            arch: "x86_64".into(),
            measurements: PaperStats {
                runtime: Duration::ZERO,
                adds_subs: 1,
                shifts: 2,
                negations: 0,
                multipliers: 0,
                depth: 1,
            },
            perf: None,
        },
        baseline: SynthOutput::new(Duration::ZERO, synthesis(40.0)),
        optimized: OptimizedOutput::new(
            Duration::from_millis(12),
            OptimizedMeasurements {
                optimization: McmMeasurements {
                    cpu_time: Duration::from_micros(2500),
                    adds_subs: 1,
                    negations: 0,
                    multipliers: 0,
                    depth: None,
                },
                synthesis: synthesis(20.0),
            },
        ),
    };
    let metrics = output.metrics();
    assert!(MetricDescriptor::group("output", McmOutput::list_metrics()).matches_value(&metrics));
    let MetricValue::Group(root) = metrics else {
        panic!("expected root group")
    };
    let MetricValue::Group(optimized) = &root["optimized"] else {
        panic!("expected optimized group")
    };
    let MetricValue::Group(measurements) = &optimized["measurements"] else {
        panic!("expected measurements group")
    };
    let MetricValue::Group(mcm) = &measurements["optimization"] else {
        panic!("expected MCM group")
    };
    let MetricValue::Group(synthesis) = &measurements["synthesis"] else {
        panic!("expected synthesis group")
    };
    assert!(!mcm.contains_key("depth"));
    assert_eq!(
        mcm["cpu_time"],
        MetricValue::Duration(Duration::from_micros(2500))
    );
    assert_eq!(synthesis["area"], MetricValue::Float(20.0));
    assert_eq!(
        optimized["runtime"],
        MetricValue::Duration(Duration::from_millis(12))
    );
    assert_eq!(root["area_savings"], MetricValue::Float(50.0));
    assert_eq!(output.get_metric("mcm.cpu_time"), None);
}

#[test]
fn reference_key_includes_coefficients_width_seed_and_sample_index() {
    let mut test = McmTest::new();
    let paper = test.root.path().join("paper");
    fs::copy(&test.suite.paper.executable, &paper).unwrap();
    test.suite.paper.executable = paper;
    let session = test.suite.begin().unwrap();
    let (first, _) = test.run(&session, no_artifacts()).unwrap();
    test.suite.coefficient_sets = CoefficientSets::Explicit(vec![vec![-11, 3]]);
    let (reordered, _) = test.run(&session, no_artifacts()).unwrap();
    assert_ne!(first.reference.id, reordered.reference.id);
    test.suite.options.paper_seed += 1;
    let (seed, _) = test.run(&session, no_artifacts()).unwrap();
    assert_ne!(seed.reference.id, reordered.reference.id);
    test.suite.input_width = 16;
    let (width, _) = test.run(&session, no_artifacts()).unwrap();
    assert_ne!(width.reference.id, seed.reference.id);
    let input = test.suite.inputs(&session).unwrap().remove(0);
    let (sample, _) = test.run_sample(&session, no_artifacts(), 1).unwrap();
    assert_ne!(sample.reference.id, width.reference.id);
    assert_eq!(test.suite.inputs(&session).unwrap()[0], input);
    let (cached, _) = test.run_sample(&session, no_artifacts(), 1).unwrap();
    assert_eq!(cached.reference.id, sample.reference.id);
    use std::io::Write;
    fs::OpenOptions::new()
        .append(true)
        .open(&test.suite.paper.executable)
        .unwrap()
        .write_all(b"changed binary")
        .unwrap();
    assert!(matches!(
        test.run(&session, no_artifacts()),
        Err(ExperimentError::Config(_))
    ));
}

#[test]
fn retention_and_diagnostics_do_not_add_measurements() {
    let test = McmTest::new();
    let session = test.suite.begin().unwrap();
    let (_, logs) = test
        .run(&session, ArtifactPolicy::new(true, false, false))
        .unwrap();
    assert_eq!(fs::read_dir(logs.path()).unwrap().count(), 10);
    for phase in ["baseline", "optimized"] {
        let directory = logs.path().join("logs").join(phase);
        for stage in ["synth", "dfflibmap", "abc", "stat"] {
            assert!(directory.join(format!("{stage}.log")).is_file());
        }
        if phase == "optimized" {
            assert!(
                fs::read_to_string(directory.join("mcm.log"))
                    .unwrap()
                    .contains("mcm:")
            );
            let stats = fs::read_to_string(directory.join("mcm-stat.log")).unwrap();
            assert!(stats.contains("$add"));
            assert!(!stats.contains("NAND2_X1"));
            assert!(directory.join("timing.json").is_file());
        }
    }
    let (_, scripts) = test
        .run(&session, ArtifactPolicy::new(false, true, true))
        .unwrap();
    assert_eq!(
        fs::read_dir(scripts.path().join("scripts"))
            .unwrap()
            .count(),
        2
    );
    for entry in fs::read_dir(scripts.path().join("scripts")).unwrap() {
        let directory = entry.unwrap().path();
        let script = fs::read_to_string(directory.join("run.ys")).unwrap();
        assert!(!script.contains("write_verilog"));
        assert!(script.contains("dfflibmap"));
        assert!(fs::read_dir(directory).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("inspect-")
        }));
    }
    let (_, netlists) = test
        .run(&session, ArtifactPolicy::new(false, false, true))
        .unwrap();
    for phase in ["baseline", "optimized"] {
        assert!(
            netlists
                .path()
                .join("netlists")
                .join(format!("{phase}.v"))
                .is_file()
        );
    }
    let processes = test.processes.0.borrow();
    assert_eq!(processes.len(), 11);
    assert!(
        processes[processes.len() - 2..]
            .iter()
            .all(|program| program == test.yosys.path())
    );
    assert_eq!(fs::read_dir(netlists.path()).unwrap().count(), 1);
}

#[test]
fn diagnostic_failures_preserve_successful_measurements() {
    let test = McmTest::new();
    let session = test.suite.begin().unwrap();
    let input = test.suite.inputs(&session).unwrap().remove(0);
    let tools = test.suite.tools(&test.yosys, &session).unwrap();
    let workspace = tempfile::tempdir().unwrap();
    fs::write(workspace.path().join("netlists"), "blocked").unwrap();
    let mut context = RunContext::new(
        workspace.path().into(),
        ArtifactPolicy::new(true, false, true),
    );
    let output = McmProcedure.run(&input, &tools, &mut context).unwrap();
    assert!(output.optimized.measurements().synthesis.area().is_some());
    assert_eq!(context.diagnostics().len(), 2);
    assert!(
        context
            .diagnostics()
            .iter()
            .all(|error| error.contains("netlist"))
    );
}

#[test]
fn failed_and_unsupported_processes_preserve_diagnostics_and_clean_scratch() {
    let test = McmTest::new();
    let session = test.suite.begin().unwrap();
    let mut input = test.suite.inputs(&session).unwrap().remove(0);
    let tools = test.suite.tools(&test.yosys, &session).unwrap();
    for (command, unsupported) in [
        ("select -assert-count 0 *", false),
        ("missing_custom_pass", true),
    ] {
        input.synthesis = crate::benchmarks::synth::SynthConfig::new(
            test.suite.library.clone(),
            vec![command.into()],
        );
        let workspace = tempfile::tempdir().unwrap();
        let mut context = RunContext::new(
            workspace.path().into(),
            ArtifactPolicy::new(true, false, false),
        );
        let error = McmProcedure.run(&input, &tools, &mut context).unwrap_err();
        if unsupported {
            assert!(
                matches!(error, ExperimentError::Unsupported(reason) if reason.contains(command))
            );
        } else {
            assert!(
                matches!(error, ExperimentError::Process { detail, .. } if detail.contains("Assertion failed"))
            );
        }
        assert!(
            workspace
                .path()
                .join("process-000-optimized.stderr.log")
                .is_file()
        );
        assert!(workspace.path().join("logs/optimized/mcm.log").is_file());
        assert!(!fs::read_dir(workspace.path()).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".job-")
        }));
    }
}

#[test]
fn signed_generation_and_paper_validation_match_v1() {
    let mut input = McmInput {
        coefficients: vec![3, -11, i32::MAX as i64, -i32::MAX as i64],
        input_width: 32,
        options: McmOptions::default(),
        synthesis: crate::benchmarks::synth::SynthConfig::new(
            CellLibrary::new("library(test) {}"),
            Vec::new(),
        ),
        paper_identity: PaperToolId::from_bytes(b"paper"),
    };
    input.validate().unwrap();
    let source = design::verilog(&input).unwrap();
    for expression in [
        "x * (3'sd3)",
        "x * (-5'sd11)",
        "x * (32'sd2147483647)",
        "x * (-32'sd2147483647)",
        "[63:0] y3",
    ] {
        assert!(source.contains(expression), "{source}");
    }
    input.coefficients = vec![0];
    assert!(matches!(
        input.validate(),
        Err(ExperimentError::Unsupported(_))
    ));
    input.coefficients = vec![i64::MAX];
    assert!(input.validate().is_err());
    input.coefficients.clear();
    assert!(input.validate().is_err());
}

#[test]
fn paper_parser_requires_complete_statistics() {
    let parsed = parse::paper(
        "// Cost: 4 adds/subtracts 5 shifts 2 negations\n// Depth: 3\nt1 = cmul(t0, 3);\n",
        Duration::ZERO,
    )
    .unwrap();
    assert_eq!(
        (
            parsed.adds_subs,
            parsed.shifts,
            parsed.negations,
            parsed.multipliers,
            parsed.depth
        ),
        (4, 5, 2, 1, 3)
    );
    assert!(
        parse::paper(
            "// Cost: 1 adds/subtracts 0 shifts 0 negations\n",
            Duration::ZERO
        )
        .is_err()
    );
}

#[test]
fn real_comparison_and_signed_arithmetic() {
    let test = McmTest::new();
    let session = test.suite.begin().unwrap();
    let tools = test.suite.tools(&test.yosys, &session).unwrap();
    let mut input = test.suite.inputs(&session).unwrap().remove(0);
    let workspace = tempfile::tempdir().unwrap();
    let mut context = RunContext::new(
        workspace.path().into(),
        ArtifactPolicy::new(true, true, true),
    );
    let result = McmProcedure.run(&input, &tools, &mut context).unwrap();
    assert!(result.optimized.measurements().synthesis.area().is_some());
    for phase in ["baseline", "optimized"] {
        assert!(
            workspace
                .path()
                .join("netlists")
                .join(format!("{phase}.v"))
                .is_file()
        );
    }
    input.coefficients = vec![3, -11, i32::MAX as i64, -i32::MAX as i64];
    fs::write(
        workspace.path().join("design.v"),
        design::verilog(&input).unwrap(),
    )
    .unwrap();
    for x in [i32::MIN, -1, 0, 1, i32::MAX] {
        let mut script =
            format!("read_verilog design.v; prep -top unoptimized; sat -verify -set x {x}");
        for (index, coefficient) in input.coefficients.iter().enumerate() {
            let width = input.input_width + (64 - coefficient.unsigned_abs().leading_zeros()) + 1;
            let bits = (i128::from(x) * i128::from(*coefficient)) & ((1i128 << width) - 1);
            script.push_str(&format!(" -prove y{index} {width}'h{bits:x}"));
        }
        let output = Command::new(test.yosys.path())
            .current_dir(workspace.path())
            .args(["-Q", "-T", "-p", &script])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
