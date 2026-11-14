use crate::api::Metadata;
use crate::{
    api,
    controls::{Choice, Edit},
    filters::Filters,
    model::*,
};
use dioxus::prelude::*;
use yosys_bencher_ui_types::{analysis::Paging, api::InputRequest, store::*};
#[component]
pub fn InputPicker(
    meta: Metadata,
    selection: ResultSelection,
    on_edit: EventHandler<Edit>,
) -> Element {
    let mut query = use_signal(String::new);
    let mut offset = use_signal(|| 0usize);
    let fetch_selection = selection.clone();
    let mut selected = use_reactive!(|(fetch_selection,)| fetch_selection.clone());
    let page = use_resource(move || {
        let selection = selected();
        let search = query();
        let offset = offset();
        let key = encoded(&(&selection, &search, offset));
        async move {
            (
                key,
                api::result(
                    api::inputs(InputRequest {
                        selection,
                        search,
                        paging: Paging { offset, limit: 40 },
                    })
                    .await,
                ),
            )
        }
    });
    let request_key = encoded(&(&selection, query(), offset()));
    let current = page
        .read()
        .as_ref()
        .filter(|(key, _)| *key == request_key)
        .map(|(_, r)| r.clone());
    let selected_mode = match selection.inputs {
        InputSelection::All => "all",
        InputSelection::Ids { .. } => "ids",
        InputSelection::Matching { .. } => "matching",
    };
    let predicate = selection.inputs.clone();
    let matching = selection.inputs.clone();
    let all_search = query();
    let mode = use_callback(move |m: String| {
        query.set(String::new());
        offset.set(0);
        on_edit.call(Edit::Inputs(match m.as_str() {
            "ids" => InputSelection::Ids { ids: vec![] },
            "matching" => InputSelection::Matching {
                filters: vec![],
                search: String::new(),
                excluded: vec![],
            },
            _ => InputSelection::All,
        }));
    });
    rsx! {
        div { class: "input-picker",
            Choice {
                label: "Inputs",
                value: selected_mode,
                options: vec![
                    ("all".into(), "All inputs".into()),
                    ("ids".into(), "Choose inputs".into()),
                    ("matching".into(), "Filter inputs…".into()),
                ],
                on_change: mode,
            }
            if let InputSelection::Matching { filters, search, excluded } = &selection.inputs {
                label { class: "field",
                    "Workload search"
                    input {
                        r#type: "search",
                        aria_label: "Workload search",
                        placeholder: "Match labels or parameter values",
                        value: search.clone(),
                        onchange: {
                            let f = filters.clone();
                            let ex = excluded.clone();
                            move |e| {
                                offset.set(0);
                                on_edit
                                    .call(
                                        Edit::Inputs(InputSelection::Matching {
                                            filters: f.clone(),
                                            search: e.value(),
                                            excluded: ex.clone(),
                                        }),
                                    );
                            }
                        },
                    }
                }
                Filters {
                    fields: meta.description.fields.clone(),
                    filters: filters.clone(),
                    on_change: {
                        let search = search.clone();
                        move |filters| {
                            offset.set(0);
                            on_edit
                                .call(
                                    Edit::Inputs(InputSelection::Matching {
                                        filters,
                                        search: search.clone(),
                                        excluded: vec![],
                                    }),
                                );
                        }
                    },
                }
            }
            details { class: "picker", open: selected_mode != "all",
                summary {
                    match &current {
                        Some(Ok(p)) => rsx! { "{p.selected} of {p.target} inputs selected" },
                        _ => rsx! { "Browse inputs" },
                    }
                }
                div { class: "picker-body",
                    input {
                        r#type: "search",
                        aria_label: "Search input list",
                        placeholder: "Search this list",
                        value: query(),
                        oninput: move |e| {
                            query.set(e.value());
                            offset.set(0);
                        },
                    }
                    div { class: "picker-actions",
                        button { onclick: move | _ | on_edit
                                    .call(Edit::Inputs(InputSelection::All)),
                            "All inputs"
                        }
                        button { onclick: move |_| on_edit.call(Edit::Inputs(InputSelection::Ids { ids: vec![] })),
                            "None"
                        }
                        button {
                            disabled: matches!(selection.inputs, InputSelection::Matching
                                    { .. }) &&! query().is_empty(),
                            title: "For an existing matching selection, use Workload search to change its predicate.",
                            onclick: move |_| {
                                let (filters, search) = match &matching {
                                    InputSelection::Matching { filters, search, .. } => {
                                        (
                                            filters.clone(),
                                            if all_search.is_empty() {
                                                search.clone()
                                            } else {
                                                all_search.clone()
                                            },
                                        )
                                    }
                                    _ => (vec![], all_search.clone()),
                                };
                                on_edit
                                    .call(
                                        Edit::Inputs(InputSelection::Matching {
                                            filters,
                                            search,
                                            excluded: vec![],
                                        }),
                                    );
                                query.set(String::new());
                                offset.set(0);
                            },
                            "Select all matching"
                        }
                    }
                    match current {
                        None => rsx! {
                            p { class: "muted", role: "status", "Loading inputs..." }
                        },
                        Some(Err(error)) => rsx! {
                            p { class: "error", role: "alert", "{error.message}" }
                        },
                        Some(Ok(p)) => rsx! {
                            div { class: "picker-list",
                                for input in &p.page.items {
                                    label { class: "check-row", title: encoded(&input.parameters),
                                        input {
                                            r#type: "checkbox",
                                            checked: input_selected(&selection.inputs, input),
                                            onchange: {
                                                let mut next = predicate.clone();
                                                let id = input.id;
                                                move |_| {
                                                    toggle_input(&mut next, id);
                                                    on_edit.call(Edit::Inputs(next.clone()));
                                                }
                                            },
                                        }
                                        span {
                                            "{input.label}"
                                            small { "Input {input.id}" }
                                        }
                                    }
                                }
                            }
                            p { class: "muted", "{p.page.total} matching inputs. Selection includes all pages." }
                            div { class: "picker-actions",
                                button {
                                    disabled: p.page.offset == 0,
                                    onclick: move |_| {
                                        let n = offset();
                                        offset.set(n.saturating_sub(40));
                                    },
                                    "Previous"
                                }
                                button {
                                    disabled: p.page.offset + p.page.items.len() >= p.page.total,
                                    onclick: move |_| {
                                        let n = offset();
                                        offset.set(n + 40);
                                    },
                                    "Next"
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}
