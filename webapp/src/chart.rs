use crate::model::*;
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
const SCRIPT: &str = concat!(
    "/*\n",
    include_str!("../../ui/vendor/plotly.LICENSE"),
    "\n*/\n",
    include_str!("../../ui/vendor/plotly-basic.min.js"),
    "\n",
    include_str!("../assets/plotly_bridge.js"),
);
#[derive(Clone, PartialEq, Serialize)]
struct PlotSpec {
    chart: Option<ChartSpec>,
    options: PlotOptions,
}
#[derive(Deserialize)]
struct Selection {
    row_id: String,
}
#[component]
pub fn PlotlyChart(
    chart: Option<ChartSpec>,
    options: PlotOptions,
    on_point: EventHandler<String>,
) -> Element {
    let mut eval_handle = use_signal(|| None::<document::Eval>);
    let mut last_sent = use_signal(String::new);
    let mut handler = use_signal(|| on_point);
    let hidden = chart.is_none();
    let mut desired =
        use_reactive!(|(chart, options, on_point)| (chart.clone(), options.clone(), on_point));
    use_effect(move || {
        let (chart, mut options, callback) = desired();
        handler.set(callback);
        if options.bounds_error().is_some() {
            options.y_min = None;
            options.y_max = None;
        }
        let spec = PlotSpec { chart, options };
        let key = encoded(&spec);
        if *last_sent.peek() == key {
            return;
        }
        let existing = *eval_handle.peek();
        let eval = match existing {
            Some(eval) => eval,
            None => {
                let eval = document::eval(SCRIPT);
                let mut receiver = eval;
                spawn(async move {
                    while let Ok(selection) = receiver.recv::<Selection>().await {
                        handler().call(selection.row_id);
                    }
                });
                eval_handle.set(Some(eval));
                eval
            }
        };
        if eval.send(Some(spec)).is_ok() {
            last_sent.set(key);
        }
    });
    use_drop(move || {
        if let Some(eval) = *eval_handle.peek() {
            let _ = eval.send(Option::<PlotSpec>::None);
        }
    });
    rsx! {
        div {
            id: "analysis-plot",
            class: "plot-host",
            hidden,
            role: "img",
            aria_label: "Interactive analysis chart. Pan, zoom, or select a point to inspect measurements.",
        }
    }
}
