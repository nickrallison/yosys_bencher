use super::*;
use crate::experiment::{ArtifactPolicy, Procedure, RunContext};

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
fn synthesis_measurements_preserve_counts_and_only_report_fully_mapped_area() {
    for (cells, expected) in [
        (vec![("NAND2", 1)], Some(3.0)),
        (vec![("NAND2", 1), ("$_DFF_P_", 1)], None),
        (vec![], Some(0.0)),
    ] {
        let stats = crate::tools::yosys::Statistics {
            area: Some(3.0),
            cell_count: cells
                .into_iter()
                .map(|(name, count)| (name.into(), count))
                .collect(),
        };
        let measurements = SynthMeasurements::from_statistics(stats.clone());
        assert_eq!(measurements.area, expected);
        assert_eq!(measurements.cell_count, stats.cell_count);
    }
}

#[test]
fn comparison_reports_area_savings_only_when_both_areas_and_a_positive_baseline_exist() {
    use comparison::{ComparisonOutput, OptimizedMeasurements};
    use std::time::Duration;
    let synthesis = |area| {
        SynthMeasurements::from_statistics(crate::tools::yosys::Statistics {
            area,
            cell_count: [("NAND2".into(), 1)].into(),
        })
    };
    for (baseline, optimized, expected) in [
        (Some(40.0), Some(20.0), Some(50.0)),
        (Some(40.0), Some(80.0), Some(-100.0)),
        (Some(0.0), Some(20.0), None),
        (None, Some(20.0), None),
        (Some(40.0), None, None),
    ] {
        let output = ComparisonOutput {
            reference: (),
            baseline: SynthOutput::new(Duration::ZERO, synthesis(baseline)),
            optimized: YosysBenchmarkOutput::new(
                Duration::ZERO,
                OptimizedMeasurements {
                    optimization: (),
                    synthesis: synthesis(optimized),
                },
            ),
        };
        assert_eq!(output.area_savings(), expected);
    }
}
