use super::*;
use crate::experiment::{ArtifactPolicy, Procedure, RunContext};

#[test]
fn comparison_combines_multiple_passes_without_an_external_reference() {
    use crate::experiment::BenchmarkOut;
    use std::{fs, os::unix::fs::PermissionsExt};

    let root = tempfile::tempdir().unwrap();
    let executable = root.path().join("yosys");
    fs::write(
        &executable,
        include_str!("../../../tests/fixtures/yosys.sh"),
    )
    .unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    let workspace = root.path().join("workspace");
    let mut context = RunContext::new(workspace.clone(), ArtifactPolicy::new(false, false, false));
    let design = design::Design {
        files: vec![SourceFile::new("top.v", "module top; endmodule")],
        ..design::Design::default()
    };
    let result = comparison::compare(
        &Yosys::new(executable),
        &design,
        &SynthConfig::new(CellLibrary::new("library(test) {}"), Vec::new()),
        &mut context,
        |job| {
            job.stage_with_statistics("opt", vec!["opt".into()])?;
            job.stage_with_statistics("clean", vec!["clean".into()])
        },
        |run| {
            for stage in ["opt-stat", "clean-stat"] {
                assert_eq!(run.statistics(stage)?.cell_count.get("$add"), Some(&1));
            }
            assert_eq!(run.statistics("stat")?.cell_count.get("NAND2_X1"), Some(&5));
            SynthMeasurements::parse(run)
        },
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(result.area_savings(), Some(0.0));
    assert_eq!(
        *result.optimized.measurements().optimization.area(),
        Some(40.0)
    );
    assert!(
        crate::metrics::MetricDescriptor::group(
            "comparison",
            comparison::ComparisonOutput::<SynthMeasurements>::list_metrics()
        )
        .matches_value(&result.metrics())
    );
    assert_eq!(fs::read_dir(workspace).unwrap().count(), 0);
}

#[test]
#[ignore = "requires YOSYS and MCM_LIBERTY pointing to real tools"]
fn real_synthesis_counts_hierarchy_and_maps_flip_flops() {
    let yosys = Yosys::new(std::env::var_os("YOSYS").expect("set YOSYS"));
    let library =
        CellLibrary::from_path(std::env::var_os("MCM_LIBERTY").expect("set MCM_LIBERTY")).unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let run = |name: &str, source: &str| {
        let input = SynthInput::new(
            design::Design {
                files: vec![SourceFile::new("design.v", source)],
                top: Some("top".into()),
                ..design::Design::default()
            },
            SynthConfig::new(library.clone(), Vec::new()),
        );
        let mut context = RunContext::new(
            workspace.path().join(name),
            ArtifactPolicy::new(false, false, false),
        );
        YosysBenchmark::new(SynthProcedure)
            .run(&input, &yosys, &mut context)
            .unwrap()
            .measurements()
            .clone()
    };
    let flat = run(
        "flat",
        "module top(input [7:0] a,b, output [7:0] y); assign y=a+b; endmodule",
    );
    let hierarchy = run(
        "hierarchy",
        "module child #(parameter N=8)(input [N-1:0] a,b, output [N-1:0] y); assign y=a+b; endmodule\nmodule top(input [7:0] a,b,c,d, output [7:0] y,z); child #(.N(8)) u1(a,b,y); child #(.N(8)) u2(c,d,z); endmodule",
    );
    assert!((hierarchy.area.unwrap() - 2.0 * flat.area.unwrap()).abs() < 1e-6);
    assert_eq!(
        hierarchy.cell_count.values().sum::<u64>(),
        2 * flat.cell_count.values().sum::<u64>()
    );
    let sequential = run(
        "sequential",
        "module top(input clk, input [7:0] a,b, output reg [7:0] y); always @(posedge clk) y<=a+b; endmodule",
    );
    assert!(sequential.area.unwrap() > flat.area.unwrap());
    assert!(
        !sequential
            .cell_count
            .keys()
            .any(|name| name.starts_with('$'))
    );
    assert_eq!(
        sequential
            .cell_count
            .iter()
            .filter(|(name, _)| name.starts_with("DFF"))
            .map(|(_, count)| count)
            .sum::<u64>(),
        8
    );
}

#[test]
fn unmapped_internal_cells_do_not_produce_partial_area() {
    use crate::{experiment::ProcessOutput, tools::yosys::YosysRun};
    use std::{collections::BTreeMap, process::Command, time::Duration};
    let process = ProcessOutput {
        output: Command::new("true").output().unwrap(),
        elapsed: Duration::ZERO,
        perf: None,
    };
    let run = YosysRun {
        process,
        stage_logs: BTreeMap::from([("stat".into(), r#"{"modules":{"top":{"num_cells":2,"area":3.0,"num_cells_by_type":{"NAND2_X1":1,"$_DFF_P_":1}}}}"#.into())]),
        pass_timings: HashMap::new(),
    };
    let measurements = SynthMeasurements::parse(&run).unwrap();
    assert_eq!(measurements.area, None);
    assert_eq!(measurements.cell_count.len(), 2);
}
