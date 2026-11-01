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
test('numeric X values and stable inspection IDs survive the V1 bridge', async () => {
  const result = await render(spec([{ x: 8, y: 2, sd: 0.1, row: 'record-a', hover: 'Input 8' }]), {}, ['record-a', 'Input 8']);
  assert.equal(result.layout.xaxis.type, 'linear');
  assert.deepEqual(result.traces[0].x, [8]);
  assert.deepEqual(result.messages, [{row_id: 'record-a'}]);
  assert.deepEqual(result.traces[0].error_y.array, [0.1]);
  assert.ok(result.purged);
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
