use super::McmInput;
use crate::{
    experiment::ExperimentError,
    tools::yosys::design::{Design, SourceFile},
};
type Result<T> = std::result::Result<T, ExperimentError>;

pub(super) fn build(input: &McmInput) -> Result<Design> {
    Ok(Design {
        files: vec![SourceFile::new("design.v", verilog(input)?)],
        top: Some("unoptimized".into()),
        ..Design::default()
    })
}

pub(super) fn verilog(benchmark: &McmInput) -> Result<String> {
    // A positive signed magnitude needs its own sign bit, including when unary
    // minus is applied. Product width is the sum of the signed operand widths.
    let widths: Vec<_> = benchmark
        .coefficients
        .iter()
        .map(|value| (64 - value.unsigned_abs().leading_zeros()).max(1) + 1)
        .collect();
    let mut ports = vec![format!(
        "    input wire signed [{}:0] x",
        benchmark.input_width - 1
    )];
    for (index, width) in widths.iter().enumerate() {
        let product = benchmark
            .input_width
            .checked_add(*width)
            .ok_or_else(|| ExperimentError::Config("product width overflow".into()))?;
        ports.push(format!(
            "    output wire signed [{}:0] y{index}",
            product - 1
        ));
    }
    let mut source = format!("module unoptimized(\n{}\n);\n", ports.join(",\n"));
    for (index, (&coefficient, width)) in benchmark.coefficients.iter().zip(widths).enumerate() {
        let sign = if coefficient < 0 { "-" } else { "" };
        source.push_str(&format!(
            "    assign y{index} = x * ({sign}{width}'sd{});\n",
            coefficient.unsigned_abs()
        ));
    }
    source.push_str("endmodule\n");
    Ok(source)
}
