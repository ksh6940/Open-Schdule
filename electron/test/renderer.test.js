const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');

function loadRenderer() {
  const handlers = {};
  const elements = {
    'connection-status-pill': {
      addEventListener: (event, handler) => { handlers[event] = handler; },
    },
  };
  let quitCount = 0;
  const context = {
    document: {
      addEventListener() {},
      getElementById: id => elements[id] || null,
    },
    window: { electronAPI: { quitApplication: () => { quitCount++; } } },
    console,
    setTimeout,
    clearTimeout,
  };
  vm.createContext(context);
  vm.runInContext(fs.readFileSync(path.join(__dirname, '..', 'renderer', 'js', 'app.js'), 'utf8'), context);
  return { context, handlers, getQuitCount: () => quitCount };
}

test('connection status button requests a complete shutdown', () => {
  const renderer = loadRenderer();
  vm.runInContext('setupWindowControls()', renderer.context);
  renderer.handlers.click();
  assert.equal(renderer.getQuitCount(), 1);
});

test('concurrent refreshes are coalesced without dropping a later change', async () => {
  const { context } = loadRenderer();
  let resolveFirst;
  let calls = 0;
  context.testRefresh = () => {
    calls++;
    return calls === 1 ? new Promise(resolve => { resolveFirst = resolve; }) : Promise.resolve();
  };
  vm.runInContext('performRefresh = testRefresh', context);
  const first = vm.runInContext('refreshAll()', context);
  const second = vm.runInContext('refreshAll()', context);
  const third = vm.runInContext('refreshAll()', context);
  assert.equal(calls, 1);
  resolveFirst();
  await Promise.all([first, second, third]);
  assert.equal(calls, 2);
});
