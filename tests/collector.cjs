const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const html = fs.readFileSync('handwriting-collector.html', 'utf8');
const script = html.match(/<script>([\s\S]*?)<\/script>/)[1];
const elements = new Map();
const element = id => {
  if (!elements.has(id)) elements.set(id, {
    textContent: '', value: '', className: '', disabled: false,
    classList: { add() {} }, setAttribute() {}, handlers: {}, addEventListener(type, handler) { this.handlers[type] = handler; },
    getContext() { return new Proxy({}, { get: (_, key) => key === 'setTransform' ? () => {} : () => {} }); },
    getBoundingClientRect() { return {left: 0, top: 0, width: 800, height: 320}; }
  });
  return elements.get(id);
};
const storage = new Map();
const context = vm.createContext({
  document: { getElementById: element, addEventListener() {}, createElement: () => ({ click() {}, remove() {} }), body: { appendChild() {} } },
  window: { devicePixelRatio: 1, addEventListener() {}, confirm: () => true, localStorage: {
    setItem(k,v) { storage.set(k,v); }, getItem(k) { return storage.get(k); }, removeItem(k) { storage.delete(k); }
  } },
  Math, Map, Date, JSON, performance: { now: () => 0 }, console, setTimeout,
  URL: { createObjectURL: () => 'blob:test', revokeObjectURL() {} }, Blob,
});
vm.runInContext(script, context);
const run = expression => vm.runInContext(expression, context);
const original = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
run(`importData(${JSON.stringify(original)})`);
let migrated = JSON.parse(run('exportTextFor(exportData())'));
assert.equal(migrated.version, 2);
assert.equal(migrated.glyphs.filter(g => g.status === 'complete').length, 153);
for (const glyph of original.glyphs) {
  const converted = migrated.glyphs.find(g => g.key === glyph.key);
  assert(converted);
  if (glyph.status === 'complete') {
    assert.deepEqual(converted.variants[0].strokes, glyph.strokes);
    assert.deepEqual(converted.variants[0].bbox, glyph.bbox);
    assert.equal(converted.variants[0].baseline, glyph.baseline);
  }
}
run('goTo(0)');
element('variantAdd').handlers.click();
run('currentStrokes = [[{x:1,y:2,t:0,p:.5}, {x:30,y:90,t:1,p:.5}]]; saveCurrent()');
const second = JSON.parse(run('exportTextFor(exportData())')).glyphs[0];
assert.equal(second.variants.length, 2);
assert.deepEqual(second.variants[0].strokes, original.glyphs[0].strokes);
assert.notDeepEqual(second.variants[1].strokes, second.variants[0].strokes);
assert.match(element('storageStatus').textContent, /autosave/);
run('window.localStorage.setItem = () => { throw Error("blocked") }; initializeStorage(); downloadJson()');
assert.match(element('storageStatus').textContent, /unavailable/);
assert(JSON.parse(element('exportText').value).glyphs[0].variants.length === 2);
console.log('v1 -> v2: all 153 glyphs, original strokes, bounds, baselines preserved');
