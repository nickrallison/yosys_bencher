use super::McmInput;
use crate::experiment::ExperimentError;
type Result<T> = std::result::Result<T, ExperimentError>;

pub(super) fn validate(benchmark: &McmInput) -> Result<()> {
    let invalid = |message: &str| ExperimentError::Config(message.into());
    if benchmark.coefficients.is_empty() {
        return Err(invalid("coefficient set is empty"));
    }
    if benchmark.input_width == 0 || benchmark.input_width > i32::MAX as u32 - 33 {
        return Err(invalid(
            "input width is outside the supported signed Verilog range",
        ));
    }
    if !(1..=30).contains(&benchmark.options.paper_bit_width) {
        return Err(invalid("paper bit width must be between 1 and 30"));
    }
    if benchmark.options.paper_seed > i32::MAX as u64 {
        return Err(invalid(
            "paper seed exceeds its signed 32-bit command-line range",
        ));
    }
    if benchmark.options.max_depth == 0
        || benchmark.options.max_depth > i32::MAX as u32
        || benchmark.options.search_budget == 0
        || benchmark.options.search_budget > i64::MAX as u64
    {
        return Err(invalid(
            "MCM depth and search budget must be positive signed integers",
        ));
    }
    for &coefficient in &benchmark.coefficients {
        if coefficient == 0 {
            return Err(ExperimentError::Unsupported(
                "the paper tool does not support zero coefficients".into(),
            ));
        }
        if i32::try_from(coefficient).is_err()
            || coefficient.unsigned_abs() >= (1u64 << (benchmark.options.paper_bit_width + 1))
        {
            return Err(invalid(
                "coefficient exceeds the paper tool's integer/search range",
            ));
        }
    }
    Ok(())
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
