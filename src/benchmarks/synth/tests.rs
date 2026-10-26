use super::*;
use crate::experiment::{ArtifactPolicy, Procedure, RunContext};

#[test]
fn comparison_combines_multiple_passes_without_an_external_reference() {
    use crate::experiment::BenchmarkOut;
    use std::fs;

    let root = tempfile::tempdir().unwrap();
    let executable = crate::test_support::tool("yosys", Some("YOSYS"));
    let workspace = root.path().join("workspace");
    let mut context = RunContext::new(workspace.clone(), ArtifactPolicy::new(false, false, false));
    let design = design::Design {
        files: vec![SourceFile::new(
            "top.v",
            "module top(input [7:0] a,b, output [7:0] y); assign y=a+b; endmodule",
        )],
        ..design::Design::default()
    };
    let result = comparison::compare(
        &Yosys::new(executable),
        &design,
        &SynthConfig::new(
            CellLibrary::from_path(crate::test_support::file("MCM_LIBERTY")).unwrap(),
            Vec::new(),
        ),
        &mut context,
        |job| {
            job.stage_with_statistics("opt", vec!["opt".into()])?;
            job.stage_with_statistics("clean", vec!["clean".into()])
        },
        |run| {
            for stage in ["opt-stat", "clean-stat"] {
                assert_eq!(run.statistics(stage)?.cell_count.get("$add"), Some(&1));
            }
            assert!(run.statistics("stat")?.area.is_some_and(|area| area > 0.0));
            SynthMeasurements::parse(run)
        },
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(result.area_savings(), Some(0.0));
    assert!(
        result
            .optimized
            .measurements()
            .optimization
            .area()
            .is_some_and(|area| area > 0.0)
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
fn real_synthesis_counts_hierarchy_and_maps_flip_flops() {
    let yosys = Yosys::new(crate::test_support::tool("yosys", Some("YOSYS")));
    let library = CellLibrary::from_path(crate::test_support::file("MCM_LIBERTY")).unwrap();
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
    let stats = crate::tools::yosys::Statistics::parse(r#"{"modules":{"top":{"num_cells":2,"area":3.0,"num_cells_by_type":{"NAND2_X1":1,"$_DFF_P_":1}}}}"#).unwrap();
    let measurements = SynthMeasurements::from_statistics(stats);
    assert_eq!(measurements.area, None);
    assert_eq!(measurements.cell_count.len(), 2);
}
