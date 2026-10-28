const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
class Element {
  constructor() { this.listeners = {}; this.children = []; }
  append(...items) { this.children.push(...items); }
  setAttribute() {}
  addEventListener(name, action) { this.listeners[name] = action; }
  focus() {}
}
const context = { window: {}, document: { createElement: () => new Element() },
  setInterval: () => 1, clearInterval: () => {}, setTimeout, clearTimeout, performance };
vm.createContext(context);
vm.runInContext(fs.readFileSync(path.join(__dirname, '../platform-service/web/console.js'), 'utf8'), context);
const Console = context.window.PlatformConsole;
const deferred = () => { let resolve; const promise = new Promise(r => { resolve = r; }); return { promise, resolve }; };
function fixture(api) { return new Console({ id: 'channel-a', name: 'A', control: 'c' }, api, new Element(), () => {}); }
(async () => {
  // Pending acquire must not resurrect control after disconnect/switch.
  const acquire = deferred(); const calls = [];
  const c = fixture(async (url) => { calls.push(url); if (url.endsWith('/acquire')) await acquire.promise; return {}; });
  c.peer = { connectionState: 'connected', close() {} };
  const pending = c.acquire(); await c.close(); acquire.resolve(); await pending;
  assert.equal(c.control, false); assert.equal(c.peer, null);
  assert(calls.at(-1).endsWith('/control/release'));

  // Releasing while a packet is in flight drains that packet, skips subsequent
  // queued packets and sends the neutral release last.
  const delivery = deferred(); const entered = deferred(); const sent = [];
  const d = fixture(async (url, method, body) => {
    sent.push([url, body]); if (url.endsWith('/input')) { entered.resolve(); await delivery.promise; } return {};
  });
  d.control = true; d.send([1, 0, 4, 0]); await entered.promise;
  d.send([1, 1, 4, 0]); const release = d.release(); delivery.resolve(); await release;
  assert.equal(sent.length, 2); assert(sent[0][0].endsWith('/input')); assert(sent[1][0].endsWith('/release'));
  assert.equal(d.control, false); assert.equal(d.pending, 0);

  // Viewer key events must never reach the delivery queue.
  let prevented = false;
  d.key({ code: 'KeyA', preventDefault() { prevented = true; } }, 0);
  await d.queue; assert.equal(sent.length, 2); assert.equal(prevented, false);
  console.log('Console lifecycle: 3 tests passed');
})().catch(error => { console.error(error); process.exitCode = 1; });
