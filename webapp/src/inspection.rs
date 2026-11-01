use crate::{api, model::*, output::Pager};
use dioxus::prelude::*;
use yosys_bench_ui_types::{analysis::*, api::*, experiment::*, store::*};
#[component]
pub fn Inspector(
    snapshot: Snapshot,
    inputs: Vec<InputDescription>,
    revisions: Vec<RevisionDescription>,
    row: String,
    on_close: EventHandler<()>,
    on_refresh: EventHandler<()>,
) -> Element {
    use_effect(|| {
        let _ = document::eval(
            "document.getElementById('result-inspector')?.scrollIntoView({behavior: 'smooth', block: 'start'});",
        );
    });
    let input_label = |id: InputId| {
        inputs
            .iter()
            .find(|i| i.id == id)
            .map(|i| i.label.clone())
            .unwrap_or_else(|| format!("Input {id}"))
    };
    let mut paging = use_signal(Paging::default);
    let mut request = use_reactive!(|(snapshot, row)| (snapshot.clone(), row.clone()));
    let data = use_resource(move || {
        let (snapshot, row) = request();
        let paging = paging();
        async move {
            api::result(
                api::inspect(InspectionRequest {
                    snapshot,
                    row,
                    paging,
                })
                .await,
            )
        }
    });
    let mut selected_run = use_signal(|| None::<RunId>);
    let mut selected_work = use_signal(|| None::<WorkItemId>);
    rsx! {
        section {
            id: "result-inspector",
            class: "details-card",
            aria_label: "Selected result details",
            div { class: "section-heading",
                h2 { "Selected result" }
                button { onclick: move |_| on_close.call(()), "Close" }
            }
            for revision in &revisions {
                details { class: "inspection-section",
                    summary { "{short(&revision.id)} · {revision.message.lines().next().unwrap_or_default()}" }
                    p { class: "muted", "{revision.author} · {commit_time(revision.committed_at)}" }
                    code { "{revision.id}" }
                    pre { class: "metadata-json", "{revision.message}" }
                }
            }
            match data.read().as_ref() {
                None => rsx! {
                    p { class: "muted", role: "status", "Loading contributors..." }
                },
                Some(Err(error)) => rsx! {
                    p { class: "error", role: "alert", "{error.message}" }
                    if matches!(error.kind, ErrorKind::Expired) {
                        button { onclick: move |_| on_refresh.call(()), "Refresh results" }
                    }
                },
                Some(Ok(details)) => rsx! {
                    p { class: "muted",
                        "Contributors from the resolved analysis snapshot. Excluded inputs and failed attempts remain available."
                    }
                    if details.estimates.total == 0 && details.attempts.total == 0 {
                        p { class: "muted", "No execution records exist for this selected slot." }
                    }
                    if !details.runs.is_empty() {
                        div { class: "inspection-section",
                            h3 { "Source runs" }
                            for run in &details.runs {
                                button {
                                    class: "row-link",
                                    onclick: {
                                        let id = run.id;
                                        move |_| selected_run.set(Some(id))
                                    },
                                    "Run {run.id} · {run.environment.machine} · {run.state:?}"
                                }
                            }
                        }
                    }
                    if !details.estimates.items.is_empty() {
                        div { class: "table-scroll",
                            table {
                                thead {
                                    tr {
                                        th { "Input" }
                                        th { "Revision" }
                                        th { "Estimate" }
                                        th { "Valid / planned repetitions" }
                                        th { "Repetition sample SD" }
                                        th { "Source runs" }
                                        th { "Reasons" }
                                    }
                                }
                                tbody {
                                    for e in &details.estimates.items {
                                        tr {
                                            td { "{input_label(e.input)}" }
                                            td { title: e.revision.clone(), "{short(&e.revision)}" }
                                            td { "{number(e.value)}" }
                                            td { "{e.valid_repetitions}/{e.planned_repetitions}" }
                                            td { "{number(e.standard_deviation)}" }
                                            td {
                                                for run in &e.run_ids {
                                                    button {
                                                        class: "row-link",
                                                        onclick: {
                                                            let id = *run;
                                                            move |_| selected_run.set(Some(id))
                                                        },
                                                        "Run {run}"
                                                    }
                                                }
                                            }
                                            td {
                                                for reason in &e.reasons {
                                                    div { "{reason}" }
                                                }
                                                if !e.newer_unfinished.is_empty() {
                                                    div { class: "notice compact",
                                                        "Newer unfinished work exists; an older completed item may be selected."
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if !details.observations.items.is_empty() {
                        details { class: "inspection-section",
                            summary { "Raw observations ({details.observations.total})" }
                            div { class: "table-scroll",
                                table {
                                    thead {
                                        tr {
                                            th { "Attempt" }
                                            th { "Run" }
                                            th { "Repetition" }
                                            th { "Value / formula operands" }
                                            th { "Reason" }
                                            th { "Observation identities" }
                                        }
                                    }
                                    tbody {
                                        for o in &details.observations.items {
                                            tr {
                                                td {
                                                    button {
                                                        class: "row-link",
                                                        onclick: {
                                                            let work = o.work_item;
                                                            move |_| selected_work.set(Some(work))
                                                        },
                                                        "{o.attempt}"
                                                    }
                                                }
                                                td { "{o.run}" }
                                                td { "{o.repetition+1}" }
                                                td {
                                                    if let Some((a, b)) = o.operands {
                                                        "Operands: {number(Some(a))}, {number(Some(b))}"
                                                    } else {
                                                        "{number(o.value)}"
                                                    }
                                                }
                                                td { "{o.reason.clone().unwrap_or_default()}" }
                                                td {
                                                    code { { o.identities
                                                            .join(", ") } }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    h3 { "Attempts ({details.attempts.total})" }
                    for attempt in &details.attempts.items {
                        AttemptDetails { key: "{attempt.id}", attempt: attempt.clone() }
                    }
                    Pager {
                        offset: paging().offset,
                        limit: paging()
                                .limit,
                        total: details.estimates.total.max(details.observations.total)
                                .max(details.attempts.total),
                        on_change: move |p| paging.set(p),
                    }
                },
            }
            if let Some(id) = selected_run() {
                RunDetails {
                    key: "{id}",
                    id,
                    on_work: move | w |
                            selected_work.set(Some(w)),
                }
            }
            if let Some(id) = selected_work() {
                WorkDetails { key: "{id}", id }
            }
        }
    }
}
#[component]
fn AttemptDetails(attempt: AttemptInspection) -> Element {
    let outcome = match attempt.record.as_ref().map(|r| &r.outcome) {
        Some(AttemptOutcome::Success(_)) => "Success",
        Some(AttemptOutcome::Failure(_)) => "Failed",
        Some(AttemptOutcome::Unsupported(_)) => "Unsupported",
        None => "Unfinished",
    };
    rsx! {
        details { class: "attempt-details",
            summary { "Attempt {attempt.id} · repetition {attempt.repetition+1} · {outcome}" }
            p { class: "muted",
                "Work item {attempt.work_item} · started {timestamp(attempt.started_at)} · artifacts {attempt.artifact_state:?}"
            }
            if let Some(record) = &attempt.record {
                match &record.outcome {
                    AttemptOutcome::Failure(reason) | AttemptOutcome::Unsupported(reason) => {
                        rsx! {
                            p { class: "error", "{reason}" }
                        }
                    }
                    _ => rsx! {},
                }
                if !record.diagnostics.is_empty() {
                    ul {
                        for diagnostic in &record.diagnostics {
                            li { "{diagnostic}" }
                        }
                    }
                }
                details {
                    summary { "Raw output" }
                    pre { class: "metadata-json",
                        "{serde_json::to_string_pretty(&record.outcome).unwrap_or_default()}"
                    }
                }
            }
            if attempt.artifacts.is_empty() {
                p { class: "muted", "No retained artifact references." }
            }
            for artifact in &attempt.artifacts {
                ArtifactActions { key: "{artifact.id}", artifact: artifact.clone() }
            }
        }
    }
}
#[component]
fn ArtifactActions(artifact: Artifact) -> Element {
    let mut data = use_signal(|| None::<Result<api::ArtifactData, ApiError>>);
    let mut loading = use_signal(|| false);
    let id = artifact.id;
    let view_name = artifact.name.clone();
    let download_name = artifact.name.clone();
    rsx! {
        div { class: "artifact-row",
            span { "{artifact.name}" }
            if artifact.available {
                button {
                    disabled: loading(),
                    onclick: move |_| {
                        let name = view_name.clone();
                        loading.set(true);
                        spawn(async move {
                            data.set(Some(api::result(api::artifact(id, name).await)));
                            loading.set(false);
                        });
                    },
                    "View text"
                }
                button {
                    disabled: loading(),
                    onclick: move |_| {
                        let name = download_name.clone();
                        loading.set(true);
                        spawn(async move {
                            match api::result(api::artifact(id, name).await) {
                                Ok(file) => {
                                    let eval = document::eval(
                                        "const file = await dioxus.recv(); const url = URL.createObjectURL(new Blob([new Uint8Array(file.bytes)], {type: 'application/octet-stream'})); const a = document.createElement('a'); a.href = url; a.download = file.name.split('/').pop(); a.click(); setTimeout(() => URL.revokeObjectURL(url), 1000);",
                                    );
                                    let _ = eval.send(file);
                                }
                                Err(error) => data.set(Some(Err(error))),
                            }
                            loading.set(false);
                        });
                    },
                    "Download"
                }
            } else {
                span { class: "muted", "Unavailable" }
            }
            if let Some(reply) = data() {
                match reply {
                    Err(error) => rsx! {
                        p { class: "error", "{error.message}" }
                    },
                    Ok(file) => rsx! {
                        pre { class: "artifact-preview",
                            "{String::from_utf8_lossy(&file.bytes[..file.bytes.len().min(200_000)])}"
                        }
                        if file.bytes.len() > 200_000 {
                            p { class: "muted", "Preview truncated. Download for the complete file." }
                        }
                    },
                }
            }
        }
    }
}
#[component]
fn RunDetails(id: RunId, on_work: EventHandler<WorkItemId>) -> Element {
    let data = use_resource(move || async move { api::result(api::inspect_run(id).await) });
    rsx! {
        section { class: "inspection-section", aria_label: "Run details",
            h3 { "Run {id}: current invocation status" }
            match data.read().as_ref() {
                None => rsx! {
                    p { role: "status", "Loading run..." }
                },
                Some(Err(error)) => rsx! {
                    p { class: "error", "{error.message}" }
                },
                Some(Ok(run)) => rsx! {
                    p { "{run.run.state:?} · {run.run.environment.machine} · workload v{version(run.run.version)}" }
                    details {
                        summary { "Configuration and environment" }
                        pre { class: "metadata-json",
                            "{serde_json::to_string_pretty(&(&run.run.options,&run.run.environment)).unwrap_or_default()}"
                        }
                    }
                    details {
                        summary { "Revisions ({run.revisions.len()})" }
                        ul {
                            for revision in &run.revisions {
                                li {
                                    code { "{short(&revision.revision)}" }
                                    " · {revision.state:?}"
                                    if let Some(reason) = &revision.reason {
                                        p { class: "error", "{reason}" }
                                    }
                                }
                            }
                        }
                    }
                    details {
                        summary { "Work items ({run.work_items.len()})" }
                        div { class: "picker-list",
                            for work in &run.work_items {
                                button {
                                    class: "row-link",
                                    onclick: {
                                        let id = work.id;
                                        move |_| on_work.call(id)
                                    },
                                    "{short(&work.revision)} · input {work.input} · {work.state:?}"
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}
#[component]
fn WorkDetails(id: WorkItemId) -> Element {
    let mut paging = use_signal(Paging::default);
    let data = use_resource(move || {
        let page = paging();
        async move { api::result(api::inspect_work(id, page).await) }
    });
    rsx! {
        section { class: "inspection-section", aria_label: "Work item details",
            h3 { "Work item {id}: current status" }
            match data.read().as_ref() {
                None => rsx! {
                    p { role: "status", "Loading attempts..." }
                },
                Some(Err(error)) => rsx! {
                    p { class: "error", "{error.message}" }
                },
                Some(Ok(page)) => rsx! {
                    for attempt in &page.items {
                        AttemptDetails { key: "{attempt.id}", attempt: attempt.clone() }
                    }
                    Pager {
                        offset: page.offset,
                        limit: paging().limit,
                        total: page.total,
                        on_change: move |p| paging.set(p),
                    }
                },
            }
        }
    }
}
