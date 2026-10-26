use super::*;
use crate::{
    experiment::{ArtifactPolicy, BenchmarkOut},
    metrics::MetricValue,
};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::Duration,
};

use crate::test_support::executable as tool;

const YOSYS: &str = include_str!("../../../tests/fixtures/yosys.sh");
const PAPER: &str = include_str!("../../../tests/fixtures/paper.sh");
struct Fixture {
    root: tempfile::TempDir,
    suite: McmSuite,
    yosys: Yosys,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let yosys = root.path().join("fake yosys");
        let paper = root.path().join("fake paper");
        tool(&yosys, YOSYS);
        tool(&paper, PAPER);
        let mut suite = McmSuite::new(
            CoefficientSets::Explicit(vec![vec![3, -11]]),
            CellLibrary::new("library(test) {}"),
            PaperTool { executable: paper },
        );
        suite.options.search_budget = 1000;
        Self {
            root,
            suite,
            yosys: Yosys::new(yosys),
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
        let mut context =
            RunContext::new(workspace.path().into(), policy).with_sample_index(sample_index);
        let input = self.suite.inputs(session)?.remove(0);
        let output = McmProcedure.run(
            &input,
            &self.suite.tools(&self.yosys, session)?,
            &mut context,
        )?;
        Ok((output, workspace))
    }

    fn events(&self) -> Vec<String> {
        fs::read_to_string(self.root.path().join("events"))
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

fn no_artifacts() -> ArtifactPolicy {
    ArtifactPolicy::new(false, false, false)
}

#[test]
fn comparison_preserves_observations_and_reuses_references_only_within_session() {
    let fixture = Fixture::new();
    let session = fixture.suite.begin().unwrap();
    let (first, workspace) = fixture.run(&session, no_artifacts()).unwrap();
    assert_eq!(*first.baseline.measurements().area(), Some(40.0));
    assert_eq!(*first.optimized.measurements().synthesis.area(), Some(20.0));
    assert_eq!(
        first.get_metric("area_savings"),
        Some(MetricValue::Float(50.0))
    );
    assert!(*first.optimized.runtime() > Duration::ZERO);
    let measurements = first.optimized.measurements();
    assert_eq!(measurements.optimization.adds_subs, 1);
    assert_eq!(measurements.optimization.multipliers, 0);
    assert_eq!(measurements.optimization.depth, Some(2));
    assert_eq!(
        measurements.optimization.cpu_time,
        Duration::from_micros(2500)
    );
    assert_eq!(
        measurements.synthesis.cell_count().get("NAND2_X1"),
        Some(&5)
    );
    assert_eq!(fixture.events(), ["optimized", "paper", "baseline"]);
    assert_eq!(fs::read_dir(workspace.path()).unwrap().count(), 0);
    let (second, _) = fixture.run(&session, no_artifacts()).unwrap();
    assert_eq!(first.reference.id, second.reference.id);
    assert_eq!(first.reference.measured_at, second.reference.measured_at);
    assert_eq!(
        fixture
            .events()
            .iter()
            .filter(|phase| *phase == "paper")
            .count(),
        1
    );
    let fresh_session = fixture.suite.begin().unwrap();
    let (fresh, _) = fixture.run(&fresh_session, no_artifacts()).unwrap();
    assert_ne!(first.reference.id, fresh.reference.id);
    assert_eq!(
        fixture
            .events()
            .iter()
            .filter(|phase| *phase == "paper")
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
    let fixture = Fixture::new();
    let session = fixture.suite.begin().unwrap();
    let (mut output, _) = fixture.run(&session, no_artifacts()).unwrap();
    let mut measurements = output.optimized.measurements().clone();
    measurements.optimization.depth = None;
    output.optimized = OptimizedOutput::new(Duration::from_millis(12), measurements);
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
    let mut fixture = Fixture::new();
    let session = fixture.suite.begin().unwrap();
    let (first, _) = fixture.run(&session, no_artifacts()).unwrap();
    fixture.suite.coefficient_sets = CoefficientSets::Explicit(vec![vec![-11, 3]]);
    let (reordered, _) = fixture.run(&session, no_artifacts()).unwrap();
    assert_ne!(first.reference.id, reordered.reference.id);
    fixture.suite.options.paper_seed += 1;
    let (seed, _) = fixture.run(&session, no_artifacts()).unwrap();
    assert_ne!(seed.reference.id, reordered.reference.id);
    fixture.suite.input_width = 16;
    let (width, _) = fixture.run(&session, no_artifacts()).unwrap();
    assert_ne!(width.reference.id, seed.reference.id);
    let input = fixture.suite.inputs(&session).unwrap().remove(0);
    let (sample, _) = fixture.run_sample(&session, no_artifacts(), 1).unwrap();
    assert_ne!(sample.reference.id, width.reference.id);
    assert_eq!(fixture.suite.inputs(&session).unwrap()[0], input);
    let (cached, _) = fixture.run_sample(&session, no_artifacts(), 1).unwrap();
    assert_eq!(cached.reference.id, sample.reference.id);
    tool(
        &fixture.suite.paper.executable,
        &format!("{PAPER}\n# changed binary\n"),
    );
    assert!(matches!(
        fixture.run(&session, no_artifacts()),
        Err(ExperimentError::Config(_))
    ));
}

#[test]
fn retention_and_diagnostics_do_not_add_measurements() {
    let fixture = Fixture::new();
    let session = fixture.suite.begin().unwrap();
    let (_, logs) = fixture
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
                    .contains("depth 2")
            );
            let stats = fs::read_to_string(directory.join("mcm-stat.log")).unwrap();
            assert!(stats.contains("$add"));
            assert!(!stats.contains("NAND2_X1"));
            assert!(directory.join("timing.json").is_file());
        }
    }
    let (_, scripts) = fixture
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
    let (_, netlists) = fixture
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
    let events = fixture.events();
    assert!(
        events[events.len() - 2..]
            .iter()
            .all(|phase| phase.ends_with("-diagnostic"))
    );
    assert_eq!(fs::read_dir(netlists.path()).unwrap().count(), 1);
}

#[test]
fn diagnostic_failures_preserve_successful_measurements() {
    let fixture = Fixture::new();
    fs::write(
        format!("{}.fail-diagnostics", fixture.yosys.path().display()),
        "",
    )
    .unwrap();
    let session = fixture.suite.begin().unwrap();
    let input = fixture.suite.inputs(&session).unwrap().remove(0);
    let tools = fixture.suite.tools(&fixture.yosys, &session).unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let mut context = RunContext::new(
        workspace.path().into(),
        ArtifactPolicy::new(true, false, true),
    );
    let output = McmProcedure.run(&input, &tools, &mut context).unwrap();
    assert_eq!(
        *output.optimized.measurements().synthesis.area(),
        Some(20.0)
    );
    assert_eq!(context.diagnostics().len(), 2);
    assert!(
        context
            .diagnostics()
            .iter()
            .all(|error| error.contains("deliberate-diagnostic-error"))
    );
}

#[test]
fn failed_and_unsupported_processes_preserve_diagnostics_and_clean_scratch() {
    let fixture = Fixture::new();
    let session = fixture.suite.begin().unwrap();
    let input = fixture.suite.inputs(&session).unwrap().remove(0);
    let tools = fixture.suite.tools(&fixture.yosys, &session).unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let mut context = RunContext::new(
        workspace.path().into(),
        ArtifactPolicy::new(true, false, false),
    );
    tool(
        fixture.yosys.path(),
        "#!/bin/sh\necho deliberate-error >&2\nexit 9\n",
    );
    assert!(
        matches!(McmProcedure.run(&input, &tools, &mut context), Err(ExperimentError::Process { detail, .. }) if detail.contains("deliberate-error"))
    );
    assert_eq!(fs::read_dir(workspace.path()).unwrap().count(), 3);
    assert!(
        workspace
            .path()
            .join("process-000-optimized.stderr.log")
            .is_file()
    );
    tool(
        fixture.yosys.path(),
        "#!/bin/sh\necho 'ERROR: No such command: mcm' >&2\nexit 1\n",
    );
    assert!(matches!(
        McmProcedure.run(&input, &tools, &mut context),
        Err(ExperimentError::Unsupported(_))
    ));
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
#[ignore = "requires MCM_YOSYS, MCM_PAPER, and MCM_LIBERTY pointing to real tools"]
fn real_comparison_and_signed_arithmetic() {
    let yosys = PathBuf::from(std::env::var_os("MCM_YOSYS").expect("set MCM_YOSYS"));
    let paper = PathBuf::from(std::env::var_os("MCM_PAPER").expect("set MCM_PAPER"));
    let library = PathBuf::from(std::env::var_os("MCM_LIBERTY").expect("set MCM_LIBERTY"));
    let mut suite = McmSuite::new(
        CoefficientSets::Explicit(vec![vec![3, -11]]),
        CellLibrary::from_path(library).unwrap(),
        PaperTool { executable: paper },
    );
    suite.options.search_budget = 1000;
    let session = suite.begin().unwrap();
    let tools = suite.tools(&Yosys::new(&yosys), &session).unwrap();
    let mut input = suite.inputs(&session).unwrap().remove(0);
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
        let output = Command::new(&yosys)
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
