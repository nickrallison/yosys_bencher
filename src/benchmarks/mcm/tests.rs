use super::*;
use crate::{
    experiment::{ArtifactPolicy, BenchmarkOut},
    metrics::MetricValue,
};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

fn tool(path: &Path, script: &str) {
    fs::write(path, script).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

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
        suite.repetitions = NonZeroU32::new(2).unwrap();
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
        let workspace = tempfile::tempdir_in(self.root.path()).unwrap();
        let mut context = RunContext::new(workspace.path().into(), policy);
        let input = self.suite.inputs()?.remove(0);
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
fn comparison_preserves_samples_and_reuses_references_only_within_session() {
    let fixture = Fixture::new();
    let session = fixture.suite.begin().unwrap();
    let (first, workspace) = fixture.run(&session, no_artifacts()).unwrap();
    assert_eq!(first.paper.samples.len(), 2);
    assert_eq!(first.baseline.len(), 2);
    assert_eq!(first.optimized.len(), 2);
    assert_eq!(*first.baseline[0].measurements().area(), Some(40.0));
    assert_eq!(
        *first.optimized[0].measurements().synthesis.area(),
        Some(20.0)
    );
    assert_eq!(
        first.get_metric("area_savings"),
        Some(MetricValue::Float(50.0))
    );
    assert!(
        first
            .optimized
            .iter()
            .all(|s| *s.runtime() > Duration::ZERO)
    );
    for sample in &first.optimized {
        assert_eq!(sample.measurements().mcm.adds_subs, 1);
        assert_eq!(sample.measurements().mcm.multipliers, 0);
        assert_eq!(sample.measurements().mcm.depth, Some(2));
        assert_eq!(
            sample.measurements().mcm.cpu_time,
            Duration::from_micros(2500)
        );
        assert_eq!(
            sample.measurements().synthesis.cell_count().get("NAND2_X1"),
            Some(&5)
        );
    }
    assert_eq!(
        fixture.events(),
        [
            "optimized",
            "optimized",
            "paper",
            "paper",
            "baseline",
            "baseline"
        ]
    );
    assert_eq!(fs::read_dir(workspace.path()).unwrap().count(), 0);
    let (second, _) = fixture.run(&session, no_artifacts()).unwrap();
    assert_eq!(first.paper.id, second.paper.id);
    assert_eq!(first.paper.measured_at, second.paper.measured_at);
    assert_eq!(
        fixture
            .events()
            .iter()
            .filter(|phase| *phase == "paper")
            .count(),
        2
    );
    let fresh_session = fixture.suite.begin().unwrap();
    let (fresh, _) = fixture.run(&fresh_session, no_artifacts()).unwrap();
    assert_ne!(first.paper.id, fresh.paper.id);
    assert_eq!(
        fixture
            .events()
            .iter()
            .filter(|phase| *phase == "paper")
            .count(),
        4
    );
    let recorded: McmOutput =
        serde_json::from_str(&serde_json::to_string(&first).unwrap()).unwrap();
    assert_eq!(recorded.paper.id, first.paper.id);
}

#[test]
fn metric_hierarchy_preserves_individual_samples_and_optional_values() {
    use crate::metrics::MetricDescriptor;

    let fixture = Fixture::new();
    let session = fixture.suite.begin().unwrap();
    let (mut output, _) = fixture.run(&session, no_artifacts()).unwrap();
    let mut second = output.optimized[1].measurements().clone();
    second.mcm.cpu_time = Duration::from_millis(9);
    second.mcm.depth = None;
    output.optimized[1] = OptimizedOutput::new(Duration::from_millis(12), second);

    let metrics = output.metrics();
    assert!(MetricDescriptor::group("output", McmOutput::list_metrics()).matches_value(&metrics));
    let MetricValue::Group(root) = metrics else {
        panic!("expected root group")
    };
    let MetricValue::Samples(optimized) = &root["optimized"] else {
        panic!("expected optimized samples")
    };
    assert_eq!(optimized.len(), 2);
    for (index, sample) in optimized.iter().enumerate() {
        let MetricValue::Group(sample) = sample else {
            panic!("expected sample group")
        };
        let MetricValue::Group(measurements) = &sample["measurements"] else {
            panic!("expected measurements group")
        };
        let MetricValue::Group(mcm) = &measurements["mcm"] else {
            panic!("expected MCM group")
        };
        let MetricValue::Group(synthesis) = &measurements["synthesis"] else {
            panic!("expected synthesis group")
        };
        assert_eq!(synthesis["area"], MetricValue::Float(20.0));
        assert!(!measurements.contains_key("area"));
        if index == 0 {
            assert_eq!(
                mcm["cpu_time"],
                MetricValue::Duration(Duration::from_micros(2500))
            );
            assert_eq!(mcm["depth"], MetricValue::UInt(2));
        } else {
            assert_eq!(
                mcm["cpu_time"],
                MetricValue::Duration(Duration::from_millis(9))
            );
            assert!(!mcm.contains_key("depth"));
            assert_eq!(
                sample["runtime"],
                MetricValue::Duration(Duration::from_millis(12))
            );
        }
    }
    let MetricValue::Group(paper) = &root["paper"] else {
        panic!("expected paper group")
    };
    let MetricValue::Samples(samples) = &paper["samples"] else {
        panic!("expected paper samples")
    };
    assert_eq!(samples.len(), 2);
    let MetricValue::Samples(baseline) = &root["baseline"] else {
        panic!("expected baseline samples")
    };
    assert_eq!(baseline.len(), 2);
    assert_eq!(root["area_savings"], MetricValue::Float(50.0));
    assert_eq!(output.get_metric("mcm.cpu_time"), None);
}

#[test]
fn reference_key_includes_actual_coefficients_widths_seed_and_repetitions() {
    let mut fixture = Fixture::new();
    let session = fixture.suite.begin().unwrap();
    let (first, _) = fixture.run(&session, no_artifacts()).unwrap();
    fixture.suite.coefficient_sets = CoefficientSets::Explicit(vec![vec![-11, 3]]);
    let (reordered, _) = fixture.run(&session, no_artifacts()).unwrap();
    assert_ne!(first.paper.id, reordered.paper.id);
    fixture.suite.options.paper_seed += 1;
    let (seed, _) = fixture.run(&session, no_artifacts()).unwrap();
    assert_ne!(seed.paper.id, reordered.paper.id);
    fixture.suite.input_width = 16;
    let (width, _) = fixture.run(&session, no_artifacts()).unwrap();
    assert_ne!(width.paper.id, seed.paper.id);
    fixture.suite.repetitions = NonZeroU32::new(1).unwrap();
    let (repetitions, _) = fixture.run(&session, no_artifacts()).unwrap();
    assert_eq!(repetitions.paper.samples.len(), 1);
    assert_ne!(repetitions.paper.id, width.paper.id);
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
fn retention_and_diagnostics_do_not_add_measurement_samples() {
    let fixture = Fixture::new();
    let session = fixture.suite.begin().unwrap();
    let (_, logs) = fixture
        .run(&session, ArtifactPolicy::new(true, false, false))
        .unwrap();
    assert_eq!(fs::read_dir(logs.path()).unwrap().count(), 19); // Six commands, three process logs, plus stage logs.
    for phase in ["baseline-0", "baseline-1", "optimized-0", "optimized-1"] {
        let directory = logs.path().join("logs").join(phase);
        for stage in ["synth", "abc", "stat"] {
            assert!(directory.join(format!("{stage}.log")).is_file());
        }
        if phase.starts_with("optimized") {
            let mcm = fs::read_to_string(directory.join("before-synth.log")).unwrap();
            assert!(mcm.contains("depth 2"));
            assert!(mcm.contains("1 $add"));
            assert!(!mcm.contains("NAND2_X1"));
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
        assert!(
            !fs::read_to_string(directory.join("run.ys"))
                .unwrap()
                .contains("write_verilog")
        );
        assert!(fs::read_dir(directory).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("inspect-")
        }));
    }
    let (output, netlists) = fixture
        .run(&session, ArtifactPolicy::new(false, false, true))
        .unwrap();
    assert_eq!(output.optimized.len(), 2);
    assert_eq!(output.baseline.len(), 2);
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
fn failed_and_unsupported_processes_preserve_diagnostics_and_clean_scratch() {
    let fixture = Fixture::new();
    let session = fixture.suite.begin().unwrap();
    let input = fixture.suite.inputs().unwrap().remove(0);
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
            .join("process-000-optimized-0.stderr.log")
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
    let fixture = Fixture::new();
    let mut input = fixture.suite.inputs().unwrap().remove(0);
    input.coefficients = vec![3, -11, i32::MAX as i64, -i32::MAX as i64];
    design::validate(&input).unwrap();
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
        design::validate(&input),
        Err(ExperimentError::Unsupported(_))
    ));
    input.coefficients = vec![i64::MAX];
    assert!(design::validate(&input).is_err());
    input.coefficients.clear();
    assert!(design::validate(&input).is_err());
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
    let mut input = suite.inputs().unwrap().remove(0);
    let workspace = tempfile::tempdir().unwrap();
    let mut context = RunContext::new(
        workspace.path().into(),
        ArtifactPolicy::new(true, true, true),
    );
    let result = McmProcedure.run(&input, &tools, &mut context).unwrap();
    assert!(
        result.optimized[0]
            .measurements()
            .synthesis
            .area()
            .is_some()
    );
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
