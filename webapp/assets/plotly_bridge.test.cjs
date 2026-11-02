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
  assert.match(line.customdata[0][1], /repetitions of one input within one run: n=3, mean=10, min=2, max=18, sample SD=3/);
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
  assert.match(variation.traces[1].customdata[0][1], /across inputs: n=3/);
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
test('coverage aggregate bars do not invent an inspection record', async () => {
  const chart = spec([{x:'revision',y:3,row:null,hover:'Measured'}]);
  chart.x_type = 'category'; chart.series[0].kind = 'bar'; chart.barmode = 'stack';
  const result = await render(chart, {show_spread:false}, [null,'Measured']);
  assert.equal(result.traces[0].type, 'bar');
  assert.equal(result.layout.barmode, 'stack');
  assert.deepEqual(result.messages, []);
  assert.equal(result.traces[0].error_y, undefined);
});
