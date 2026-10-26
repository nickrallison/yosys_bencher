use super::*;
use crate::{
    experiment::ArtifactPolicy,
    test_support::{file, tool},
};
use std::{fs, process::Command, time::Duration};

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
    let yosys = Yosys::new(tool("yosys", Some("MCM_YOSYS")));
    let mut suite = McmSuite::new(
        CoefficientSets::Explicit(vec![vec![3, -11]]),
        CellLibrary::from_path(file("MCM_LIBERTY")).unwrap(),
        PaperTool {
            executable: tool("acm", Some("MCM_PAPER")),
        },
    );
    suite.options.search_budget = 1000;
    let session = suite.begin().unwrap();
    let tools = suite.tools(&yosys, &session).unwrap();
    let mut input = suite.inputs(&session).unwrap().remove(0);
    let workspace = tempfile::tempdir().unwrap();
    let mut context = RunContext::new(
        workspace.path().into(),
        ArtifactPolicy::new(true, true, true),
    );
    let result = McmProcedure.run(&input, &tools, &mut context).unwrap();
    assert!(result.baseline.measurements().area().is_some());
    assert!(result.optimized.measurements().synthesis.area().is_some());
    assert_eq!(result.optimized.measurements().optimization.multipliers, 0);
    assert!(result.area_savings().is_some());
    assert!(
        context.diagnostics().is_empty(),
        "{:?}",
        context.diagnostics()
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
    input.validate().unwrap();
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
        let output = Command::new(yosys.path())
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
