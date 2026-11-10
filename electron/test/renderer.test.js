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

test('dashboard refresh uses filtered snapshot without a second request', async () => {
  const { context } = loadRenderer();
  let queries = 0;
  let rendered = 0;
  context.api = {
    getDashboardData: async params => {
      assert.equal(params.date, 'today_and_overdue');
      return { smart_counts: {}, category_counts: {}, categories: [], todos: [{ id: 1 }],
        filtered_todos: [{ id: 1 }] };
    },
    getTodos: async () => { queries++; return []; },
  };
  context.renderTest = () => { rendered++; };
  vm.runInContext('renderSmartCounts = () => {}; renderCategories = () => {}; updateCategorySelect = () => {}; renderTodoList = renderTest; updateScheduleFileIndicator = () => {}', context);
  await vm.runInContext('performRefresh()', context);
  assert.equal(queries, 0);
  assert.equal(rendered, 1);
  assert.equal(vm.runInContext('state.todos[0].id', context), 1);
});

test('hotzone settings save all fields and notify main process', async () => {
  const listeners = {};
  const elements = {
    'setting-hotzone-enabled': { checked: false },
    'setting-hotzone-center': { value: '', reportValidity: () => true },
    'setting-hotzone-width': { value: '', reportValidity: () => true },
    'setting-hotzone-height': { value: '', reportValidity: () => true },
    'setting-hotzone-dwell': { value: '', reportValidity: () => true },
    'btn-save-hotzone': { addEventListener: (event, fn) => { listeners[event] = fn; } },
  };
  let settings = { hotzone_enabled: true, hotzone_width: 240, hotzone_height: 4, hotzone_dwell_ms: 150 };
  let reloads = 0;
  const context = {
    document: { addEventListener() {}, getElementById: id => elements[id] || null },
    window: { electronAPI: { reloadHotzoneSettings: () => { reloads++; } } },
    api: {
      getSettings: async () => ({ ...settings }),
      saveSettings: async value => { settings = { ...value }; },
    },
    console, setTimeout, clearTimeout,
  };
  vm.createContext(context);
  vm.runInContext(fs.readFileSync(path.join(__dirname, '..', 'renderer', 'js', 'app.js'), 'utf8'), context);
  await vm.runInContext('setupSettings()', context);
  assert.equal(elements['setting-hotzone-center'].value, 50);
  elements['setting-hotzone-center'].value = '25';
  elements['setting-hotzone-width'].value = '100';
  await listeners.click();
  assert.equal(settings.hotzone_center_percent, 25);
  assert.equal(settings.hotzone_width, 100);
  assert.equal(reloads, 1);
});
