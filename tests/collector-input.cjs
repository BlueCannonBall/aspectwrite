const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');

const html = fs.readFileSync('handwriting-collector.html', 'utf8');
const script = html.match(/<script>([\s\S]*?)<\/script>/)[1];
const elements = new Map();
const captures = new Set();
const element = id => {
  if (!elements.has(id)) elements.set(id, {
    checked: true, textContent: '', value: '', className: '', disabled: false,
    classList: { add() {} }, setAttribute() {}, handlers: {}, addEventListener(type, handler) { this.handlers[type] = handler; },
    getContext() { return new Proxy({}, { get: () => () => {} }); },
    getBoundingClientRect() { return { left: 0, top: 0, width: 800, height: 320 }; },
    setPointerCapture(id) { captures.add(id); },
    hasPointerCapture(id) { return captures.has(id); },
    releasePointerCapture(id) { captures.delete(id); }
  });
  return elements.get(id);
};
const storage = new Map();
const context = vm.createContext({
  document: { getElementById: element, addEventListener() {} },
  window: { devicePixelRatio: 1, addEventListener() {}, localStorage: {
    setItem(k, v) { storage.set(k, v); }, getItem(k) { return storage.get(k); }, removeItem(k) { storage.delete(k); }
  } },
  Math, Map, Date, JSON, performance: { now: () => 1000 }, console
});
vm.runInContext(script, context);
const run = expression => vm.runInContext(expression, context);
assert.equal(run('itemByKey.has("(") && itemByKey.has(")")'), true);
const send = (type, pointerId, pointerType, x, timeStamp, extra = {}) =>
  element('drawingCanvas').handlers[type]({ pointerId, pointerType, button: 0, clientX: x, clientY: 50, timeStamp,
    pressure: .7, preventDefault() {}, ...extra });

element('allowTouchDrawing').checked = false;
element('allowTouchDrawing').handlers.change();
assert.equal(JSON.parse(storage.get('aspectwrite.handwriting-session.v1')).allowTouchDrawing, false);
send('pointerdown', 1, 'touch', 1, 1000);
assert.equal(run('activeStroke'), null);
send('pointerdown', 2, 'pen', 10, 1000);
send('pointermove', 1, 'touch', 20, 1005);
send('pointerdown', 1, 'touch', 20, 1005);
send('pointermove', 2, 'pen', 40, 1020, {
  getCoalescedEvents: () => [
    { clientX: 20, clientY: 50, timeStamp: 1005, pressure: .4 },
    { clientX: 30, clientY: 50, timeStamp: 1010, pressure: .6 },
    { clientX: 40, clientY: 50, timeStamp: 1020, pressure: .8 }
  ]
});
send('pointerup', 1, 'touch', 20, 1021);
assert.equal(run('activeStroke.length'), 4);
send('pointerup', 2, 'pen', 40, 1020);
assert.deepEqual(JSON.parse(run('JSON.stringify(currentStrokes[0].map(({x,t,p}) => ({x,t,p})))')), [
  { x: 10, t: 0, p: .7 }, { x: 20, t: 5, p: .4 },
  { x: 30, t: 10, p: .6 }, { x: 40, t: 20, p: .8 }
]);
assert.equal(captures.size, 0);

send('pointerdown', 3, 'mouse', 50, 1100);
send('pointermove', 3, 'mouse', 60, 1110, { getCoalescedEvents: () => [] });
send('pointermove', 3, 'mouse', 70, 1120);
send('pointerup', 3, 'mouse', 70, 1120);
assert.deepEqual(JSON.parse(run('JSON.stringify(currentStrokes[1].map(point => point.x))')), [50, 60, 70]);

element('allowTouchDrawing').checked = true;
element('allowTouchDrawing').handlers.change();
send('pointerdown', 4, 'touch', 80, 1200);
assert.equal(run('activePointerId'), 4);
element('allowTouchDrawing').checked = false;
element('allowTouchDrawing').handlers.change();
assert.equal(run('activeStroke'), null);
assert.equal(captures.size, 0);
assert.equal(run('currentStrokes.length'), 2);
console.log('touch toggle, pointer ownership, coalesced points, and fallback verified');
