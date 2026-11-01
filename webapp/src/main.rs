use dioxus::prelude::*;
use yosys_bench_ui_types::{analysis::*, api::*, store::*};
mod api;
mod chart;
mod controls;
mod filters;
mod inputs;
mod inspection;
mod metric_picker;
mod model;
mod output;
use model::*;
const THEME: &str = include_str!("../../ui/theme.css");
fn main() {
    #[cfg(feature = "server")]
    dioxus::serve(|| async move { Ok(dioxus::server::router(App)) });
    #[cfg(not(feature = "server"))]
    dioxus::launch(App);
}
#[component]
fn App() -> Element {
    let mut epoch = use_signal(|| 0_u64);
    let catalog = use_server_future(move || {
        let _ = epoch();
        async move { api::result(api::catalog().await) }
    })?;
    rsx! {
        style { dangerous_inner_html: THEME }
        match catalog.read().as_ref() {
            None => rsx! {
                main { class: "state-page",
                    h1 { "Results" }
                    p { role: "status", "Loading experiments..." }
                }
            },
            Some(Err(error)) => rsx! {
                main { class: "state-page",
                    h1 { "Unable to load experiments" }
                    p { class: "error", role: "alert", "{error.message}" }
                    button {
                        onclick: move |_| {
                            let n = epoch();
                            epoch.set(n + 1);
                        },
                        "Retry"
                    }
                }
            },
            Some(Ok(experiments)) => rsx! {
                Workspace {
                    experiments: experiments.clone(),
                    epoch: epoch(),
                    on_refresh: move |_| {
                        let n = epoch();
                        epoch.set(n + 1);
                    },
                }
            },
        }
    }
}
#[component]
fn Workspace(
    experiments: Vec<ExperimentSummary>,
    epoch: u64,
    on_refresh: EventHandler<()>,
) -> Element {
    let first = experiments.first().map(|e| e.id);
    let mut selected = use_signal(|| first);
    let experiment_ids = experiments.iter().map(|e| e.id).collect::<Vec<_>>();
    let mut available = use_reactive!(|(experiment_ids,)| experiment_ids.clone());
    use_effect(move || {
        let ids = available();
        if !selected().is_some_and(|id| ids.contains(&id)) {
            selected.set(ids.first().copied());
        }
    });
    let mut refresh = use_reactive!(|(epoch,)| epoch);
    let metadata = use_resource(move || {
        let id = selected();
        let _ = refresh();
        async move {
            if let Some(id) = id {
                Some((id, api::result(api::metadata(id).await)))
            } else {
                None
            }
        }
    });
    let current = metadata
        .read()
        .as_ref()
        .and_then(|v| v.as_ref())
        .filter(|(id, _)| Some(*id) == selected())
        .map(|(_, r)| r.clone());
    let body = if experiments.is_empty() {
        rsx! {section{class:"empty-state",h2{"No experiments recorded"}p{"Run a benchmark, then refresh to explore its results."}}}
    } else {
        match current {
            None => rsx! {section{class:"empty-state",p{role:"status","Loading experiment..."}}},
            Some(Err(error)) => rsx! {p{class:"error",role:"alert","{error.message}"}},
            Some(Ok(meta)) => {
                rsx! {LoadedWorkspace{key:"{meta.description.experiment.id}",meta,epoch,on_refresh}}
            }
        }
    };
    let choose = move |value: String| {
        if let Ok(id) = value.parse() {
            selected.set(Some(ExperimentId(id)));
        }
    };
    rsx! {
        main { class:"workspace",
            header { class:"page-heading",
                div {h1 {"Results"} p {class:"muted","Performance across revisions"}}
                div {class:"controls",
                    controls::Choice {
                        label:"Experiment",
                        value:selected().map(|id|id.to_string()).unwrap_or_default(),
                        options:experiments.iter().map(|e|(e.id.to_string(),e.spec.name.clone())).collect(),
                        on_change:choose,
                    }
                    button {class:"refresh-button",onclick:move|_|on_refresh.call(()),"Refresh"}
                }
            }
            {body}
        }
    }
}
#[component]
fn LoadedWorkspace(meta: api::Metadata, epoch: u64, on_refresh: EventHandler<()>) -> Element {
    let mut draft = use_signal(|| initial(&meta));
    let mut applied = use_signal(|| initial(&meta));
    let mut plot = use_signal(PlotOptions::default);
    let mut notice = use_signal(|| None::<String>);
    let mut selected_row = use_signal(|| None::<String>);
    let fetch_meta = meta.clone();
    let mut props = use_reactive!(|(fetch_meta, epoch)| (fetch_meta.clone(), epoch));
    let result = use_resource(move || {
        let (meta, epoch) = props();
        let plan = applied();
        let key = encoded(&(&plan, epoch));
        async move {
            let response = if let Some(message) = ready(&meta, &plan) {
                Err(ApiError {
                    kind: ErrorKind::Validation,
                    field: None,
                    message,
                })
            } else {
                api::result(api::analyze(plan).await)
            };
            (key, response)
        }
    });
    let key = encoded(&(applied(), epoch));
    let current = result
        .read()
        .as_ref()
        .filter(|(k, _)| *k == key)
        .map(|(_, r)| r.clone());
    let (response, error, loading) = match current {
        Some(Ok(response)) => (Some(response), None, false),
        Some(Err(error)) => (None, Some(error), false),
        None => (None, None, true),
    };
    let pending = encoded(&draft()) != encoded(&applied());
    let edit_meta = meta.clone();
    let edit = use_callback(move |change: controls::Edit| {
        let mut next = draft();
        let message = controls::apply(&mut next, change, &edit_meta);
        draft.set(next);
        notice.set(message);
    });
    let d = &meta.description;
    let measured = d
        .experiment
        .last_measured_at
        .map(timestamp)
        .unwrap_or_else(|| "Never".into());
    let error_for_sidebar = error.clone();
    let output_meta = meta.clone();
    rsx! {
        div { class: "experiment-summary",
            span { "{d.experiment.spec.description}" }
            span { class: "muted",
                "{d.experiment.successful_attempts} successful attempts · {d.experiment.measured_revisions} measured revisions"
            }
            details {
                summary { "Experiment details" }
                dl {
                    dt { "Created" }
                    dd { "{timestamp(d.experiment.created_at)}" }
                    dt { "Repository" }
                    dd { "{d.experiment.spec.repository}" }
                    dt { "Last measured" }
                    dd { "{measured}" }
                    dt { "Procedure" }
                    dd { "{d.experiment.contract.procedure}" }
                }
            }
            controls::Choice {
                label: "Workload version",
                value: draft().selection.version.map(version)
                        .unwrap_or_default(),
                options: d.versions
                    .iter()
                    .map(|v| (
                        version(v.version),
                        format!(
                            "v{} · {} inputs · {} successful attempts",
                            version(v.version),
                            v.input_count,
                            v.successful_attempts,
                        ),
                    ))
                    .collect(),
                on_change: {
                    let versions = d.versions.clone();
                    move |v: String| {
                        if let Some(def) = versions.iter().find(|d| version(d.version) == v) {
                            edit.call(controls::Edit::Version(def.version));
                        }
                    }
                },
            }
        }
        for warning in &meta.warnings {
            p { class: "notice", role: "status", "{warning}" }
        }
        if let Some(message) = notice() {
            p { class: "notice", role: "status", "{message}" }
        }
        div { class: "analysis-layout",
            controls::Sidebar {
                meta: meta.clone(),
                plan: draft(),
                plot: plot(),
                pending,
                error: error_for_sidebar,
                on_edit: edit,
                on_plot: move |p| plot.set(p),
                on_apply: move |_| {
                    applied.set(draft());
                    selected_row.set(None);
                },
            }
            section {
                class: "analysis-output",
                aria_label: "Analysis results",
                aria_busy: loading,
                nav { class: "recipe", aria_label: "Current analysis recipe", "{recipe(&applied())}" }
                if pending {
                    p { class: "notice compact", role: "status",
                        "Controls have changed. Update analysis to apply them."
                    }
                }
                if let Some(error) = error {
                    section { class: "details-card error", role: "alert",
                        h2 { "Analysis unavailable" }
                        p { "{error.message}" }
                        if matches!(error.kind, ErrorKind::Expired) {
                            button { onclick: move |_| on_refresh.call(()), "Refresh results" }
                        }
                    }
                }
                output::AnalysisOutput {
                    meta: output_meta,
                    result: response.clone(),
                    plot: plot(),
                    loading,
                    on_select: move | row | selected_row
                            .set(Some(row)),
                    on_page: move |paging: Paging| {
                        let before = applied();
                        let mut p = before.clone();
                        p.paging = paging;
                        if encoded(&draft()) == encoded(&before) {
                            draft.set(p.clone());
                        }
                        applied.set(p);
                        selected_row.set(None);
                    },
                }
                if let (Some(response), Some(row)) = (response, selected_row()) {
                    inspection::Inspector {
                        key: "{response.id}-{row}",
                        revisions: row_revisions(&response, &row).into_iter().filter_map(|id| meta.revisions.iter().find(|r| r.id == id).cloned()).collect(),
                        snapshot: response.snapshot,
                        inputs: response.inputs,
                        row,
                        on_close: move | _ | selected_row
                                .set(None),
                        on_refresh,
                    }
                }
            }
        }
    }
}
#[cfg(all(test, feature = "server"))]
mod tests;
