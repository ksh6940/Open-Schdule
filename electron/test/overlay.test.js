const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');

test('overlay renders only when the data changes', async () => {
  let title = 'first';
  let renders = 0;
  const context = {
    document: { addEventListener() {} },
    api: {
      getOverlayData: async () => ({
        todos: [{ id: 1, title }],
        categories: [{ id: 1, name: 'work' }],
      }),
    },
    console,
    countRender: () => { renders++; },
  };
  vm.createContext(context);
  vm.runInContext(fs.readFileSync(path.join(__dirname, '..', 'renderer', 'js', 'overlay.js'), 'utf8'), context);
  vm.runInContext('renderOverlayList = countRender; renderProgressPill = () => {}', context);
  await vm.runInContext('loadOverlayData()', context);
  await vm.runInContext('loadOverlayData()', context);
  assert.equal(renders, 1);
  title = 'updated';
  await vm.runInContext('loadOverlayData()', context);
  assert.equal(renders, 2);
});

test('overlay falls back to existing endpoints only when the combined endpoint is unavailable', async () => {
  let combinedCalls = 0;
  const context = { window: {}, console, URLSearchParams };
  vm.createContext(context);
  vm.runInContext(fs.readFileSync(path.join(__dirname, '..', 'renderer', 'js', 'api.js'), 'utf8'), context);
  const api = context.window.api;
  api.request = async () => {
    combinedCalls++;
    const error = new Error('not found');
    error.status = 404;
    throw error;
  };
  api.getTodos = async () => [{ id: 1 }];
  api.getCategories = async () => [{ id: 2 }];
  const data = await api.getOverlayData();
  assert.equal(data.todos[0].id, 1);
  assert.equal(data.categories[0].id, 2);
  await api.getOverlayData();
  assert.equal(combinedCalls, 1);

  api.overlayEndpointAvailable = true;
  api.request = async () => { throw new Error('offline'); };
  await assert.rejects(api.getOverlayData(), /offline/);
});

test('duplicate status clicks are coalesced and the confirmed result renders immediately', async () => {
  let resolveStatus;
  let calls = 0;
  let renders = 0;
  const context = {
    document: { addEventListener() {}, getElementById() { return null; } },
    api: {
      toggleStatus: () => {
        calls++;
        return new Promise(resolve => { resolveStatus = resolve; });
      },
    },
    console,
    setTimeout() { return 1; },
    clearTimeout() {},
    countRender: () => { renders++; },
  };
  vm.createContext(context);
  vm.runInContext(fs.readFileSync(path.join(__dirname, '..', 'renderer', 'js', 'overlay.js'), 'utf8'), context);
  vm.runInContext("renderOverlayList = countRender; renderProgressPill = () => {}; overlayTodos = [{id: 1, status: 'planned', due_date: getLocalDateStr()}]", context);
  const first = vm.runInContext("updateOverlayStatus(overlayTodos[0], 'done')", context);
  const second = vm.runInContext("updateOverlayStatus(overlayTodos[0], 'done')", context);
  assert.equal(calls, 1);
  const today = vm.runInContext('getLocalDateStr()', context);
  resolveStatus({ id: 1, status: 'done', due_date: today, due_time: null });
  await Promise.all([first, second]);
  assert.equal(renders, 1);
  assert.equal(vm.runInContext('overlayTodos[0].status', context), 'done');
});

test('dashboard fallback preserves existing engine compatibility', async () => {
  let combinedCalls = 0;
  const context = { window: {}, console, URLSearchParams };
  vm.createContext(context);
  vm.runInContext(fs.readFileSync(path.join(__dirname, '..', 'renderer', 'js', 'api.js'), 'utf8'), context);
  const api = context.window.api;
  api.request = async () => {
    combinedCalls++;
    const error = new Error('not found');
    error.status = 404;
    throw error;
  };
  api.getSmartCounts = async () => ({ today: 1 });
  api.getCategoryCounts = async () => ({});
  api.getCategories = async () => [];
  api.getTodos = async () => [{ id: 1 }];
  const data = await api.getDashboardData();
  assert.equal(data.smart_counts.today, 1);
  assert.equal(data.todos.length, 1);
  await api.getDashboardData();
  assert.equal(combinedCalls, 1);
});

test('read retries after agent changes but mutations are never retried', async () => {
  let info = { port: 1, token: 'old' };
  let calls = [];
  const response = { ok: true, headers: { get: () => 'application/json' }, json: async () => ({ ok: true }) };
  const context = {
    window: { electronAPI: { getAgentInfo: () => info } }, console, URLSearchParams, setTimeout,
    fetch: async url => {
      calls.push(url);
      if (calls.length === 1) { info = { port: 2, token: 'new' }; throw new Error('offline'); }
      return response;
    },
  };
  vm.createContext(context);
  vm.runInContext(fs.readFileSync(path.join(__dirname, '..', 'renderer', 'js', 'api.js'), 'utf8'), context);
  const api = context.window.api;
  api.overlayEndpointAvailable = false;
  assert.equal((await api.request('/api/overlay')).ok, true);
  assert.deepEqual(calls, ['http://127.0.0.1:1/api/overlay', 'http://127.0.0.1:2/api/overlay']);
  assert.equal(api.overlayEndpointAvailable, true);
  calls = [];
  context.fetch = async url => { calls.push(url); info = { port: 3, token: 'next' }; throw new Error('offline'); };
  await assert.rejects(api.request('/api/todos', { method: 'POST' }), /offline/);
  assert.equal(calls.length, 1);
});
