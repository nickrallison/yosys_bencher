use super::*;
use dioxus::dioxus_core::{AttributeValue, ElementId, Mutation, Mutations};
use std::{
    any::Any,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
use yosys_bench::{benchmarks::mcm::McmOutput, experiment::BenchmarkOut};

fn trace_metrics() -> Vec<MetricSpec> {
    use yosys_bench::{benchmarks::yosys::YosysBenchmarkOutput, tools::yosys::YosysTimings};
    let timings = YosysTimings::parse(br#"{"generator":"test","total_ns":0,"passes":{}}"#)
        .unwrap()
        .with_call_trace(include_bytes!("../../../tests/fixtures/yosys-calls.json"))
        .unwrap();
    let output = YosysBenchmarkOutput::new(std::time::Duration::ZERO, ()).with_timings(timings);
    yosys_bench::metrics::catalog([&output])
        .into_iter()
        .map(|m| m.prefixed("baseline"))
        .collect()
}

#[test]
fn pass_navigation_preserves_call_order_and_exposes_parent_and_child_times() {
    let metrics = trace_metrics();
    let path = ["baseline", "timings", "calls", "synth"].map(str::to_owned);
    let synth = level(&metrics, &path);
    assert_eq!(
        synth
            .measurements
            .iter()
            .map(|m| m.name.as_str())
            .collect::<Vec<_>>(),
        ["runtime", "self runtime"]
    );
    assert_eq!(
        synth
            .groups
            .iter()
            .map(|(name, _)| group_label(name))
            .collect::<Vec<_>>(),
        ["proc", "opt", "opt (2)"]
    );
    let second_opt = &synth.groups[2].1;
    let opt = level(&metrics, second_opt);
    assert_eq!(
        opt.measurements[0].path,
        "baseline.timings.calls.synth.children.opt~2.runtime"
    );
    assert_eq!(
        parent(
            metrics
                .iter()
                .find(|m| m.path == opt.measurements[0].path)
                .unwrap()
        ),
        *second_opt
    );
    let roots = level(&metrics, &path[..3]);
    assert_eq!(
        roots
            .groups
            .iter()
            .map(|(name, _)| group_label(name))
            .collect::<Vec<_>>(),
        ["synth", "abc", "plugin.pass%~"]
    );
    let mut named_children = metrics[0].clone();
    named_children.path = "baseline.timings.calls.children.children.measurements.runtime".into();
    assert_eq!(
        hierarchy(&named_children),
        [
            "baseline",
            "timings",
            "calls",
            "children",
            "measurements",
            "runtime"
        ]
    );
}

#[test]
fn every_registered_metric_is_reachable_with_its_original_identity() {
    let metrics = McmOutput::analysis_metrics();
    let root = level(&metrics, &[]);
    assert_eq!(root.derived.len(), 2);
    assert_eq!(
        root.groups
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        ["baseline", "optimized", "reference"]
    );

    let mut pending = vec![vec![]];
    let mut found = Vec::new();
    while let Some(path) = pending.pop() {
        let entries = level(&metrics, &path);
        for metric in entries.derived.into_iter().chain(entries.measurements) {
            assert_eq!(
                parent(metrics.iter().find(|m| m.path == metric.path).unwrap()),
                path
            );
            found.push(metric.path);
        }
        pending.extend(entries.groups.into_iter().map(|(_, path)| path));
    }
    assert_eq!(found.len(), metrics.len());
    assert_eq!(
        found.into_iter().collect::<BTreeSet<_>>(),
        metrics.iter().map(|m| m.path.clone()).collect()
    );
}

#[test]
fn mcm_flows_share_synthesis_and_mcm_stats_groups() {
    let metrics = McmOutput::analysis_metrics();
    for flow in ["baseline", "optimized"] {
        let synth = level(&metrics, &[flow.into(), "synth".into()]);
        assert_eq!(
            synth
                .measurements
                .iter()
                .map(|m| m.name.as_str())
                .collect::<Vec<_>>(),
            ["area", "cell count"]
        );
        assert!(synth.groups.is_empty());
    }
    for flow in ["optimized", "reference"] {
        let stats = level(&metrics, &[flow.into(), "mcm_stats".into()]);
        for name in ["adds subs", "negations", "multipliers", "depth"] {
            assert!(
                stats.measurements.iter().any(|m| m.name == name),
                "{flow}: {name}"
            );
        }
        assert!(stats.groups.is_empty());
    }
    let baseline = level(&metrics, &["baseline".into()]);
    assert!(
        !baseline
            .measurements
            .iter()
            .any(|m| m.name == "area" || m.name == "cell count")
    );
    let reference = level(&metrics, &["reference".into()]);
    assert!(!reference.measurements.iter().any(|m| m.name == "adds subs"));
}

fn picker() -> Element {
    let mut value = use_signal(|| Some("optimized.runtime".to_string()));
    rsx! {
        MetricPicker {
            metrics: McmOutput::analysis_metrics(),
            value: value(),
            allow_none: true,
            on_change: move |path: String| value.set((!path.is_empty()).then_some(path)),
        }
        output { id: "selection", "{value().unwrap_or_default()}" }
    }
}

fn call_picker() -> Element {
    let mut value = use_signal(|| None::<String>);
    rsx! {
        MetricPicker {
            metrics: trace_metrics(),
            value: value(),
            allow_none: false,
            on_change: move |path: String| value.set(Some(path)),
        }
        output { id: "selection", "{value().unwrap_or_default()}" }
    }
}

// Exercise Dioxus event handlers and rerenders without a browser or benchmark tools.
struct Picker {
    dom: VirtualDom,
    buttons: BTreeMap<String, ElementId>,
}
impl Picker {
    fn new() -> Self {
        Self::component(picker)
    }
    fn component(component: fn() -> Element) -> Self {
        set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
        let mut picker = Self {
            dom: VirtualDom::new(component),
            buttons: BTreeMap::new(),
        };
        let edits = picker.dom.rebuild_to_vec();
        picker.update(edits);
        picker
    }
    fn update(&mut self, edits: Mutations) {
        for edit in edits.edits {
            if let Mutation::SetAttribute {
                name: "aria-label" | "title",
                value: AttributeValue::Text(value),
                id,
                ..
            } = edit
            {
                self.buttons.insert(value, id);
            }
        }
    }
    fn click(&mut self, name: &str) {
        let id = *self
            .buttons
            .get(name)
            .unwrap_or_else(|| panic!("No button: {name}"));
        let event = Event::new(
            Rc::new(PlatformEventData::new(Box::<SerializedMouseData>::default())) as Rc<dyn Any>,
            true,
        );
        self.dom.runtime().handle_event("click", event, id);
        let edits = self.dom.render_immediate_to_vec();
        self.update(edits);
    }
    fn html(&self) -> String {
        dioxus::ssr::render(&self.dom)
    }
}

#[test]
fn pass_picker_selects_inclusive_parent_and_repeated_subpass_times() {
    let mut picker = Picker::component(call_picker);
    for group in ["baseline", "timings", "calls", "synth"] {
        picker.click(&format!("Open {group}"));
    }
    picker.click("Inclusive CPU time for this pass invocation and its subpasses");
    assert!(
        picker
            .html()
            .contains("baseline.timings.calls.synth.runtime</output>")
    );
    picker.click("Open opt (2)");
    picker.click("Self CPU time for this pass invocation, excluding subpasses");
    assert!(
        picker
            .html()
            .contains("baseline.timings.calls.synth.children.opt~2.self_runtime</output>")
    );
    picker.click("Back to synth");
    picker.click("Show selected measurement: self runtime");
    assert!(picker.html().contains("opt (2)"));
}

#[test]
fn navigation_preserves_selection_and_leaf_clicks_keep_full_metric_paths() {
    let mut picker = Picker::new();
    assert!(picker.html().contains("Derived measurements"));
    assert!(!picker.html().contains("Open synth"));
    picker.click("Open optimized");
    assert!(picker.html().contains("Open synth"));
    assert!(!picker.html().contains("Mapped synthesis area"));
    assert!(picker.html().contains("optimized.runtime</output>"));

    picker.click("Open synth");
    assert!(picker.html().contains("Mapped synthesis area"));
    assert!(!picker.html().contains("Open perf"));
    picker.click("Mapped synthesis area");
    assert!(picker.html().contains("optimized.synth.area</output>"));

    picker.click("Back to optimized");
    assert!(picker.html().contains("Open synth"));
    picker.click("Back to all measurements");
    assert!(picker.html().contains("Derived measurements"));
    assert!(picker.html().contains("optimized.synth.area</output>"));

    picker.click("Show selected measurement: area");
    assert!(picker.html().contains("Mapped synthesis area"));
    picker.click("Back to optimized");
    picker.click("Back to all measurements");
    picker.click("Area saved against baseline");
    assert!(picker.html().contains("area_savings</output>"));
    picker.click("Show execution coverage without requiring a metric.");
    assert!(picker.html().contains("Execution states only"));
    assert!(!picker.html().contains("area_savings</output>"));
    picker.click("Open baseline");
    picker.click("Open synth");
    picker.click("Mapped synthesis area");
    assert!(picker.html().contains("baseline.synth.area</output>"));
    picker.click("Back to baseline");
    picker.click("Back to all measurements");
    picker.click("Open reference");
    picker.click("Open mcm stats");
    picker.click("Add/subtract operations");
    assert!(
        picker
            .html()
            .contains("reference.mcm_stats.adds_subs</output>")
    );
    picker.click("Back to reference");
    picker.click("Back to all measurements");
    picker.click("Show execution coverage without requiring a metric.");
    picker.click("Show selected measurement: Execution states only");
    assert!(picker.html().contains("Derived measurements"));
}
