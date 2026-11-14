use crate::model::label;
use dioxus::prelude::*;
use yosys_bencher_ui_types::metrics::{MetricSource, MetricSpec};

#[derive(Clone, Debug, PartialEq)]
struct Measurement {
    path: String,
    name: String,
    unit: String,
    description: String,
}
impl From<&MetricSpec> for Measurement {
    fn from(metric: &MetricSpec) -> Self {
        let name = if metric.label == metric.path {
            label(metric.path.rsplit('.').next().unwrap_or(&metric.path))
        } else {
            label(&metric.label)
        };
        Self {
            path: metric.path.clone(),
            name,
            unit: metric.unit.clone(),
            description: metric.description.clone(),
        }
    }
}

// The payload's `measurements` wrapper adds no useful navigation level.
fn hierarchy(metric: &MetricSpec) -> Vec<String> {
    let parts = metric.path.split('.').collect::<Vec<_>>();
    let calls = parts
        .windows(2)
        .position(|pair| pair == ["timings", "calls"])
        .map(|index| index + 2);
    parts
        .into_iter()
        .enumerate()
        .filter(|(index, name)| {
            if let Some(start) = calls.filter(|start| index >= start) {
                !((index - start) % 2 == 1 && *name == "children")
            } else {
                *name != "measurements"
            }
        })
        .map(|(_, name)| name.to_owned())
        .collect()
}
fn group_label(name: &str) -> String {
    let (name, occurrence) = name
        .rsplit_once('~')
        .map_or((name, None), |(name, n)| (name, Some(n)));
    let name = label(name)
        .replace("%2E", ".")
        .replace("%7E", "~")
        .replace("%25", "%");
    occurrence.map_or_else(|| name.clone(), |n| format!("{name} ({n})"))
}
fn parent(metric: &MetricSpec) -> Vec<String> {
    if matches!(metric.source, MetricSource::Formula { .. }) {
        vec![]
    } else {
        let mut path = hierarchy(metric);
        path.pop();
        path
    }
}
#[derive(Clone, Debug, Default, PartialEq)]
struct Level {
    derived: Vec<Measurement>,
    measurements: Vec<Measurement>,
    groups: Vec<(String, Vec<String>)>,
}
fn level(metrics: &[MetricSpec], location: &[String]) -> Level {
    let mut level = Level::default();
    for metric in metrics {
        if matches!(metric.source, MetricSource::Formula { .. }) {
            if location.is_empty() {
                level.derived.push(metric.into());
            }
            continue;
        }
        let path = hierarchy(metric);
        if let Some(rest) = path.strip_prefix(location) {
            match rest {
                [] | [_] => level.measurements.push(metric.into()),
                [name, ..] => {
                    if !level.groups.iter().any(|(key, _)| key == name) {
                        let mut next = location.to_vec();
                        next.push(name.clone());
                        level.groups.push((name.clone(), next));
                    }
                }
            }
        }
    }
    level
}

#[component]
pub fn MetricPicker(
    metrics: Vec<MetricSpec>,
    value: Option<String>,
    allow_none: bool,
    on_change: EventHandler<String>,
) -> Element {
    let mut location = use_signal(Vec::<String>::new);
    let selected = metrics.iter().find(|m| Some(&m.path) == value.as_ref());
    let current_name = selected
        .map(|m| Measurement::from(m).name)
        .unwrap_or_else(|| {
            if allow_none {
                "Execution states only".into()
            } else {
                "Choose a measurement".into()
            }
        });
    let selected_parent = selected.map(parent).unwrap_or_default();
    let selected_path = selected
        .map(|m| {
            parent(m)
                .iter()
                .map(|s| group_label(s))
                .collect::<Vec<_>>()
                .join(" / ")
        })
        .unwrap_or_default();
    let current_unit = selected.map(|m| m.unit.clone()).unwrap_or_default();
    let current = location();
    let entries = level(&metrics, &current);
    let heading = if current.is_empty() {
        "All measurements".into()
    } else {
        current
            .iter()
            .map(|s| group_label(s))
            .collect::<Vec<_>>()
            .join(" / ")
    };
    let back_label = current
        .split_last()
        .map(|(_, parent)| {
            parent
                .last()
                .map(|s| group_label(s))
                .unwrap_or_else(|| "all measurements".into())
        })
        .unwrap_or_else(|| "all measurements".into());
    rsx! {
        div { class: "metric-field",
            span { class: "metric-field-label", if allow_none { "Metric availability" } else { "Metric" } }
            button {
                r#type: "button",
                class: "metric-current",
                title: "Show the selected measurement in the hierarchy",
                aria_label: "Show selected measurement: {current_name}",
                onclick: move |_| location.set(selected_parent.clone()),
                if !selected_path.is_empty() {
                    small { "{selected_path}" }
                }
                span { "{current_name}" }
                small { class: "metric-unit", "{current_unit}" }
            }
            div { class: "metric-browser", role: "group", aria_label: "Choose a metric",
                nav { class: "metric-navigation", aria_label: "Metric hierarchy",
                    button {
                        r#type: "button",
                        disabled: current.is_empty(),
                        aria_label: "Back to {back_label}",
                        onclick: move |_| { location.write().pop(); },
                        "Back"
                    }
                    span { class: "metric-location", aria_live: "polite", "{heading}" }
                    if !current.is_empty() {
                        button {
                            r#type: "button",
                            onclick: move |_| location.set(vec![]),
                            "All"
                        }
                    }
                }
                MetricLevel {
                    level: entries,
                    value,
                    allow_none: allow_none && current.is_empty(),
                    on_enter: move |path| location.set(path),
                    on_change,
                }
            }
        }
    }
}

#[component]
fn MetricLevel(
    level: Level,
    value: Option<String>,
    allow_none: bool,
    on_enter: EventHandler<Vec<String>>,
    on_change: EventHandler<String>,
) -> Element {
    let empty =
        level.derived.is_empty() && level.measurements.is_empty() && level.groups.is_empty();
    rsx! {
        div { class: "metric-entries",
            if allow_none {
                MetricButton {
                    measurement: Measurement {
                        path: String::new(),
                        name: "Execution states only".into(),
                        unit: String::new(),
                        description: "Show execution coverage without requiring a metric.".into(),
                    },
                    selected: value.is_none(),
                    on_change,
                }
            }
            if !level.derived.is_empty() {
                h3 { class: "metric-group-label", "Derived measurements" }
                for measurement in level.derived {
                    MetricButton {
                        key: "{measurement.path}",
                        selected: value.as_ref() == Some(&measurement.path),
                        measurement,
                        on_change,
                    }
                }
            }
            if !level.measurements.is_empty() {
                h3 { class: "metric-group-label", "Measurements" }
                for measurement in level.measurements {
                    MetricButton {
                        key: "{measurement.path}",
                        selected: value.as_ref() == Some(&measurement.path),
                        measurement,
                        on_change,
                    }
                }
            }
            if !level.groups.is_empty() {
                h3 { class: "metric-group-label", "Submeasurements" }
                for (name, path) in level.groups {
                    button {
                        key: "group-{name}",
                        r#type: "button",
                        class: "metric-entry metric-folder",
                        aria_label: "Open {group_label(&name)}",
                        onclick: move |_| on_enter.call(path.clone()),
                        span { "{group_label(&name)}" }
                        span { class: "metric-chevron", aria_hidden: "true" }
                    }
                }
            }
            if empty {
                p { class: "muted", "No measurements in this group. Use Back to return." }
            }
        }
    }
}

#[component]
fn MetricButton(
    measurement: Measurement,
    selected: bool,
    on_change: EventHandler<String>,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "metric-entry metric-value",
            title: measurement.description,
            aria_pressed: selected,
            onclick: move |_| on_change.call(measurement.path.clone()),
            span { "{measurement.name}" }
            span { class: "metric-unit", "{measurement.unit}" }
            if selected { small { class: "metric-selected-label", "Selected" } }
        }
    }
}

#[cfg(all(test, feature = "server"))]
mod tests;
