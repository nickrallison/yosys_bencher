const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync(`${__dirname}/plotly_bridge.js`, 'utf8');
const plain = value => JSON.parse(JSON.stringify(value));
async function render(chart, options = {}, click) {
  let render, handler, reads = 0, purged = false;
  const messages = [];
  const host = { removeAllListeners() {}, on(_, fn) { handler = fn; } };
  const context = {
    document: { getElementById() { return host; }, documentElement: {} },
    getComputedStyle() { return { fontFamily: 'sans-serif', getPropertyValue() { return '#202936'; } }; },
    Plotly: { async react(_, traces, layout) { render = { traces, layout }; host.data = traces; }, purge() { purged = true; } },
    dioxus: {
      async recv() {
        if (reads++ === 0) return { chart, options: { y_scale: 'linear', show_reference_lines: true, show_spread: true, ...options } };
        if (click) handler({ points: [{ data: { uid: 'ignored-position' }, pointIndex: 999, customdata: click }] });
        return null;
      },
      send(value) { messages.push(value); }
    }
  };
  await vm.runInNewContext(`(async () => { ${source}\n })()`, context);
  return { ...plain(render), messages: plain(messages), purged };
}
function spec(points) {
  return { id: 'snapshot-1', x_label: 'Width', y_label: 'Runtime (s)', x_type: 'linear', x_values: [8, 16], x_labels: ['8', '16'], barmode: 'group', reference_lines: [{value: 1, label: 'Reference'}], series: [{ id: 'series-1', label: 'Runtime', kind: 'scatter', points }] };
}
test('numeric X values and stable inspection IDs survive spread overlays', async () => {
  const result = await render(spec([{ x: 8, y: 2, spread: { count: 3, mean: 2, standard_deviation: 0.1, min: 1.9, max: 2.1 }, row: 'record-a', hover: 'Input 8' }]), {}, ['record-a', 'Input 8']);
  assert.equal(result.layout.xaxis.type, 'linear');
  assert.deepEqual(result.traces[0].x, [8]);
  assert.deepEqual(result.messages, [{row_id: 'record-a'}]);
  assert.deepEqual(result.traces.find(trace => trace.uid.endsWith('-sd')).error_y.array, [0.1]);
  assert.deepEqual(result.traces.find(trace => trace.uid === 'series-1').y, [2]);
  assert.ok(result.purged);
});

test('min-max and one SD are separate bars while the selected estimate stays unchanged', async () => {
  const chart = spec([{ x: 8, y: 12, row: 'a', spread: { count: 3, mean: 10, min: 2, max: 18, standard_deviation: 3 } }]);
  const result = await render(chart, { show_range: true });
  const [range, sd, line] = result.traces;
  assert.deepEqual(range.y, [10]);
  assert.deepEqual(range.error_y.array, [8]);
  assert.deepEqual(range.error_y.arrayminus, [8]);
  assert.deepEqual(sd.y, [10]);
  assert.deepEqual(sd.error_y.array, [3]);
  assert.deepEqual(line.y, [12]);
  assert.ok(range.error_y.thickness < sd.error_y.thickness);
  assert.equal(range.legendgroup, line.legendgroup);
  assert.equal(sd.showlegend, false);
  assert.match(line.customdata[0][1], /Mean: 10 · Min: 2 · Max: 18<br>SD: 3 · 3 measurements/);
  const hidden = await render(chart, { show_range: false, show_spread: false });
  assert.equal(hidden.traces.length, 1);
  assert.deepEqual(hidden.traces[0].y, [12]);
});

test('combined history has no spread and workload variation is explicit', async () => {
  const chart = spec([{ x: 8, y: 10, row: 'a', spread: null }]);
  const combined = await render(chart, { show_range: true, spread_population: 'inputs' });
  assert.equal(combined.traces.length, 1);
  assert.deepEqual(combined.traces[0].y, [10]);
  chart.spread_population = 'across inputs';
  chart.series[0].points[0].spread = { count: 3, mean: 10, min: 2, max: 18, standard_deviation: 3 };
  const variation = await render(chart);
  assert.deepEqual(variation.traces[0].error_y.array, [3]);
  assert.match(variation.traces[1].customdata[0][1], /3 measurements/);
});

test('hover uses one neutral card and wraps long labels without losing inspection IDs', async () => {
  const chart = spec([{ x: 8, y: 2, row: 'record-a', hover: '70bf41c29b ' + 'A long commit message '.repeat(8) }]);
  chart.series[0].label = 'input: 32-bit [-44666, 4693, -1249, -16947, 819, 33704, 123456789, 987654321]';
  const result = await render(chart, {}, ['record-a', 'details']);
  const trace = result.traces[0];
  assert.equal(trace.hoverlabel.bgcolor, '#ffffff');
  assert.equal(trace.hoverlabel.font.color, '#202936');
  assert.equal(trace.hoverlabel.bordercolor, trace.line.color);
  assert.match(trace.hovertemplate, /<b>%\{customdata\[2\]\}<\/b>/);
  assert.ok(trace.hovertemplate.endsWith('<extra></extra>'));
  for (const text of trace.customdata[0].slice(1)) {
    assert.ok(text.includes('<br>'));
    assert.ok(text.split('<br>').every(line => Array.from(line).length <= 52));
  }
  assert.deepEqual(result.messages, [{ row_id: 'record-a' }]);
});

test('input names and commit messages remain escaped inside the hover card', async () => {
  const chart = spec([{ x: 8, y: 2, row: 'a', hover: '<script>alert(1)</script>\nA & B' }]);
  chart.series[0].label = '<img src=x onerror=alert(1)>';
  const result = await render(chart);
  assert.equal(result.traces[0].customdata[0][2], '&lt;img src=x onerror=alert(1)&gt;');
  assert.equal(result.traces[0].customdata[0][1], '&lt;script&gt;alert(1)&lt;/script&gt;<br>A &amp; B');
});

test('insufficient samples, missing points, and invalid log intervals do not invent bars', async () => {
  const chart = spec([
    { x: 8, y: 1, spread: { count: 1, mean: 1, min: 1, max: 1, standard_deviation: null } },
    { x: 16, y: null, spread: { count: 3, mean: 1, min: 0, max: 2, standard_deviation: 1 } },
    { x: 32, y: 1, spread: { count: 3, mean: 1, min: 0.5, max: 2, standard_deviation: 2 } },
    { x: 64, y: 4, spread: { count: 3, mean: 4, min: 2, max: 6, standard_deviation: 1 } }
  ]);
  const result = await render(chart, { show_range: true, y_scale: 'log' });
  assert.deepEqual(result.traces[0].y, [null, null, 1.25, 4]);
  assert.deepEqual(result.traces[1].y, [null, null, null, 4]);
  assert.deepEqual(result.traces[2].y, [1, null, 1, 4]);
  assert.equal(chart.series[0].points[2].spread.standard_deviation, 2);
});
test('logarithmic omissions preserve positions and do not mutate analysis values', async () => {
  const chart = spec([{x:8,y:-2,row:'a',hover:'<script>bad</script>'},{x:16,y:4,row:'b',hover:'Valid'}]);
  const result = await render(chart, { y_scale: 'log', y_min: 1, y_max: 100 });
  assert.deepEqual(result.traces[0].y, [null,4]);
  assert.equal(chart.series[0].points[0].y, -2);
  assert.deepEqual(result.layout.yaxis.range, [0,2]);
  assert.equal(result.traces[0].customdata[0][1], '&lt;script&gt;bad&lt;/script&gt;');
});

test('scale changes reset the Y view without resetting commit zoom or changing measured values', async () => {
  const chart = spec([
    { x: 8, y: 0.6, row: 'a', spread: { count: 5, mean: 0.6, min: 0.5, max: 0.8, standard_deviation: 0.1 } },
    { x: 16, y: 15, row: 'b', spread: { count: 5, mean: 15, min: 14.8, max: 15.2, standard_deviation: 0.1 } }
  ]);
  const linear = await render(chart, { y_scale: 'linear', show_range: true });
  const log = await render(chart, { y_scale: 'log', show_range: true });
  assert.notEqual(linear.layout.yaxis.uirevision, log.layout.yaxis.uirevision);
  assert.equal(linear.layout.uirevision, log.layout.uirevision);
  assert.equal(log.layout.yaxis.autorange, true);
  assert.equal(log.layout.yaxis.range, undefined);
  for (let i = 0; i < linear.traces.length; i++) {
    assert.deepEqual(log.traces[i].y, linear.traces[i].y);
    assert.deepEqual(log.traces[i].error_y, linear.traces[i].error_y);
  }
  const linearAgain = await render(chart, { y_scale: 'linear' });
  assert.notEqual(log.layout.yaxis.uirevision, linearAgain.layout.yaxis.uirevision);
});

test('manual log bounds are transformed once and clearing them restores automatic bounds', async () => {
  const chart = spec([{ x: 8, y: 2, row: 'a' }]);
  const bounded = await render(chart, { y_scale: 'log', y_min: 0.1, y_max: 100 });
  assert.deepEqual(bounded.layout.yaxis.range, [-1, 2]);
  assert.equal(bounded.layout.yaxis.autorange, false);
  const upper = await render(chart, { y_scale: 'log', y_max: 100 });
  assert.deepEqual(upper.layout.yaxis.range, [null, 2]);
  assert.equal(upper.layout.yaxis.autorange, 'min');
  const lower = await render(chart, { y_scale: 'log', y_min: 0.1 });
  assert.deepEqual(lower.layout.yaxis.range, [-1, null]);
  assert.equal(lower.layout.yaxis.autorange, 'max');
  const automatic = await render(chart, { y_scale: 'log' });
  assert.notEqual(bounded.layout.yaxis.uirevision, automatic.layout.yaxis.uirevision);
  assert.equal(automatic.layout.yaxis.autorange, true);
  assert.equal(automatic.layout.yaxis.range, undefined);
});
test('coverage aggregate bars do not invent an inspection record', async () => {
  const chart = spec([{x:'revision',y:3,row:null,hover:'Measured'}]);
  chart.x_type = 'category'; chart.series[0].kind = 'bar'; chart.barmode = 'stack';
  const result = await render(chart, {show_spread:false}, [null,'Measured']);
  assert.equal(result.traces[0].type, 'bar');
  assert.equal(result.layout.barmode, 'stack');
  assert.deepEqual(result.messages, []);
  assert.equal(result.traces[0].error_y, undefined);
});
