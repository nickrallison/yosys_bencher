use super::*;
use dioxus::dioxus_core::{AttributeValue, ElementId, Mutation, Mutations};
use std::{any::Any, collections::BTreeSet, rc::Rc};
use yosys_bench::{benchmarks::mcm::McmOutput, experiment::BenchmarkOut};

#[test]
fn every_registered_metric_is_reachable_with_its_original_identity() {
    let metrics = McmOutput::analysis_metrics();
    let root = level(&metrics, &[]);
    assert_eq!(root.derived.len(), 2);
    assert_eq!(
        root.groups.keys().map(String::as_str).collect::<Vec<_>>(),
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
        pending.extend(entries.groups.into_values());
    }
    assert_eq!(found.len(), metrics.len());
    assert_eq!(
        found.into_iter().collect::<BTreeSet<_>>(),
        metrics.iter().map(|m| m.path.clone()).collect()
    );
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

// Exercise Dioxus event handlers and rerenders without a browser or benchmark tools.
struct Picker {
    dom: VirtualDom,
    buttons: BTreeMap<String, ElementId>,
}
impl Picker {
    fn new() -> Self {
        set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
        let mut picker = Self {
            dom: VirtualDom::new(picker),
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
fn navigation_preserves_selection_and_leaf_clicks_keep_full_metric_paths() {
    let mut picker = Picker::new();
    assert!(picker.html().contains("Derived measurements"));
    assert!(!picker.html().contains("Open synthesis"));
    picker.click("Open optimized");
    assert!(picker.html().contains("Open synthesis"));
    assert!(!picker.html().contains("Mapped synthesis area"));
    assert!(picker.html().contains("optimized.runtime</output>"));

    picker.click("Open synthesis");
    assert!(picker.html().contains("Mapped synthesis area"));
    assert!(!picker.html().contains("Open perf"));
    picker.click("Mapped synthesis area");
    assert!(
        picker
            .html()
            .contains("optimized.measurements.synthesis.area</output>")
    );

    picker.click("Back to optimized");
    assert!(picker.html().contains("Open synthesis"));
    picker.click("Back to all measurements");
    assert!(picker.html().contains("Derived measurements"));
    assert!(
        picker
            .html()
            .contains("optimized.measurements.synthesis.area</output>")
    );

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
    picker.click("Show selected measurement: Execution states only");
    assert!(picker.html().contains("Derived measurements"));
}
