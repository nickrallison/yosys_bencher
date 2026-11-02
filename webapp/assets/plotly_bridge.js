const host = document.getElementById("analysis-plot");
const colors = ["#4b72b8", "#d6368f", "#31508a", "#9c286a", "#819ed0", "#ea91c2"];
const theme = getComputedStyle(document.documentElement);

const formatNumber = value => Number.isFinite(value) ? String(Number(value.toPrecision(5))) : "unavailable";
const escapeText = value => String(value || "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

function makeTraces(chart, options) {
  return chart.series.flatMap((series, index) => {
    const color = colors[index % colors.length];
    const visible = value => Number.isFinite(value) && (options.y_scale !== "log" || value > 0);
    const spreads = series.points.map(point => point.y == null ? null : point.spread);
    const population = chart.spread_population || "repetitions of one input within one run";
    const customdata = series.points.map((point, i) => {
      const spread = spreads[i];
      const details = (options.show_spread || options.show_range) && spread
        ? ` · ${population}: n=${spread.count}, mean=${formatNumber(spread.mean)}, min=${formatNumber(spread.min)}, max=${formatNumber(spread.max)}, sample SD=${formatNumber(spread.standard_deviation)}`
        : "";
      return [point.row, escapeText((point.hover || "") + details)];
    });
    const traces = [];
    function whiskers(kind) {
      const intervals = spreads.map((spread, i) => {
        if (!spread || spread.count < 2 || !visible(series.points[i].y)) return null;
        const lower = kind === "range" ? spread.min : spread.mean - spread.standard_deviation;
        const upper = kind === "range" ? spread.max : spread.mean + spread.standard_deviation;
        if (kind === "sd" && !Number.isFinite(spread.standard_deviation)) return null;
        if (!visible(lower) || !visible(upper)) return null;
        const center = kind === "range" ? lower / 2 + upper / 2 : spread.mean;
        const plus = kind === "sd" ? spread.standard_deviation : upper - center;
        const minus = kind === "sd" ? spread.standard_deviation : center - lower;
        return Number.isFinite(plus) && Number.isFinite(minus) ? { center, plus, minus } : null;
      });
      if (!intervals.some(Boolean)) return;
      traces.push({
        uid: `${series.id}-${kind}`, name: series.label, legendgroup: series.id,
        type: "scatter", mode: "markers", showlegend: false, hoverinfo: "skip",
        x: series.points.map(point => point.x), y: intervals.map(value => value?.center ?? null),
        customdata,
        marker: { color, size: 0, opacity: 0 },
        opacity: kind === "range" ? 0.4 : 0.8,
        error_y: {
          type: "data", symmetric: false, visible: true, color,
          array: intervals.map(value => value?.plus ?? null),
          arrayminus: intervals.map(value => value?.minus ?? null),
          thickness: kind === "range" ? 1 : 3, width: kind === "range" ? 7 : 4
        }
      });
    }
    if (series.kind !== "bar") {
      if (options.show_range) whiskers("range");
      if (options.show_spread) whiskers("sd");
    }
    traces.push({
      uid: series.id, name: series.label, legendgroup: series.id,
      type: series.kind || "scatter", mode: "lines+markers", connectgaps: false,
      x: series.points.map(point => point.x),
      // Keep point indices and original Rust values intact on logarithmic axes.
      y: series.points.map(point => visible(point.y) ? point.y : null),
      customdata,
      marker: { color, size: 7 }, line: { color, width: 2 },
      hovertemplate: "%{customdata[1]}<br>%{y}<extra>%{fullData.name}</extra>"
    });
    return traces;
  });
}

function makeLayout(chart, options) {
  const commits = chart.x_values;
  const references = options.show_reference_lines
    ? chart.reference_lines.filter(line => options.y_scale !== "log" || line.value > 0)
    : [];
  const bound = value => value == null ? null : options.y_scale === "log" ? Math.log10(value) : value;
  const range = options.y_min == null && options.y_max == null ? undefined : [bound(options.y_min), bound(options.y_max)];
  return {
    autosize: true,
    uirevision: chart.id,
    barmode: chart.barmode,
    paper_bgcolor: "#ffffff",
    plot_bgcolor: "#ffffff",
    font: { family: theme.fontFamily, color: theme.getPropertyValue("--ink").trim() },
    margin: { l: 72, r: 24, t: 24, b: 80 },
    legend: { orientation: "h", y: -0.25 },
    hovermode: "closest",
    dragmode: "pan",
    xaxis: {
      title: { text: chart.x_label },
      type: chart.x_type,
      categoryorder: "array", categoryarray: commits,
      tickmode: chart.x_type === "category" ? "array" : "auto",
      tickvals: chart.x_type === "category" ? commits : undefined,
      ticktext: chart.x_type === "category" ? chart.x_labels : undefined,
      gridcolor: theme.getPropertyValue("--grid").trim(),
      tickangle: -40, automargin: true
    },
    yaxis: {
      title: { text: chart.y_label },
      type: options.y_scale,
      range,
      autorange: range ? undefined : true,
      gridcolor: theme.getPropertyValue("--grid").trim(),
      zerolinecolor: theme.getPropertyValue("--grid").trim(),
      automargin: true
    },
    shapes: references.map(line => ({
      type: "line", xref: "paper", x0: 0, x1: 1,
      y0: line.value, y1: line.value,
      line: { color: theme.getPropertyValue("--pink").trim(), dash: "dash", width: 1 }
    })),
    annotations: references.map(line => ({
      xref: "paper", x: 1, y: bound(line.value), text: line.label,
      showarrow: false, xanchor: "right", yshift: 10,
      font: { color: theme.getPropertyValue("--pink").trim(), size: 11 }
    }))
  };
}

let spec = await dioxus.recv();
while (spec) {
  if (spec.chart) {
    await Plotly.react(host, makeTraces(spec.chart, spec.options), makeLayout(spec.chart, spec.options), {
      responsive: true, scrollZoom: true, displaylogo: false,
      modeBarButtonsToRemove: ["lasso2d", "select2d"]
    });
    host.removeAllListeners("plotly_click");
    host.on("plotly_click", event => {
      const point = event.points[0];
      if (point.customdata && point.customdata[0]) dioxus.send({ row_id: point.customdata[0] });
    });
  } else if (host.data) {
    Plotly.purge(host);
  }
  spec = await dioxus.recv();
}
Plotly.purge(host);
