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

test('settings and AI navigation return home and do not alter the gear icon', () => {
  const { context } = loadRenderer();
  const handlers = {};
  const pressed = {};
  let focused;
  const elements = {
    'btn-toggle-settings': {
      addEventListener: (_, fn) => { handlers.settings = fn; },
      setAttribute: (_, value) => { pressed.settings = value; },
      focus() { focused = 'settings'; },
    },
    'btn-ai': {
      addEventListener: (_, fn) => { handlers.ai = fn; },
      setAttribute: (_, value) => { pressed.ai = value; },
      focus() { focused = 'ai'; },
    },
    'btn-settings-back': { hidden: true, addEventListener: (_, fn) => { handlers.back = fn; } },
    'nav-brand-name': { hidden: false },
    'tab-reminders': { style: { display: 'flex' } },
    'tab-settings': { style: { display: 'none' } },
    'tab-ai': { style: { display: 'none' } },
  };
  context.document.getElementById = id => elements[id] || null;
  vm.runInContext('setupNavigation()', context);
  handlers.settings();
  assert.equal(elements['tab-reminders'].style.display, 'none');
  assert.equal(elements['btn-settings-back'].hidden, false);
  assert.equal(elements['nav-brand-name'].hidden, true);
  handlers.back();
  assert.equal(elements['tab-reminders'].style.display, 'flex');
  assert.equal(elements['tab-settings'].style.display, 'none');
  assert.equal(elements['nav-brand-name'].hidden, false);
  assert.equal(focused, 'settings');
  handlers.ai();
  assert.equal(elements['tab-ai'].style.display, 'flex');
  assert.equal(elements['tab-reminders'].style.display, 'none');
  assert.equal(elements['btn-settings-back'].hidden, false);
  assert.equal(pressed.ai, 'true');
  assert.equal(pressed.settings, 'false');
  handlers.settings();
  assert.equal(elements['tab-ai'].style.display, 'none');
  assert.equal(elements['tab-settings'].style.display, 'flex');
  handlers.ai();
  handlers.back();
  assert.equal(elements['tab-ai'].style.display, 'none');
  assert.equal(elements['tab-reminders'].style.display, 'flex');
  assert.equal(focused, 'ai');
  handlers.ai();
  handlers.ai();
  assert.equal(elements['tab-reminders'].style.display, 'flex');
  assert.equal(pressed.ai, 'false');
});

test('home shortcuts do not act while AI or settings is open', () => {
  const { context } = loadRenderer();
  let handler;
  let switched = 0;
  const home = { style: { display: 'none' } };
  context.document.getElementById = id => id === 'tab-reminders' ? home : null;
  context.document.addEventListener = (_, fn) => { handler = fn; };
  context.switchTest = () => { switched++; };
  vm.runInContext('switchViewMode = switchTest; setupGlobalShortcuts()', context);
  const event = { ctrlKey: true, key: '2', preventDefault() {} };
  handler(event);
  assert.equal(switched, 0);
  home.style.display = 'flex';
  handler(event);
  assert.equal(switched, 1);
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

test('unchanged refreshes preserve interactive list, category and calendar nodes', async () => {
  const { context } = loadRenderer();
  let title = 'first';
  let categoryName = 'work';
  let updatedAt = 'first';
  const calls = { list: 0, categories: 0, selects: 0, grid: 0, panel: 0 };
  context.api = {
    getDashboardData: async () => ({
      smart_counts: {}, category_counts: { 1: 1 },
      categories: [{ id: 1, name: categoryName }],
      todos: [{ id: 1, title, updated_at: updatedAt }],
      filtered_todos: [{ id: 1, title, updated_at: updatedAt }],
    }),
  };
  context.counts = calls;
  vm.runInContext(`
    state.viewMode = 'calendar';
    renderSmartCounts = () => {};
    renderCategories = () => counts.categories++;
    updateCategorySelect = () => counts.selects++;
    renderTodoList = () => counts.list++;
    renderCalendarGrid = () => counts.grid++;
    renderCalendarDayPanel = () => counts.panel++;
  `, context);
  await vm.runInContext('performRefresh()', context);
  updatedAt = 'second';
  await vm.runInContext('performRefresh()', context);
  assert.deepEqual(calls, { list: 1, categories: 1, selects: 1, grid: 1, panel: 1 });
  title = 'updated';
  await vm.runInContext('performRefresh()', context);
  assert.deepEqual(calls, { list: 2, categories: 1, selects: 1, grid: 2, panel: 2 });
  categoryName = 'renamed';
  await vm.runInContext('performRefresh()', context);
  assert.deepEqual(calls, { list: 3, categories: 2, selects: 2, grid: 3, panel: 3 });
});

test('one category click selects it and leaves calendar mode', () => {
  const { context } = loadRenderer();
  const handlers = {};
  const item = {
    classList: { add() {} },
    addEventListener(name, callback) { handlers[name] = callback; },
  };
  const container = { innerHTML: '', appendChild(child) { assert.equal(child, item); } };
  context.document.getElementById = id => id === 'category-lists' ? container : null;
  context.document.querySelectorAll = () => [];
  context.document.createElement = tag => { assert.equal(tag, 'button'); return item; };
  let switches = 0;
  context.switchTest = mode => { assert.equal(mode, 'list'); switches++; };
  vm.runInContext("state.viewMode = 'calendar'; state.categories = [{id: 7, name: 'work', color: 0}]; switchViewMode = switchTest; renderCategories()", context);
  handlers.click();
  assert.equal(vm.runInContext('state.selectedCategoryId', context), 7);
  assert.equal(switches, 1);
});

test('repeated list fetches do not replace unchanged rows', async () => {
  const { context } = loadRenderer();
  let title = 'first';
  let renders = 0;
  context.api = { getTodos: async () => [{ id: 1, title }] };
  context.renderTest = () => { renders++; };
  vm.runInContext('renderTodoList = renderTest; hasDashboardSnapshot = true', context);
  await vm.runInContext('loadTodos()', context);
  await vm.runInContext('loadTodos()', context);
  assert.equal(renders, 1);
  title = 'updated';
  await vm.runInContext('loadTodos()', context);
  assert.equal(renders, 2);
});

test('opening a schedule shows read-only details and editing is explicit', () => {
  const { context } = loadRenderer();
  const elements = {};
  const element = id => elements[id] || (elements[id] = {
    hidden: false, textContent: '', value: '', style: {}, children: [],
    classList: {
      hidden: true,
      contains(name) { return name === 'hidden' && this.hidden; },
      remove(name) { if (name === 'hidden') this.hidden = false; },
    },
    setAttribute() {}, removeAttribute() {}, focus() { this.focused = true; },
    replaceChildren() { this.children = []; },
    appendChild(child) { this.children.push(child); },
  });
  context.document.getElementById = element;
  context.document.querySelectorAll = () => [];
  context.document.createElement = () => ({ textContent: '' });
  context.document.activeElement = { isConnected: true };
  context.todo = {
    id: 1, title: '<title>', due_date: '2030-01-01', end_date: '2030-01-03',
    due_time: '09:30:00', reminder_at: null, category_id: 1, status: 'planned',
    priority: 2, memo: 'memo', condition_note: '',
    subtasks: [{ title: 'step', done: true }],
  };
  vm.runInContext("state.categories = [{id: 1, name: 'work'}]; openInspector(todo)", context);
  assert.equal(element('inspector-details').hidden, false);
  assert.equal(element('detail-title').textContent, '<title>');
  assert.equal(element('detail-date').textContent, '2030-01-01 ~ 2030-01-03');
  assert.equal(element('detail-category').textContent, 'work');
  assert.equal(element('detail-subtasks').children[0].textContent, '☑ step');
  assert.equal(element('btn-close-inspector').focused, true);
  assert.equal(element('inspector-panel').scrollTop, 0);
});

test('calendar selection opens the panel and double click opens the add modal', () => {
  const { context } = loadRenderer();
  const events = {};
  const classes = () => ({
    values: new Set(['hidden']),
    add(name) { this.values.add(name); },
    remove(name) { this.values.delete(name); },
    contains(name) { return this.values.has(name); },
  });
  let modalOpens = 0;
  const elements = {
    'calendar-days-grid': {
      addEventListener: (name, fn) => { events[name] = fn; },
      setPointerCapture() {},
    },
    'calendar-day-panel': { classList: classes() },
    'inspector-panel': { classList: classes() },
    'btn-close-calendar-panel': { addEventListener() {} },
    'btn-calendar-add-details': {
      addEventListener: (_, fn) => { events.details = fn; },
      click() { events.details(); },
    },
    'btn-open-new-todo-modal': { click() { modalOpens++; } },
  };
  context.document.getElementById = id => elements[id] || null;
  vm.runInContext('renderCalendarGrid = () => {}; renderCalendarDayPanel = () => {}; updateViewTitle = () => {}; loadTodos = () => {}; setupCalendarSelection()', context);
  const cell = { dataset: { calendarDate: '2026-11-12' } };
  const click = (time, date = cell.dataset.calendarDate) => {
    const targetCell = { dataset: { calendarDate: date } };
    events.pointerdown({
      button: 0, pointerId: 1, clientX: 20, clientY: 30, preventDefault() {},
      target: { closest: selector => selector === '[data-calendar-date]' ? targetCell : null },
    });
    events.pointerup({ pointerId: 1, timeStamp: time });
  };
  click(100);
  assert.equal(elements['calendar-day-panel'].classList.contains('hidden'), false);
  assert.equal(vm.runInContext('state.selectedCalendarDate', context), '2026-11-12');
  assert.equal(modalOpens, 0);
  click(200);
  assert.equal(modalOpens, 1);
  click(1000);
  click(1600);
  click(1700, '2026-11-13');
  assert.equal(modalOpens, 1);
  events.details();
  assert.equal(modalOpens, 2);
});

test('calendar shows at most two events without an overflow indicator', () => {
  const { context } = loadRenderer();
  function element() {
    const node = {
      children: [], dataset: {}, style: { setProperty() {} }, listeners: {},
      appendChild(child) { this.children.push(child); },
      setAttribute() {},
      addEventListener(name, fn) { this.listeners[name] = fn; },
      querySelector() { return this.preview; },
      set innerHTML(_) { this.children = []; this.preview = element(); },
    };
    return node;
  }
  const grid = element();
  context.document.getElementById = id => id === 'calendar-days-grid' ? grid : null;
  context.document.createElement = element;
  vm.runInContext("updateViewTitle = () => {}; state.calendarYear = 2026; state.calendarMonth = 11", context);
  for (const count of [0, 1, 2, 3, 5]) {
    context.todos = Array.from({ length: count }, (_, id) => ({
      id, title: `event ${id}`, due_date: '2026-11-12', status: 'planned',
    }));
    vm.runInContext('state.allTodosCache = todos; renderCalendarGrid()', context);
    const cell = grid.children.find(day => day.dataset.calendarDate === '2026-11-12');
    assert.equal(cell.preview.children.filter(child => child.dataset.todoId !== undefined).length, Math.min(count, 2));
    assert.equal(cell.preview.children.length, Math.min(count, 2));
  }
});

test('calendar header follows month navigation without changing list title', () => {
  const { context } = loadRenderer();
  const handlers = {};
  const title = { textContent: '', style: {} };
  const elements = {
    'view-title': title,
    'btn-cal-prev': { addEventListener: (_, fn) => { handlers.prev = fn; } },
    'btn-cal-next': { addEventListener: (_, fn) => { handlers.next = fn; } },
  };
  context.document.getElementById = id => elements[id] || null;
  vm.runInContext("state.viewMode = 'calendar'; state.calendarYear = 2026; state.calendarMonth = 1; renderCalendarGrid = updateViewTitle; setupCalendarControls(); updateViewTitle()", context);
  assert.equal(title.textContent, '2026년 1월');
  handlers.prev();
  assert.equal(title.textContent, '2025년 12월');
  handlers.next();
  assert.equal(title.textContent, '2026년 1월');
  vm.runInContext("state.viewMode = 'list'; state.currentFilter = 'today'; updateViewTitle()", context);
  assert.equal(title.textContent, '오늘');
});

test('calendar panel dates use Korean month, day and weekday', () => {
  const { context } = loadRenderer();
  assert.equal(vm.runInContext("formatCalendarDate('2026-11-12')", context), '11월 12일 (목)');
  assert.equal(vm.runInContext("formatCalendarDate('2027-01-01')", context), '1월 1일 (금)');
});

test('new todo picker, inline details and priority save the selected values', async () => {
  const { context } = loadRenderer();
  function node() {
    const attrs = {};
    return {
      value: '', textContent: '', hidden: false, disabled: false, children: [], dataset: {}, listeners: {}, style: {}, scrollHeight: 22,
      classList: {
        values: new Set(),
        add(name) { this.values.add(name); },
        remove(name) { this.values.delete(name); },
        contains(name) { return this.values.has(name); },
        toggle() {},
      },
      setAttribute(name, value) { attrs[name] = value; },
      getAttribute(name) { return attrs[name]; },
      addEventListener(name, fn) { this.listeners[name] = fn; },
      click() { if (this.listeners.click) return this.listeners.click({ target: this }); },
      focus() {},
      appendChild(child) { this.children.push(child); },
      replaceChildren() { this.children = []; },
    };
  }
  const elements = {};
  const element = id => elements[id] || (elements[id] = node());
  const modal = element('modal-add-todo');
  modal.classList.add('hidden');
  const tools = ['date', 'time', 'memo', 'subtask', 'priority'].map(name => {
    const button = node();
    button.setAttribute('aria-controls', `modal-${name}-details`);
    return button;
  });
  const modes = ['single', 'range'].map(mode => {
    const button = node();
    button.dataset.dateMode = mode;
    return button;
  });
  modal.querySelectorAll = selector => selector === '.todo-tool-button' ? tools : modes;
  modal.querySelector = selector => tools.find(tool => selector.includes(tool.getAttribute('aria-controls')));
  context.document.getElementById = element;
  context.document.querySelectorAll = () => [];
  context.document.createElement = node;
  context.document.activeElement = { isConnected: true, focus() {} };
  let saved;
  let updated;
  context.api = {
    requirePrioritySupport: async () => {},
    requireTodoDetailsSupport: async () => {},
    createTodo: async draft => { saved = draft; },
    updateTodo: async (id, draft) => { updated = { id, draft }; },
  };
  vm.runInContext(`
    getLocalDateStr = () => '2026-11-11';
    createSubtaskEditor = () => ({ set() {}, get: () => [{ title: 'step', done: false }] });
    refreshAll = async () => {};
    showToast = () => {};
    setupNewTodoModal();
  `, context);
  element('btn-open-new-todo-modal').click();
  assert.equal(element('modal-todo-date').value, '');
  assert.equal(element('modal-date-summary').hidden, true);
  tools[0].click();
  const day = number => element('modal-calendar-grid').children.find(child => child.textContent === String(number));
  assert.equal(element('modal-calendar-grid').children.length, 36);
  day(20).click();
  assert.equal(element('modal-date-summary').textContent, '11월 20일 (금)');
  modes[1].click();
  day(15).click();
  assert.equal(element('modal-todo-date').value, '2026-11-15');
  assert.equal(element('modal-todo-end-date').value, '2026-11-20');
  element('modal-date-clear').click();
  assert.equal(element('modal-date-summary').hidden, true);
  element('modal-date-done').click();
  assert.equal(element('modal-date-details').hidden, true);
  tools[1].click();
  assert.equal(element('modal-time-details').hidden, false);
  assert.equal(element('modal-date-details').hidden, true);
  element('modal-todo-time').value = '14:30';
  element('modal-todo-time').listeners.input();
  assert.equal(element('modal-time-summary').textContent, '14:30');
  assert.equal(element('modal-time-summary').hidden, false);
  tools[2].click();
  tools[3].click();
  tools[4].click();
  assert.equal(element('modal-memo-details').hidden, false);
  assert.equal(element('modal-subtask-details').hidden, false);
  element('modal-todo-memo').value = 'memo';
  element('modal-todo-memo').scrollHeight = 44;
  element('modal-todo-memo').listeners.input();
  assert.equal(element('modal-todo-memo').style.height, '44px');
  element('modal-todo-memo').scrollHeight = 22;
  element('modal-todo-memo').listeners.input();
  assert.equal(element('modal-todo-memo').style.height, '22px');
  element('modal-todo-priority').value = '3';
  element('modal-todo-priority').listeners.input();
  assert.equal(element('modal-priority-value').textContent, '높음');
  element('modal-todo-title').value = 'new';
  await element('btn-confirm-todo-modal').click();
  assert.equal(saved.due_date, '2026-11-11');
  assert.equal(saved.end_date, null);
  assert.equal(saved.priority, 3);
  assert.equal(saved.due_time, '14:30');
  assert.equal(saved.reminder_at, undefined);
  assert.equal(saved.memo, 'memo');
  assert.equal(saved.subtasks[0].title, 'step');
  vm.runInContext("state.viewMode = 'calendar'; state.selectedCalendarDate = '2026-11-12'; state.calendarRangeEnd = '2026-11-14'", context);
  element('btn-open-new-todo-modal').click();
  assert.equal(element('modal-todo-date').value, '2026-11-12');
  assert.equal(element('modal-todo-end-date').value, '2026-11-14');
  element('btn-close-todo-modal').click();
  context.todo = {
    id: 8, title: 'old', due_date: '2026-11-12', end_date: '2026-11-14', due_time: '10:00:45',
    memo: ' memo ', subtasks: [{ title: 'step', done: true }], category_id: null,
    status: 'conditional_done', condition_note: 'condition', reminder_at: '2026-11-12T10:00:30', priority: 2,
  };
  vm.runInContext('openTodoEditor(todo)', context);
  assert.equal(element('modal-todo-title').value, 'old');
  assert.equal(element('modal-todo-heading').textContent, '일정 수정');
  element('modal-todo-title').value = 'changed';
  await element('btn-confirm-todo-modal').click();
  assert.equal(updated.id, 8);
  assert.equal(updated.draft.status, 'conditional_done');
  assert.equal(updated.draft.condition_note, 'condition');
  assert.equal(updated.draft.reminder_at, undefined);
  assert.equal(updated.draft.due_time, '10:00:45');
  assert.equal(updated.draft.memo, ' memo ');
  assert.equal(updated.draft.priority, 2);
});

test('subtask editor adds checkboxes and removes individual items', () => {
  const { context } = loadRenderer();
  const makeNode = () => ({
    children: [], listeners: {}, value: '', checked: false,
    setAttribute(name, value) { this[name] = value; },
    addEventListener(name, fn) { this.listeners[name] = fn; },
    append(...children) { this.children.push(...children); },
    appendChild(child) { this.children.push(child); },
    replaceChildren() { this.children = []; },
  });
  const list = makeNode();
  const input = makeNode();
  const add = makeNode();
  context.container = { querySelector: selector => ({
    '.subtask-list': list, '.subtask-input': input, '.subtask-add': add,
  })[selector] };
  context.document.createElement = makeNode;
  const editor = vm.runInContext('createSubtaskEditor(container)', context);
  input.value = 'first';
  add.listeners.click();
  input.value = 'second';
  add.listeners.click();
  const [checkbox, , remove] = list.children[0].children;
  assert.equal(checkbox.type, 'checkbox');
  checkbox.checked = true;
  checkbox.listeners.change();
  assert.equal(editor.get()[0].done, true);
  assert.equal(remove.children[0].className, 'ph ph-x');
  assert.equal(remove['aria-label'], 'first 삭제');
  remove.listeners.click();
  assert.equal(editor.get().length, 1);
  assert.equal(editor.get()[0].title, 'second');
});

test('todo swipe distinguishes clicks, vertical scroll, reveal, edit and delete', async () => {
  const { context } = loadRenderer();
  const listeners = {};
  const actions = {};
  const classes = new Set();
  let edited = 0;
  let deleted = 0;
  const row = {
    style: {},
    addEventListener(name, fn) { listeners[name] = fn; },
    setPointerCapture() {},
  };
  const wrapper = {
    clientWidth: 400,
    classList: {
      add(name) { classes.add(name); },
      remove(name) { classes.delete(name); },
      toggle(name, active) { if (active) classes.add(name); else classes.delete(name); },
    },
    querySelector(selector) {
      return { addEventListener: (_, fn) => { actions[selector] = fn; } };
    },
  };
  context.wrapper = wrapper;
  context.row = row;
  context.todo = { id: 1, title: 'task' };
  context.edit = () => { edited++; };
  context.remove = async () => { deleted++; };
  vm.runInContext('openTodoEditor = edit; deleteTodoFromList = remove; setupTodoSwipe(wrapper, row, todo)', context);
  const event = (x, y = 0) => ({
    pointerId: 1, button: 0, clientX: x, clientY: y, target: { closest: () => null },
    preventDefault() {}, stopImmediatePropagation() {},
  });
  listeners.pointerdown(event(200));
  listeners.pointermove(event(210, 40));
  listeners.pointerup(event(210, 40));
  assert.equal(row.style.transform, undefined);
  listeners.pointerdown(event(200));
  listeners.pointermove(event(120));
  listeners.pointerup(event(120));
  assert.equal(row.style.transform, 'translateX(-76px)');
  assert.equal(classes.has('swipe-left'), true);
  let blocked = false;
  listeners.click({ preventDefault() {}, stopImmediatePropagation() { blocked = true; } });
  assert.equal(blocked, true);
  await actions['.swipe-delete']();
  assert.equal(deleted, 1);
  listeners.pointerdown(event(100));
  listeners.pointermove(event(340));
  listeners.pointerup(event(340));
  assert.equal(edited, 1);
  listeners.pointerdown(event(300));
  listeners.pointermove(event(50));
  listeners.pointercancel(event(50));
  assert.equal(deleted, 1);
  assert.equal(row.style.transform, '');
  listeners.pointerdown(event(300));
  listeners.pointermove(event(50));
  listeners.pointerup(event(50));
  assert.equal(deleted, 2);
});

test('hotzone settings save all fields and notify main process', async () => {
  const listeners = {};
  const elements = {
    'setting-hotzone-enabled': { checked: false },
    'setting-hotzone-center': { value: '', reportValidity: () => true },
    'setting-hotzone-width': { value: '', reportValidity: () => true },
    'setting-hotzone-height': { value: '', reportValidity: () => true },
    'setting-hotzone-dwell': { value: '', reportValidity: () => true },
    'setting-hide-delay': { value: '', reportValidity: () => true },
    'setting-panel-width': { value: '', reportValidity: () => true },
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
  elements['setting-hide-delay'].value = '750';
  elements['setting-panel-width'].value = '500';
  await listeners.click();
  assert.equal(settings.hotzone_center_percent, 25);
  assert.equal(settings.hotzone_width, 100);
  assert.equal(settings.hide_delay_ms, 750);
  assert.equal(settings.panel_width, 500);
  assert.equal(reloads, 1);
});

test('settings reset requires confirmation and refreshes the applied settings', async () => {
  const { context } = loadRenderer();
  const handlers = {};
  const elements = {
    'setting-hotzone-enabled': { checked: false },
    'btn-save-hotzone': { addEventListener() {} },
    'btn-reset-settings': { addEventListener: (_, fn) => { handlers.reset = fn; } },
  };
  for (const id of ['setting-hotzone-center', 'setting-hotzone-width', 'setting-hotzone-height',
    'setting-hotzone-dwell', 'setting-hide-delay', 'setting-panel-width']) elements[id] = { value: '' };
  const defaults = {
    hotzone_enabled: true, hotzone_center_percent: 50, hotzone_width: 240,
    hotzone_height: 4, hotzone_dwell_ms: 150, hide_delay_ms: 400, panel_width: 480, autostart: false,
  };
  let settings = { ...defaults, hotzone_width: 350 };
  let accepted = false;
  let saves = 0;
  let reloads = 0;
  context.document.getElementById = id => elements[id] || null;
  context.window = { confirm: () => accepted, electronAPI: { reloadHotzoneSettings: () => { reloads++; } } };
  context.api = {
    getSettings: async () => ({ ...settings }),
    saveSettings: async value => { assert.equal(Object.keys(value).length, 0); saves++; settings = { ...defaults }; },
  };
  await vm.runInContext('setupSettings()', context);
  await handlers.reset();
  assert.equal(saves, 0);
  accepted = true;
  await handlers.reset();
  assert.equal(saves, 1);
  assert.equal(reloads, 1);
  assert.equal(elements['setting-hotzone-width'].value, 240);
});

test('memory display reports measurement errors without reporting success', async () => {
  const { context } = loadRenderer();
  const handlers = {};
  const elements = {
    'btn-save-hotzone': { addEventListener() {} },
    'btn-refresh-memory': { addEventListener: (_, fn) => { handlers.refresh = fn; } },
    'settings-memory-usage': { textContent: '' },
  };
  context.document.getElementById = id => elements[id] || null;
  context.api = { getSettings: async () => ({}) };
  context.window = { electronAPI: { getElectronMemory: async () => 2048 } };
  await vm.runInContext('setupSettings()', context);
  await handlers.refresh();
  assert.equal(elements['settings-memory-usage'].textContent, '2.0 MB');
  context.window.electronAPI.getElectronMemory = async () => { throw new Error('offline'); };
  await handlers.refresh();
  assert.equal(elements['settings-memory-usage'].textContent, '측정 실패');
});

test('HTTP cache cleanup requires user confirmation', async () => {
  const { context } = loadRenderer();
  const handlers = {};
  const elements = {
    'btn-save-hotzone': { addEventListener() {} },
    'btn-clear-http-cache': { addEventListener: (_, fn) => { handlers.clear = fn; } },
  };
  let accepted = false;
  let clears = 0;
  context.document.getElementById = id => elements[id] || null;
  context.api = { getSettings: async () => ({}) };
  context.window = { confirm: () => accepted, electronAPI: { clearHttpCache: async () => { clears++; } } };
  await vm.runInContext('setupSettings()', context);
  await handlers.clear();
  assert.equal(clears, 0);
  accepted = true;
  await handlers.clear();
  assert.equal(clears, 1);
  assert.equal(elements['btn-clear-http-cache'].disabled, false);
});
