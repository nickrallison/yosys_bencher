use crate::{controls::Choice, model::*};
use dioxus::prelude::*;
use serde_json::{Value, json};
use yosys_bench_ui_types::experiment::*;
#[component]
pub fn Filters(
    fields: Vec<InputField>,
    filters: Vec<Filter>,
    on_change: EventHandler<Vec<Filter>>,
) -> Element {
    let current = filters.clone();
    let edit = use_callback(move |(index, value): (usize, Option<Filter>)| {
        let mut next = current.clone();
        if let Some(v) = value {
            next[index] = v
        } else {
            next.remove(index);
        }
        on_change.call(next);
    });
    let added = filters.clone();
    let first = fields.first().cloned();
    rsx! {
        div { class: "filters",
            for (index, filter) in filters.iter().enumerate() {
                FilterRow {
                    key: "{index}",
                    fields: fields.clone(),
                    filter: filter.clone(),
                    on_change: move |value| edit.call((index, Some(value))),
                    on_remove: move | _
                            | edit.call((index, None)),
                }
            }
            button {
                r#type: "button",
                class: "text-button",
                disabled: fields.is_empty(),
                onclick: move |_| {
                    if let Some(f) = &first {
                        let mut next = added.clone();
                        next.push(filter_default(f));
                        on_change.call(next);
                    }
                },
                "Add filter"
            }
        }
    }
}
#[component]
fn FilterRow(
    fields: Vec<InputField>,
    filter: Filter,
    on_change: EventHandler<Filter>,
    on_remove: EventHandler<()>,
) -> Element {
    let field = fields.iter().find(|f| f.path == filter.field).cloned();
    let kind = field.as_ref().map(|f| f.kind).unwrap_or(FieldKind::Text);
    let operator_options = kind
        .operators()
        .into_iter()
        .map(|op| {
            (
                encoded(&op),
                match op {
                    FilterOperator::Equal => "Equals",
                    FilterOperator::NotEqual => "Does not equal",
                    FilterOperator::Less => "Less than",
                    FilterOperator::AtMost => "At most",
                    FilterOperator::Greater => "Greater than",
                    FilterOperator::AtLeast => "At least",
                    FilterOperator::Contains => "Contains",
                    FilterOperator::Present => "Present",
                    FilterOperator::Missing => "Missing",
                }
                .into(),
            )
        })
        .collect();
    let by_field = fields.clone();
    let by_operator = filter.clone();
    let by_value = filter.clone();
    let text = value_text(&filter.value);
    let set_value = use_callback(move |v: Value| {
        let mut next = by_value.clone();
        next.value = v;
        on_change.call(next);
    });
    rsx! {
        div { class: "filter-row",
            div { class: "filter-heading",
                Choice {
                    label: "Filter field",
                    value: filter.field.clone(),
                    options: fields.iter().map(| f |
                            (f.path.clone(), f.label.clone())).collect(),
                    on_change: move |path| {
                        if let Some(f) = by_field.iter().find(|f| f.path == path) {
                            on_change.call(filter_default(f));
                        }
                    },
                }
                button {
                    class: "remove-button",
                    aria_label: "Remove filter",
                    onclick: move |_| on_remove.call(()),
                    "Remove"
                }
            }
            Choice {
                label: "Operator",
                value: encoded(&filter.operator),
                options: operator_options,
                on_change: move |value: String| {
                    if let Some(op) = decoded(&value) {
                        let mut next = by_operator.clone();
                        next.operator = op;
                        on_change.call(next);
                    }
                },
            }
            if !matches!(filter.operator, FilterOperator::Present | FilterOperator::Missing) {
                label { class: "field",
                    "Filter value"
                    match kind {
                        FieldKind::Number => rsx! {
                            input {
                                r#type: "number",
                                step: "any",
                                aria_label: "Filter value",
                                value: text,
                                onchange: move |e| {
                                    if let Ok(v) = e.value().parse::<f64>() && v.is_finite() {
                                        set_value.call(json!(v));
                                    }
                                },
                            }
                        },
                        FieldKind::Boolean => rsx! {
                            select {
                                aria_label: "Filter value",
                                value: text,
                                onchange: move | e | set_value
                                        .call(json!(e.value() == "true")),
                                option { value: "true", "True" }
                                option { value: "false", "False" }
                            }
                        },
                        FieldKind::Text => rsx! {
                            input {
                                r#type: "text",
                                aria_label: "Filter value",
                                value: text,
                                onchange: move | e |
                                        set_value.call(json!(e.value())),
                            }
                        },
                    }
                }
            }
        }
    }
}
