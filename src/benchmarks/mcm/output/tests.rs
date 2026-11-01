use super::*;
use crate::metrics::{MetricSource, catalog, scalar};
use serde_json::{Value, json};
use std::time::Duration;

fn stored_payload() -> Value {
    json!({
        "baseline": {
            "runtime": Duration::from_secs(4),
            "measurements": {"area": 100.0, "cell_count": {"NAND": 6, "NOT": 4}}
        },
        "optimized": {
            "runtime": Duration::from_secs(6),
            "measurements": {
                "synthesis": {"area": 60.0, "cell_count": {"NAND": 3, "NOT": 2}},
                "optimization": {
                    "cpu_time": Duration::from_secs(1),
                    "adds_subs": 3, "negations": 1, "multipliers": 0, "depth": 2
                }
            }
        },
        "reference": {
            "id": 1,
            "executable": crate::tools::external::ExecutableId::from_bytes(b"paper"),
            "measured_at": std::time::SystemTime::UNIX_EPOCH,
            "os": "test", "arch": "test",
            "measurements": {
                "runtime": Duration::from_secs(2),
                "adds_subs": 4, "shifts": 2, "negations": 1, "multipliers": 0, "depth": 3
            }
        }
    })
}

#[test]
fn stored_outputs_expose_consistent_groups_without_changing_the_payload() {
    let payload = stored_payload();
    let output: McmOutput = serde_json::from_value(payload.clone()).unwrap();
    for (path, value) in [
        ("baseline.synth.area", 100.0),
        ("baseline.synth.cell_count", 10.0),
        ("optimized.synth.area", 60.0),
        ("optimized.synth.cell_count", 5.0),
        ("optimized.mcm_stats.adds_subs", 3.0),
        ("optimized.mcm_stats.cpu_time", 1.0),
        ("reference.mcm_stats.adds_subs", 4.0),
        ("reference.mcm_stats.runtime", 2.0),
        ("baseline.runtime", 4.0),
        ("optimized.runtime", 6.0),
    ] {
        assert_eq!(scalar(&output, path), Some(value), "{path}");
    }
    assert_eq!(serde_json::to_value(output).unwrap(), payload);
}

#[test]
fn derived_metrics_resolve_regrouped_operands_and_pass_timings_remain_available() {
    let mut output: McmOutput = serde_json::from_value(stored_payload()).unwrap();
    output.baseline = output.baseline.with_timings(crate::tools::yosys::YosysTimings::parse(
        br#"{"generator":"test","total_ns":10000000,"passes":{"opt":{"runtime_ns":10000000,"num_calls":2}}}"#,
    ).unwrap());
    let metrics = catalog([&output]);
    for (path, expected) in [
        ("area_savings", 40.0),
        ("optimized_to_reference_runtime", 3.0),
    ] {
        let metric = metrics.iter().find(|metric| metric.path == path).unwrap();
        let MetricSource::Formula {
            numerator,
            denominator,
            formula,
            ..
        } = &metric.source
        else {
            panic!("expected a formula for {path}");
        };
        assert_eq!(
            formula.apply(
                scalar(&output, numerator).unwrap(),
                scalar(&output, denominator).unwrap()
            ),
            Some(expected)
        );
    }
    assert_eq!(output.area_savings(), Some(40.0));
    let timing = "baseline.timings.passes.opt.runtime";
    assert!(metrics.iter().any(|metric| metric.path == timing));
    assert_eq!(scalar(&output, timing), Some(0.01));
}
