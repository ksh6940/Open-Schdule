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
