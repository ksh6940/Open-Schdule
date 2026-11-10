// Main Application State & Controller (Frameless Apple Design System v1.0)

const nowInit = new Date();

const state = {
  viewMode: 'list', // 'list' (투두리스트) | 'calendar' (캘린더 뷰)
  currentFilter: 'today', // 'today', 'scheduled', 'all', 'completed'
  selectedCategoryId: null,
  searchQuery: '',

  calendarYear: nowInit.getFullYear(),
  calendarMonth: nowInit.getMonth() + 1, // 1..12
  selectedCalendarDate: getLocalDateStr(0),
  calendarRangeEnd: null,
  allTodosCache: [],

  todos: [],
  categories: [],
  smartCounts: { today: 0, scheduled: 0, all: 0, completed: 0 },
  categoryCounts: {},

  selectedTodo: null,
  newCategoryColor: 0x0071E3,
};

let toastTimeout = null;
let searchDebounceTimeout = null;
let todoLoadRequestId = 0;
let refreshPromise = null;
let refreshPending = false;
let hasDashboardSnapshot = false;

function todoDisplayKey(todos) {
  return JSON.stringify(todos, (key, value) =>
    key === 'created_at' || key === 'updated_at' ? undefined : value);
}

function showToast(message) {
  const toast = document.getElementById('apple-toast');
  const msgEl = document.getElementById('apple-toast-msg');
  if (!toast || !msgEl) return;

  msgEl.textContent = message;
  toast.classList.remove('hidden');

  if (toastTimeout) clearTimeout(toastTimeout);
  toastTimeout = setTimeout(() => {
    toast.classList.add('hidden');
  }, 2400);
}

// 로컬 타임존 기준 YYYY-MM-DD 반환 (UTC 오차 방지)
function getLocalDateStr(offsetDays = 0) {
  const d = new Date();
  if (offsetDays !== 0) {
    d.setDate(d.getDate() + offsetDays);
  }
  const yyyy = d.getFullYear();
  const mm = String(d.getMonth() + 1).padStart(2, '0');
  const dd = String(d.getDate()).padStart(2, '0');
  return `${yyyy}-${mm}-${dd}`;
}

function addDaysToDateStr(dateStr, days) {
  const parts = (dateStr || getLocalDateStr(0)).split('-').map(Number);
  const d = new Date(parts[0], (parts[1] || 1) - 1, parts[2] || 1);
  d.setDate(d.getDate() + days);
  const yyyy = d.getFullYear();
  const mm = String(d.getMonth() + 1).padStart(2, '0');
  const dd = String(d.getDate()).padStart(2, '0');
  return `${yyyy}-${mm}-${dd}`;
}

// ── 초기화 ──
document.addEventListener('DOMContentLoaded', async () => {
  setupWindowControls();
  setupNavigation();
  setupSidebarToggle();
  setupViewModeSwitch();
  setupCalendarControls();
  setupCalendarSelection();
  setupSmartCards();
  setupNewTodoModal();
  setupInspector();
  setupCategoryModal();
  setupSettings();
  setupSyncButton();
  setupGlobalShortcuts();

  // 오늘 날짜 아이콘 번호 설정
  const todayNum = new Date().getDate();
  const dayIcon = document.getElementById('today-day-icon');
  if (dayIcon) dayIcon.textContent = todayNum;

  // IPC 이벤트 수신 (엔진 준비 완료 또는 CLI/외부 데이터 변경)
  if (window.electronAPI) {
    if (window.electronAPI.onAgentReady) {
      window.electronAPI.onAgentReady(async () => {
        api.init();
        await initEngineConnection();
      });
    }
    if (window.electronAPI.onRefreshData) {
      window.electronAPI.onRefreshData(() => refreshAll(false));
    }
  }

  // 엔진 연결 대기 및 데이터 로드
  await initEngineConnection();

});

function setupSidebarToggle() {
  const button = document.getElementById('btn-toggle-sidebar');
  const expandButton = document.getElementById('btn-expand-sidebar');
  if (!button || !expandButton) return;

  button.addEventListener('click', () => {
    setSidebarExpanded(false);
    expandButton.focus();
  });
  expandButton.addEventListener('click', () => {
    setSidebarExpanded(true);
    button.focus();
  });
}

function setSidebarExpanded(expanded) {
  const main = document.getElementById('tab-reminders');
  if (!main) return;
  main.classList.toggle('sidebar-collapsed', !expanded);
  document.querySelectorAll('[aria-controls="reminders-sidebar"]').forEach(button => {
    button.setAttribute('aria-expanded', String(expanded));
  });
}

// ── 프레임리스 윈도우 컨트롤 ──
function setupWindowControls() {
  const btnMin = document.getElementById('btn-win-min');
  const btnMax = document.getElementById('btn-win-max');
  const btnClose = document.getElementById('btn-win-close');
  const iconMax = document.getElementById('icon-win-max');
  const btnQuit = document.getElementById('connection-status-pill');

  if (btnQuit) {
    btnQuit.addEventListener('click', () => {
      if (window.electronAPI && window.electronAPI.quitApplication) {
        window.electronAPI.quitApplication();
      }
    });
  }

  if (btnMin) {
    btnMin.addEventListener('click', () => {
      if (window.electronAPI && window.electronAPI.minimizeWindow) {
        window.electronAPI.minimizeWindow();
      }
    });
  }

  if (btnMax) {
    btnMax.addEventListener('click', () => {
      if (window.electronAPI && window.electronAPI.maximizeWindow) {
        window.electronAPI.maximizeWindow();
      }
    });
  }

  if (btnClose) {
    btnClose.addEventListener('click', () => {
      if (window.electronAPI && window.electronAPI.closeWindow) {
        window.electronAPI.closeWindow();
      }
    });
  }

  if (window.electronAPI && window.electronAPI.onWindowMaximized && iconMax) {
    window.electronAPI.onWindowMaximized((isMax) => {
      iconMax.className = isMax ? 'ph-bold ph-corners-in' : 'ph-bold ph-corners-out';
    });
  }
}

// ── 전역 키보드 단축키 ──
function setupGlobalShortcuts() {
  document.addEventListener('keydown', (e) => {
    if (e.defaultPrevented) return;
    const activeTag = document.activeElement ? document.activeElement.tagName.toLowerCase() : '';
    const isTyping = activeTag === 'input' || activeTag === 'textarea' || activeTag === 'select';
    const home = document.getElementById('tab-reminders');
    if (home && home.style.display === 'none') return;

    if ((e.ctrlKey || e.metaKey) && e.key === '1') {
      e.preventDefault();
      switchViewMode('list');
    } else if ((e.ctrlKey || e.metaKey) && e.key === '2') {
      e.preventDefault();
      switchViewMode('calendar');
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
      e.preventDefault();
      const todoModal = document.getElementById('modal-add-todo');
      const inspector = document.getElementById('inspector-panel');
      if (todoModal && !todoModal.classList.contains('hidden')) {
        document.getElementById('btn-confirm-todo-modal').click();
      } else if (!inspector || inspector.classList.contains('hidden')) {
        document.getElementById('btn-sync-json').click();
      }
    } else if (!isTyping && e.key === '/') {
      e.preventDefault();
      const searchInput = document.getElementById('search-input');
      setSidebarExpanded(true);
      if (searchInput) searchInput.focus();
    } else if (e.key === 'Escape') {
      const todoModal = document.getElementById('modal-add-todo');
      if (todoModal && !todoModal.classList.contains('hidden')) return;
      const inspector = document.getElementById('inspector-panel');
      if (inspector && !inspector.classList.contains('hidden')) {
        document.getElementById('btn-close-inspector').click();
      } else {
        const calendarPanel = document.getElementById('calendar-day-panel');
        if (calendarPanel && !calendarPanel.classList.contains('hidden')) {
          document.getElementById('btn-close-calendar-panel').click();
        }
      }
    }
  });
}

async function initEngineConnection() {
  const pill = document.getElementById('connection-status-pill');
  if (pill) {
    pill.className = 'status-pill in_progress';
    pill.innerHTML = '<i class="ph-bold ph-spinner"></i> 연결 중...';
  }

  const ready = await api.ensureReady() && await api.checkHealth();
  if (ready) {
    if (pill) {
      pill.className = 'status-pill done';
      pill.innerHTML = '<i class="ph-fill ph-circle"></i> 연결됨';
    }
    await refreshAll(true);
    await loadSettings();
  } else {
    if (pill) {
      pill.className = 'status-pill planned';
      pill.innerHTML = '<i class="ph-bold ph-warning"></i> 대기 중';
    }
  }
}

async function refreshAll(full = true) {
  refreshPending = true;
  if (refreshPromise) return refreshPromise;

  refreshPromise = (async () => {
    while (refreshPending) {
      refreshPending = false;
      await performRefresh();
    }
  })();
  try {
    await refreshPromise;
  } finally {
    refreshPromise = null;
  }
}

async function performRefresh() {
  try {
    const requestId = ++todoLoadRequestId;
    const params = getTodoParams();
    const { smart_counts: smartCounts, category_counts: categoryCounts,
      categories, todos: allTodos, filtered_todos: filteredTodos } = await api.getDashboardData(params);
    const categoriesChanged = !hasDashboardSnapshot ||
      JSON.stringify(state.categories) !== JSON.stringify(categories || []);
    const countsChanged = !hasDashboardSnapshot ||
      JSON.stringify(state.categoryCounts) !== JSON.stringify(categoryCounts || {});
    const calendarChanged = !hasDashboardSnapshot ||
      todoDisplayKey(state.allTodosCache) !== todoDisplayKey(allTodos || []);

    const pill = document.getElementById('connection-status-pill');
    if (pill) {
      pill.className = 'status-pill done';
      pill.innerHTML = '<i class="ph-fill ph-circle"></i> 연결됨';
    }
    state.smartCounts = smartCounts || { today: 0, scheduled: 0, all: 0, completed: 0 };
    state.categoryCounts = categoryCounts || {};
    state.categories = categories || [];
    state.allTodosCache = allTodos || [];

    renderSmartCounts();
    if (categoriesChanged || countsChanged) renderCategories();
    if (categoriesChanged) updateCategorySelect();

    if (state.viewMode === 'calendar' && (calendarChanged || categoriesChanged)) {
      renderCalendarGrid();
      renderCalendarDayPanel();
    }

    if (requestId === todoLoadRequestId) {
      if (Array.isArray(filteredTodos)) {
        const todosChanged = !hasDashboardSnapshot ||
          todoDisplayKey(state.todos) !== todoDisplayKey(filteredTodos);
        state.todos = filteredTodos;
        if (todosChanged || categoriesChanged) renderTodoList();
      } else {
        await loadTodos(!hasDashboardSnapshot || categoriesChanged);
      }
    }
    hasDashboardSnapshot = true;
  } catch (err) {
    console.error('Failed to refresh data:', err);
    const pill = document.getElementById('connection-status-pill');
    if (pill) {
      pill.className = 'status-pill planned';
      pill.innerHTML = '<i class="ph-bold ph-warning"></i> 연결 오류';
    }
  }
}

function getTodoParams() {
  const params = {};

  if (state.viewMode === 'calendar') {
    params.date = state.selectedCalendarDate;
    if (state.selectedCategoryId != null) {
      params.category_id = state.selectedCategoryId;
    }
  } else {
    if (state.selectedCategoryId != null) {
      params.category_id = state.selectedCategoryId;
    } else {
      params.date = state.currentFilter === 'today' ? 'today_and_overdue' : state.currentFilter;
    }
  }

  if (state.searchQuery.trim()) {
    params.search = state.searchQuery.trim();
  }

  return params;
}

async function loadTodos(forceRender = false) {
  if (searchDebounceTimeout) {
    clearTimeout(searchDebounceTimeout);
    searchDebounceTimeout = null;
  }
  const requestId = ++todoLoadRequestId;
  const params = getTodoParams();

  try {
    const todos = await api.getTodos(params);
    if (requestId !== todoLoadRequestId) return;
    const nextTodos = todos || [];
    const changed = todoDisplayKey(state.todos) !== todoDisplayKey(nextTodos);
    state.todos = nextTodos;
    if (changed || forceRender || !hasDashboardSnapshot) renderTodoList();
  } catch (err) {
    if (requestId === todoLoadRequestId) {
      console.error('Failed to load todos:', err);
    }
  }
}

// ── [투두리스트 / 캘린더 뷰] 모드 전환 ──
function switchViewMode(mode) {
  state.viewMode = mode;
  document.getElementById('tab-reminders').classList.toggle('calendar-mode', mode === 'calendar');
  updateInspectorPresentation();
  const btns = document.querySelectorAll('#view-mode-switch .view-mode-btn');
  const calBox = document.getElementById('calendar-view-box');

  btns.forEach(b => b.classList.toggle('active', b.dataset.mode === mode));

  if (mode === 'calendar') {
    calBox.classList.remove('hidden');
    renderCalendarGrid();
  } else {
    calBox.classList.add('hidden');
    document.getElementById('calendar-day-panel').classList.add('hidden');
  }

  updateViewTitle();
  loadTodos();
}

function setupViewModeSwitch() {
  const btns = document.querySelectorAll('#view-mode-switch .view-mode-btn');
  btns.forEach(btn => {
    btn.addEventListener('click', () => {
      switchViewMode(btn.dataset.mode);
    });
  });
}

// ── 월간 캘린더 컨트롤 & 그리드 렌더링 ──
function setupCalendarControls() {
  const btnPrev = document.getElementById('btn-cal-prev');
  const btnNext = document.getElementById('btn-cal-next');

  if (btnPrev) {
    btnPrev.addEventListener('click', () => {
      if (state.calendarMonth === 1) {
        state.calendarYear -= 1;
        state.calendarMonth = 12;
      } else {
        state.calendarMonth -= 1;
      }
      renderCalendarGrid();
    });
  }

  if (btnNext) {
    btnNext.addEventListener('click', () => {
      if (state.calendarMonth === 12) {
        state.calendarYear += 1;
        state.calendarMonth = 1;
      } else {
        state.calendarMonth += 1;
      }
      renderCalendarGrid();
    });
  }

}

function formatCalendarDate(value) {
  const [year, month, day] = value.split('-').map(Number);
  const weekday = ['일', '월', '화', '수', '목', '금', '토'][new Date(year, month - 1, day).getDay()];
  return `${month}월 ${day}일 (${weekday})`;
}

function showCalendarDate(start, end = start) {
  const [first, last] = [start, end].sort();
  state.selectedCalendarDate = first;
  state.calendarRangeEnd = last;
  const [year, month] = first.split('-').map(Number);
  state.calendarYear = year;
  state.calendarMonth = month;
  document.getElementById('inspector-panel').classList.add('hidden');
  state.selectedTodo = null;
  document.getElementById('calendar-day-panel').classList.remove('hidden');
  renderCalendarGrid();
  renderCalendarDayPanel();
  updateViewTitle();
  loadTodos();
}

function renderCalendarDayPanel() {
  const panel = document.getElementById('calendar-day-panel');
  if (!panel || panel.classList.contains('hidden')) return;
  const start = state.selectedCalendarDate;
  const end = state.calendarRangeEnd || start;
  document.getElementById('calendar-panel-date').textContent = start === end
    ? formatCalendarDate(start) : `${formatCalendarDate(start)} ~ ${formatCalendarDate(end)}`;
  const list = document.getElementById('calendar-panel-list');
  list.replaceChildren();
  state.allTodosCache.filter(todo => todo.due_date <= end && (todo.end_date || todo.due_date) >= start).forEach(todo => {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'calendar-panel-item';
    button.textContent = `${todo.due_date === start && start === end ? '' : `${todo.due_date} · `}${todo.title}`;
    button.addEventListener('click', () => {
      panel.classList.add('hidden');
      openInspector(todo);
    });
    list.appendChild(button);
  });
}

function setupCalendarSelection() {
  const grid = document.getElementById('calendar-days-grid');
  const panel = document.getElementById('calendar-day-panel');
  let drag = null;
  let lastClick = null;

  grid.addEventListener('pointerdown', event => {
    if (event.button !== 0 || event.target.closest('[data-todo-id]')) {
      lastClick = null;
      return;
    }
    const cell = event.target.closest('[data-calendar-date]');
    if (!cell) return;
    drag = { start: cell.dataset.calendarDate, end: cell.dataset.calendarDate, pointerId: event.pointerId,
      x: event.clientX, y: event.clientY };
    grid.setPointerCapture(event.pointerId);
    event.preventDefault();
  });
  grid.addEventListener('pointermove', event => {
    if (!drag || drag.pointerId !== event.pointerId) return;
    const target = document.elementFromPoint(event.clientX, event.clientY);
    const cell = target && target.closest('[data-calendar-date]');
    if (!cell || !grid.contains(cell)) return;
    drag.end = cell.dataset.calendarDate;
    const [start, end] = [drag.start, drag.end].sort();
    grid.querySelectorAll('[data-calendar-date]').forEach(day => {
      day.classList.toggle('selected', day.dataset.calendarDate >= start && day.dataset.calendarDate <= end);
    });
  });
  grid.addEventListener('pointerup', event => {
    if (!drag || drag.pointerId !== event.pointerId) return;
    const selection = drag;
    drag = null;
    const singleDate = selection.start === selection.end;
    const doubleClick = singleDate && lastClick && lastClick.date === selection.start &&
      event.timeStamp - lastClick.time < 500 && event.timeStamp >= lastClick.time &&
      Math.hypot(selection.x - lastClick.x, selection.y - lastClick.y) <= 8;
    lastClick = singleDate && !doubleClick
      ? { date: selection.start, time: event.timeStamp, x: selection.x, y: selection.y } : null;
    showCalendarDate(selection.start, selection.end);
    if (!singleDate || doubleClick) document.getElementById('btn-calendar-add-details').click();
  });
  grid.addEventListener('pointercancel', () => {
    drag = null;
    lastClick = null;
    renderCalendarGrid();
  });
  document.getElementById('btn-close-calendar-panel').addEventListener('click', () => panel.classList.add('hidden'));
  document.getElementById('btn-calendar-add-details').addEventListener('click', () => {
    document.getElementById('btn-open-new-todo-modal').click();
  });
}

function renderCalendarGrid() {
  const grid = document.getElementById('calendar-days-grid');
  if (!grid) return;

  const year = state.calendarYear;
  const month = state.calendarMonth;
  updateViewTitle();

  const firstDay = new Date(year, month - 1, 1);
  const startOffset = (firstDay.getDay() + 6) % 7;
  const daysInMonth = new Date(year, month, 0).getDate();
  const todayStr = getLocalDateStr(0);

  grid.innerHTML = '';

  const totalSlots = Math.ceil((startOffset + daysInMonth) / 7) * 7;
  for (let idx = 0; idx < totalSlots; idx++) {
    const cell = document.createElement('div');
    const date = new Date(year, month - 1, idx - startOffset + 1);
    const dayNum = date.getDate();
    const dateStr = `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(dayNum).padStart(2, '0')}`;
    const dow = idx % 7;

    let cls = 'cal-day-cell';
    if (date.getMonth() !== month - 1) cls += ' outside-month';
    if (dow === 6) cls += ' sun';
    if (dow === 5) cls += ' sat';
    if (dateStr === todayStr) cls += ' today';
    const selectedEnd = state.calendarRangeEnd || state.selectedCalendarDate;
    if (dateStr >= state.selectedCalendarDate && dateStr <= selectedEnd) cls += ' selected';
    cell.className = cls;
    cell.dataset.calendarDate = dateStr;
    cell.tabIndex = 0;
    cell.setAttribute('role', 'group');
    cell.setAttribute('aria-label', dateStr);

    const dayTodos = state.allTodosCache.filter(t => t.due_date <= dateStr && (t.end_date || t.due_date) >= dateStr);
    cell.innerHTML = `
      <div class="cal-day-top">
        <span class="cal-day-num">${dayNum}</span>
      </div>
      <div class="cal-day-preview"></div>
    `;
    const preview = cell.querySelector('.cal-day-preview');
    dayTodos.slice(0, 2).forEach(todo => {
      const button = document.createElement('button');
      const category = state.categories.find(cat => cat.id === todo.category_id);
      const color = category && Number.isInteger(category.color) && category.color >= 0 && category.color <= 0xffffff
        ? category.color : 0x8e9bad;
      button.type = 'button';
      button.className = `cal-day-item-dot ${todo.status === 'done' || todo.status === 'conditional_done' ? 'done' : ''}`;
      button.dataset.todoId = todo.id;
      button.textContent = todo.title;
      button.title = todo.title;
      button.style.setProperty('--event-color', `#${color.toString(16).padStart(6, '0')}`);
      button.addEventListener('click', event => {
        event.stopPropagation();
        document.getElementById('calendar-day-panel').classList.add('hidden');
        openInspector(todo);
      });
      button.addEventListener('dblclick', event => event.stopPropagation());
      preview.appendChild(button);
    });

    cell.addEventListener('keydown', event => {
      if (event.target === cell && (event.key === 'Enter' || event.key === ' ')) {
        event.preventDefault();
        showCalendarDate(dateStr);
        const selectedCell = grid.querySelector(`[data-calendar-date="${dateStr}"]`);
        if (selectedCell) selectedCell.focus();
      }
    });

    grid.appendChild(cell);
  }
}

// ── 수동 JSON 동기화 버튼 ──
function setupSyncButton() {
  const btn = document.getElementById('btn-sync-json');
  if (!btn) return;
  btn.addEventListener('click', async () => {
    try {
      await api.syncJson();
      await refreshAll(true);
      showToast('SQLite ⇄ schedules/{date}_schedule.json 동기화 완료');
    } catch (e) {
      console.error('Sync failed:', e);
    }
  });
}

// ── 네비게이션 탭 ──
function setupNavigation() {
  const viewToggle = document.getElementById('btn-toggle-settings');
  const aiToggle = document.getElementById('btn-ai');
  const remindersView = document.getElementById('tab-reminders');
  const settingsView = document.getElementById('tab-settings');
  const aiView = document.getElementById('tab-ai');
  const brand = document.getElementById('nav-brand-name');
  const back = document.getElementById('btn-settings-back');

  if (viewToggle && aiToggle && remindersView && settingsView && aiView && brand && back) {
    let activePage = 'home';
    const showPage = page => {
      activePage = page;
      remindersView.style.display = page === 'home' ? 'flex' : 'none';
      settingsView.style.display = page === 'settings' ? 'flex' : 'none';
      aiView.style.display = page === 'ai' ? 'flex' : 'none';
      brand.hidden = page !== 'home';
      back.hidden = page === 'home';
      viewToggle.setAttribute('aria-pressed', String(page === 'settings'));
      aiToggle.setAttribute('aria-pressed', String(page === 'ai'));
    };
    viewToggle.addEventListener('click', () => {
      showPage(activePage === 'settings' ? 'home' : 'settings');
    });
    aiToggle.addEventListener('click', () => {
      showPage(activePage === 'ai' ? 'home' : 'ai');
    });
    back.addEventListener('click', () => {
      const returnButton = activePage === 'ai' ? aiToggle : viewToggle;
      showPage('home');
      returnButton.focus();
    });
  }

  const searchInput = document.getElementById('search-input');
  const clearSearchButton = document.getElementById('btn-clear-search');

  const clearSearch = () => {
    if (searchDebounceTimeout) {
      clearTimeout(searchDebounceTimeout);
      searchDebounceTimeout = null;
    }
    todoLoadRequestId += 1;
    state.searchQuery = '';
    if (searchInput) {
      searchInput.value = '';
      searchInput.focus();
    }
    if (clearSearchButton) clearSearchButton.hidden = true;
    loadTodos();
  };

  if (searchInput) {
    searchInput.addEventListener('input', (e) => {
      todoLoadRequestId += 1;
      state.searchQuery = e.target.value;
      if (clearSearchButton) clearSearchButton.hidden = !state.searchQuery;
      if (searchDebounceTimeout) clearTimeout(searchDebounceTimeout);
      searchDebounceTimeout = setTimeout(() => {
        searchDebounceTimeout = null;
        loadTodos();
      }, 250);
    });
    searchInput.addEventListener('keydown', (e) => {
      if (e.key === 'Escape' && searchInput.value) {
        e.preventDefault();
        clearSearch();
      }
    });
  }
  if (clearSearchButton) clearSearchButton.addEventListener('click', clearSearch);
}

// ── 스마트 카드 (오늘, 예정, 전체, 완료됨) ──
function setupSmartCards() {
  const cards = document.querySelectorAll('.smart-card');
  cards.forEach(card => {
    card.addEventListener('click', () => {
      selectSmartFilter(card.dataset.filter);
    });
  });
}

function selectSmartFilter(filterName) {
  const cards = document.querySelectorAll('.smart-card');
  cards.forEach(c => c.classList.toggle('active', c.dataset.filter === filterName));
  document.querySelectorAll('.list-item').forEach(li => li.classList.remove('active'));

  state.currentFilter = filterName;
  state.selectedCategoryId = null;

  if (state.viewMode === 'calendar') {
    state.viewMode = 'list';
    document.getElementById('tab-reminders').classList.remove('calendar-mode');
    document.getElementById('calendar-day-panel').classList.add('hidden');
    updateInspectorPresentation();
    document.querySelectorAll('#view-mode-switch .view-mode-btn').forEach(b => {
      b.classList.toggle('active', b.dataset.mode === 'list');
    });
    document.getElementById('calendar-view-box').classList.add('hidden');
  }

  updateViewTitle();
  loadTodos();
}

function renderSmartCounts() {
  document.getElementById('count-today').textContent = state.smartCounts.today || 0;
  document.getElementById('count-scheduled').textContent = state.smartCounts.scheduled || 0;
  document.getElementById('count-all').textContent = state.smartCounts.all || 0;
  document.getElementById('count-completed').textContent = state.smartCounts.completed || 0;
}

function updateViewTitle() {
  const titleEl = document.getElementById('view-title');
  if (!titleEl) return;

  if (state.viewMode === 'calendar') {
    titleEl.textContent = `${state.calendarYear}년 ${state.calendarMonth}월`;
    titleEl.style.color = 'var(--apple-foreground)';
    return;
  }

  if (state.selectedCategoryId != null) {
    const cat = state.categories.find(c => c.id === state.selectedCategoryId);
    if (cat) {
      titleEl.textContent = cat.name;
      titleEl.style.color = `#${cat.color.toString(16).padStart(6, '0')}`;
      return;
    }
  }

  titleEl.style.color = 'var(--apple-foreground)';
  switch (state.currentFilter) {
    case 'today':
      titleEl.textContent = '오늘';
      break;
    case 'scheduled':
      titleEl.textContent = '예정';
      break;
    case 'all':
      titleEl.textContent = '전체';
      break;
    case 'completed':
      titleEl.textContent = '완료됨';
      break;
  }
}

// ── 카테고리 렌더링 ──
function renderCategories() {
  const container = document.getElementById('category-lists');
  if (!container) return;

  container.innerHTML = '';
  state.categories.forEach(cat => {
    const count = state.categoryCounts[cat.id] || 0;
    const colorHex = `#${cat.color.toString(16).padStart(6, '0')}`;

    const item = document.createElement('button');
    item.type = 'button';
    item.className = `list-item ${state.selectedCategoryId === cat.id ? 'active' : ''}`;
    item.innerHTML = `
      <div class="list-item-left">
        <div class="cat-dot" style="background-color: ${colorHex};"></div>
        <span>${escapeHtml(cat.name)}</span>
      </div>
      <span class="list-count">${count}</span>
    `;

    item.addEventListener('click', () => {
      document.querySelectorAll('.smart-card').forEach(c => c.classList.remove('active'));
      document.querySelectorAll('.list-item').forEach(li => li.classList.remove('active'));

      item.classList.add('active');
      state.selectedCategoryId = cat.id;
      state.currentFilter = null;

      if (state.viewMode === 'calendar') {
        switchViewMode('list');
      } else {
        updateViewTitle();
        loadTodos();
      }
    });

    container.appendChild(item);
  });
}

function updateCategorySelect() {
  const selects = [
    document.getElementById('modal-todo-category'),
  ];

  selects.forEach(select => {
    if (!select) return;
    const currentVal = select.value;
    select.innerHTML = '<option value="">미분류</option>';
    state.categories.forEach(cat => {
      const opt = document.createElement('option');
      opt.value = cat.id;
      opt.textContent = cat.name;
      select.appendChild(opt);
    });
    select.value = currentVal;
  });
}

function createSubtaskEditor(container) {
  const list = container.querySelector('.subtask-list');
  const input = container.querySelector('.subtask-input');
  const addButton = container.querySelector('.subtask-add');
  let items = [];
  const render = () => {
    list.replaceChildren();
    items.forEach((item, index) => {
      const row = document.createElement('div');
      row.className = 'subtask-row';
      const checkbox = document.createElement('input');
      checkbox.type = 'checkbox';
      checkbox.checked = item.done;
      checkbox.setAttribute('aria-label', `${item.title} 완료`);
      checkbox.addEventListener('change', () => { item.done = checkbox.checked; });
      const text = document.createElement('span');
      text.textContent = item.title;
      const remove = document.createElement('button');
      remove.type = 'button';
      remove.className = 'subtask-remove';
      const icon = document.createElement('i');
      icon.className = 'ph ph-x';
      icon.setAttribute('aria-hidden', 'true');
      remove.appendChild(icon);
      remove.setAttribute('aria-label', `${item.title} 삭제`);
      remove.addEventListener('click', () => { items.splice(index, 1); render(); });
      row.append(checkbox, text, remove);
      list.appendChild(row);
    });
  };
  const addPending = () => {
    const title = input.value.trim();
    if (title) {
      items.push({ title, done: false });
      input.value = '';
      render();
    }
  };
  addButton.addEventListener('click', addPending);
  input.addEventListener('keydown', event => {
    if (event.key === 'Enter') {
      event.preventDefault();
      addPending();
    }
  });
  return {
    set(value) {
      items = (value || []).map(item => ({ title: item.title, done: item.done }));
      input.value = '';
      render();
    },
    get() {
      addPending();
      return items.map(item => ({ ...item }));
    },
  };
}

let openTodoEditor = null;

// ── 일정 추가 및 수정 모달 ──
function setupNewTodoModal() {
  const modal = document.getElementById('modal-add-todo');
  const btnOpen = document.getElementById('btn-open-new-todo-modal');
  const btnClose = document.getElementById('btn-close-todo-modal');
  const btnConfirm = document.getElementById('btn-confirm-todo-modal');

  const titleInput = document.getElementById('modal-todo-title');
  const dateInput = document.getElementById('modal-todo-date');
  const endDateInput = document.getElementById('modal-todo-end-date');
  const timeInput = document.getElementById('modal-todo-time');
  const subtasks = createSubtaskEditor(document.getElementById('modal-subtasks'));
  const catSelect = document.getElementById('modal-todo-category');
  const prioritySelect = document.getElementById('modal-todo-priority');
  const memoInput = document.getElementById('modal-todo-memo');
  const dateSummary = document.getElementById('modal-date-summary');
  const timeSummary = document.getElementById('modal-time-summary');
  const calendarGrid = document.getElementById('modal-calendar-grid');
  const calendarMonth = document.getElementById('modal-calendar-month');
  const modeButtons = Array.from(modal.querySelectorAll('[data-date-mode]'));
  const toolButtons = Array.from(modal.querySelectorAll('.todo-tool-button'));
  let pickerMonth = new Date();
  let dateMode = 'single';
  let selectingRangeEnd = false;
  let editingTodo = null;
  let returnFocus = null;

  const showTool = id => {
    toolButtons.forEach(button => {
      const controlledId = button.getAttribute('aria-controls');
      if (controlledId === 'modal-memo-details' || controlledId === 'modal-subtask-details') return;
      const active = controlledId === id;
      button.setAttribute('aria-expanded', String(active));
      document.getElementById(controlledId).hidden = !active;
    });
  };
  toolButtons.forEach(button => button.addEventListener('click', () => {
    const id = button.getAttribute('aria-controls');
    const active = button.getAttribute('aria-expanded') !== 'true';
    if (id === 'modal-memo-details' || id === 'modal-subtask-details') {
      button.setAttribute('aria-expanded', String(active));
      document.getElementById(id).hidden = !active;
      if (active) {
        if (id === 'modal-memo-details') resizeMemo();
        document.getElementById(id === 'modal-memo-details' ? 'modal-todo-memo' : 'modal-subtask-input').focus();
      }
    } else {
      showTool(active ? id : null);
    }
  }));

  const updateDateSummary = () => {
    dateSummary.textContent = dateInput.value && endDateInput.value && endDateInput.value !== dateInput.value
      ? `${formatCalendarDate(dateInput.value)} ~ ${formatCalendarDate(endDateInput.value)}`
      : dateInput.value ? formatCalendarDate(dateInput.value) : '';
    dateSummary.hidden = !dateInput.value;
  };
  const renderPicker = focusDate => {
    const year = pickerMonth.getFullYear();
    const month = pickerMonth.getMonth();
    calendarMonth.textContent = `${month + 1}월 ${year}`;
    calendarGrid.replaceChildren();
    const firstWeekday = (new Date(year, month, 1).getDay() + 6) % 7;
    for (let i = 0; i < firstWeekday; i++) {
      calendarGrid.appendChild(document.createElement('span'));
    }
    const lastDay = new Date(year, month + 1, 0).getDate();
    for (let day = 1; day <= lastDay; day++) {
      const value = `${year}-${String(month + 1).padStart(2, '0')}-${String(day).padStart(2, '0')}`;
      const button = document.createElement('button');
      button.type = 'button';
      button.textContent = String(day);
      button.setAttribute('aria-label', `${year}년 ${formatCalendarDate(value)}`);
      button.setAttribute('aria-pressed', String(value === dateInput.value || value === endDateInput.value));
      button.classList.toggle('is-today', value === getLocalDateStr(0));
      button.classList.toggle('in-range', Boolean(endDateInput.value && value > dateInput.value && value < endDateInput.value));
      button.addEventListener('click', () => {
        if (dateMode === 'range' && selectingRangeEnd && dateInput.value) {
          [dateInput.value, endDateInput.value] = [dateInput.value, value].sort();
          selectingRangeEnd = false;
        } else {
          dateInput.value = value;
          endDateInput.value = '';
          selectingRangeEnd = dateMode === 'range';
        }
        updateDateSummary();
        renderPicker(value);
      });
      calendarGrid.appendChild(button);
      if (value === focusDate) button.focus({ preventScroll: true });
    }
  };
  const setDateMode = mode => {
    dateMode = mode;
    selectingRangeEnd = mode === 'range' && Boolean(dateInput.value) && !endDateInput.value;
    if (mode === 'single') endDateInput.value = '';
    modeButtons.forEach(button => button.setAttribute('aria-pressed', String(button.dataset.dateMode === mode)));
    updateDateSummary();
    renderPicker();
  };
  const updatePriority = () => {
    const label = ['낮음', '보통', '긴급', '높음'][Number(prioritySelect.value)];
    document.getElementById('modal-priority-value').textContent = label;
    prioritySelect.setAttribute('aria-valuetext', label);
  };
  const resizeMemo = () => {
    memoInput.style.height = 'auto';
    memoInput.style.height = `${memoInput.scrollHeight}px`;
  };
  const updateTimeSummary = () => {
    timeSummary.textContent = timeInput.value;
    timeSummary.hidden = !timeInput.value;
  };
  memoInput.addEventListener('input', resizeMemo);
  timeInput.addEventListener('input', updateTimeSummary);
  timeInput.addEventListener('change', updateTimeSummary);
  const openModal = (todo = null) => {
    if (!modal.classList.contains('hidden')) return;
    editingTodo = todo;
    returnFocus = document.activeElement;
    document.getElementById('modal-todo-heading').textContent = todo ? '일정 수정' : '새 일정 추가';
    btnConfirm.title = todo ? '일정 수정 저장' : '일정 등록';
    btnConfirm.setAttribute('aria-label', btnConfirm.title);
    titleInput.value = todo ? todo.title : '';
    btnConfirm.hidden = !titleInput.value.trim();
    dateInput.value = todo ? todo.due_date : state.viewMode === 'calendar' ? state.selectedCalendarDate : '';
    endDateInput.value = todo ? todo.end_date || '' :
      state.viewMode === 'calendar' && state.calendarRangeEnd !== state.selectedCalendarDate
        ? state.calendarRangeEnd || '' : '';
    timeInput.value = todo && todo.due_time ? todo.due_time.slice(0, 5) : '';
    updateTimeSummary();
    subtasks.set(todo ? todo.subtasks : []);
    const categoryId = todo ? todo.category_id : state.selectedCategoryId;
    catSelect.value = categoryId != null ? String(categoryId) : '';
    prioritySelect.value = String(todo ? todo.priority || 0 : 0);
    updatePriority();
    memoInput.value = todo ? todo.memo || '' : '';
    memoInput.style.height = '';
    toolButtons.forEach(button => {
      button.setAttribute('aria-expanded', 'false');
      document.getElementById(button.getAttribute('aria-controls')).hidden = true;
    });
    const [year, month] = (dateInput.value || getLocalDateStr(0)).split('-').map(Number);
    pickerMonth = new Date(year, month - 1, 1);
    setDateMode(endDateInput.value ? 'range' : 'single');
    modal.classList.remove('hidden');
    if (todo && todo.memo) {
      const button = modal.querySelector('[aria-controls="modal-memo-details"]');
      button.setAttribute('aria-expanded', 'true');
      document.getElementById('modal-memo-details').hidden = false;
      resizeMemo();
    }
    if (todo && todo.subtasks && todo.subtasks.length) {
      modal.querySelector('[aria-controls="modal-subtask-details"]').setAttribute('aria-expanded', 'true');
      document.getElementById('modal-subtask-details').hidden = false;
    }
    titleInput.focus();
  };
  openTodoEditor = todo => openModal(todo);

  const closeModal = (force = false) => {
    if (btnConfirm.disabled && force !== true) return;
    modal.classList.add('hidden');
    if (returnFocus && returnFocus.isConnected) returnFocus.focus();
  };

  if (btnOpen) btnOpen.addEventListener('click', () => openModal());
  if (btnClose) btnClose.addEventListener('click', closeModal);
  modal.addEventListener('click', event => {
    if (event.target === modal) closeModal();
  });
  titleInput.addEventListener('input', () => {
    btnConfirm.hidden = !titleInput.value.trim();
  });

  document.addEventListener('keydown', (e) => {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'n' &&
        document.getElementById('tab-reminders').style.display !== 'none') {
      e.preventDefault();
      openModal();
    } else if (e.key === 'Escape' && !modal.classList.contains('hidden')) {
      e.preventDefault();
      closeModal();
    } else if (e.key === 'Tab' && !modal.classList.contains('hidden')) {
      const focusable = Array.from(modal.querySelectorAll('button:not([disabled]):not([hidden]), input:not([disabled]), select:not([disabled]), textarea:not([disabled])'))
        .filter(element => element.getClientRects().length > 0);
      if (!focusable.length) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    }
  });

  document.querySelectorAll('[data-modal-date]').forEach(chip => {
    chip.addEventListener('click', () => {
      const offset = parseInt(chip.dataset.modalDate, 10) || 0;
      dateInput.value = getLocalDateStr(offset);
      endDateInput.value = '';
      selectingRangeEnd = dateMode === 'range';
      const [year, month] = dateInput.value.split('-').map(Number);
      pickerMonth = new Date(year, month - 1, 1);
      updateDateSummary();
      renderPicker();
    });
  });

  modeButtons.forEach(button => button.addEventListener('click', () => setDateMode(button.dataset.dateMode)));
  document.getElementById('modal-calendar-prev').addEventListener('click', () => {
    pickerMonth = new Date(pickerMonth.getFullYear(), pickerMonth.getMonth() - 1, 1);
    renderPicker();
  });
  document.getElementById('modal-calendar-next').addEventListener('click', () => {
    pickerMonth = new Date(pickerMonth.getFullYear(), pickerMonth.getMonth() + 1, 1);
    renderPicker();
  });
  document.getElementById('modal-date-clear').addEventListener('click', () => {
    dateInput.value = '';
    endDateInput.value = '';
    selectingRangeEnd = false;
    updateDateSummary();
    renderPicker();
  });
  document.getElementById('modal-date-done').addEventListener('click', () => {
    showTool(null);
    modal.querySelector('[aria-controls="modal-date-details"]').focus();
  });
  prioritySelect.addEventListener('input', updatePriority);

  document.querySelectorAll('[data-modal-time]').forEach(chip => {
    chip.addEventListener('click', () => {
      timeInput.value = chip.dataset.modalTime || '';
      updateTimeSummary();
    });
  });

  const submitModal = async () => {
    if (btnConfirm.disabled) return;
    const title = titleInput.value.trim();
    if (!title) {
      titleInput.focus();
      return;
    }

    const todayStr = getLocalDateStr(0);
    const dueDate = dateInput.value || todayStr;
    const endDate = endDateInput.value || null;
    if (endDate && endDate < dueDate) {
      showToast('종료일은 시작일보다 빠를 수 없습니다.');
      showTool('modal-date-details');
      document.getElementById('modal-date-done').focus();
      return;
    }
    const draft = {
      title,
      memo: editingTodo ? memoInput.value : memoInput.value.trim(),
      category_id: catSelect.value ? parseInt(catSelect.value, 10) : null,
      due_date: dueDate,
      end_date: endDate,
      due_time: editingTodo && timeInput.value && timeInput.value === (editingTodo.due_time || '').slice(0, 5)
        ? editingTodo.due_time : timeInput.value || null,
      subtasks: subtasks.get(),
      priority: Number(prioritySelect.value),
      status: editingTodo ? editingTodo.status : 'planned',
      condition_note: editingTodo ? editingTodo.condition_note || '' : '',
    };

    btnConfirm.disabled = true;
    try {
      if (draft.subtasks.length || (editingTodo && editingTodo.subtasks && editingTodo.subtasks.length)) await api.requireTodoDetailsSupport();
      if (draft.priority || (editingTodo && editingTodo.priority)) await api.requirePrioritySupport();
      if (editingTodo) {
        await api.updateTodo(editingTodo.id, draft);
      } else {
        await api.createTodo(draft);
      }
      closeModal(true);

      if (!editingTodo && state.viewMode === 'list' && state.selectedCategoryId == null && state.currentFilter === 'today' && dueDate > todayStr) {
        selectSmartFilter('scheduled');
      } else if (!editingTodo && state.viewMode === 'calendar') {
        state.selectedCalendarDate = dueDate;
        state.calendarRangeEnd = endDate || dueDate;
        const [year, month] = dueDate.split('-').map(Number);
        state.calendarYear = year;
        state.calendarMonth = month;
      }

      await refreshAll(false);
      if (editingTodo && state.selectedTodo && state.selectedTodo.id === editingTodo.id) {
        state.selectedTodo = { ...editingTodo, ...draft };
        renderInspectorDetails(state.selectedTodo);
      }
      showToast(editingTodo ? `"${title}" 변경사항이 저장되었습니다` : `"${title}" 일정이 등록되었습니다 (${dueDate})`);
    } catch (err) {
      console.error('Failed to save todo from modal:', err);
      showToast(`일정 ${editingTodo ? '수정' : '등록'} 실패: ${err.message}`);
    } finally {
      btnConfirm.disabled = false;
    }
  };

  if (btnConfirm) btnConfirm.addEventListener('click', submitModal);
  if (titleInput) {
    titleInput.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        submitModal();
      }
    });
  }
}

// ── 체크리스트 렌더링 & 달성률 프로그레스 바 ──
async function deleteTodoFromList(todo) {
  try {
    await api.deleteTodo(todo.id);
    if (state.selectedTodo && state.selectedTodo.id === todo.id) {
      document.getElementById('inspector-panel').classList.add('hidden');
      state.selectedTodo = null;
    }
    await refreshAll(false);
    showToast(`"${todo.title}" 일정이 삭제되었습니다`);
  } catch (err) {
    console.error('Failed to delete todo:', err);
    showToast(`일정 삭제 실패: ${err.message}`);
  }
}

function setupTodoSwipe(wrapper, row, todo) {
  const reveal = 76;
  let startX = 0;
  let startY = 0;
  let pointerId = null;
  let offset = 0;
  let origin = 0;
  let dragging = false;
  let suppressClick = false;
  let busy = false;
  const setOffset = value => {
    offset = value;
    row.style.transform = value ? `translateX(${value}px)` : '';
    wrapper.classList.toggle('swipe-left', value < 0);
    wrapper.classList.toggle('swipe-right', value > 0);
  };
  const act = async direction => {
    if (busy) return;
    busy = true;
    setOffset(0);
    try {
      if (direction === 'delete') await deleteTodoFromList(todo);
      else openTodoEditor(todo);
    } finally {
      busy = false;
    }
  };
  wrapper.querySelector('.swipe-delete').addEventListener('click', () => act('delete'));
  wrapper.querySelector('.swipe-edit').addEventListener('click', () => act('edit'));
  row.addEventListener('click', event => {
    if (suppressClick || offset) {
      event.preventDefault();
      event.stopImmediatePropagation();
      suppressClick = false;
      if (offset) setOffset(0);
    }
  }, true);
  row.addEventListener('pointerdown', event => {
    if (event.button !== 0 || event.target.closest('button, input, .apple-checkbox, .status-pill')) return;
    pointerId = event.pointerId;
    startX = event.clientX;
    startY = event.clientY;
    origin = offset;
    dragging = false;
    suppressClick = false;
  });
  row.addEventListener('pointermove', event => {
    if (pointerId !== event.pointerId) return;
    const dx = event.clientX - startX;
    const dy = event.clientY - startY;
    if (!dragging) {
      if (Math.abs(dy) > 8 && Math.abs(dy) >= Math.abs(dx)) {
        pointerId = null;
        return;
      }
      if (Math.abs(dx) < 8 || Math.abs(dx) <= Math.abs(dy)) return;
      dragging = true;
      row.setPointerCapture(event.pointerId);
      wrapper.classList.add('swiping');
    }
    event.preventDefault();
    const width = wrapper.clientWidth;
    setOffset(Math.max(-width, Math.min(width, origin + dx)));
  });
  const finish = (event, cancelled) => {
    if (pointerId !== event.pointerId) return;
    pointerId = null;
    if (!dragging) return;
    dragging = false;
    wrapper.classList.remove('swiping');
    suppressClick = true;
    const distance = offset;
    if (cancelled) setOffset(0);
    else if (Math.abs(distance) >= Math.max(130, wrapper.clientWidth * 0.55)) {
      act(distance < 0 ? 'delete' : 'edit');
    } else {
      setOffset(Math.abs(distance) >= 48 ? Math.sign(distance) * reveal : 0);
    }
  };
  row.addEventListener('pointerup', event => finish(event, false));
  row.addEventListener('pointercancel', event => finish(event, true));
}

function renderTodoList() {
  const container = document.getElementById('todo-checklist');
  const subtitle = document.getElementById('view-count-subtitle');
  const progFill = document.getElementById('view-progress-fill');
  const progText = document.getElementById('view-progress-text');
  if (!container) return;

  const total = state.todos.length;
  const doneCount = state.todos.filter(t => t.status === 'done' || t.status === 'conditional_done').length;
  const pct = total > 0 ? Math.round((doneCount / total) * 100) : 0;

  subtitle.textContent = `${total}개`;
  if (progFill && progText) {
    progFill.style.width = `${pct}%`;
    progFill.classList.toggle('complete', pct === 100 && total > 0);
    progText.textContent = total > 0 ? `${pct}% 완료 (${doneCount}/${total})` : '일정 없음';
  }

  container.classList.toggle('empty', total === 0);
  if (total === 0) {
    container.innerHTML = '<div class="empty-message">등록된 일정이 없습니다</div>';
    return;
  }

  container.innerHTML = '';
  const todayStr = getLocalDateStr(0);

  state.todos.forEach(todo => {
    const wrapper = document.createElement('div');
    wrapper.className = 'todo-swipe-wrapper';
    wrapper.innerHTML = `
      <button type="button" class="swipe-action swipe-edit" aria-label="일정 수정">수정</button>
      <button type="button" class="swipe-action swipe-delete" aria-label="일정 삭제">제거</button>
    `;
    const row = document.createElement('div');
    row.className = `todo-row ${state.selectedTodo && state.selectedTodo.id === todo.id ? 'selected' : ''}`;

    const isFinished = todo.status === 'done' || todo.status === 'conditional_done';
    const isOverdue = !isFinished && (todo.end_date || todo.due_date) < todayStr;

    let catHtml = '';
    if (todo.category_id != null) {
      const cat = state.categories.find(c => c.id === todo.category_id);
      if (cat) {
        const hex = `#${cat.color.toString(16).padStart(6, '0')}`;
        catHtml = `
          <div class="cat-chip">
            <div class="cat-dot" style="background-color: ${hex};"></div>
            <span>${escapeHtml(cat.name)}</span>
          </div>
        `;
      }
    }

    let dueHtml = `<span><i class="ph ph-calendar"></i> ${todo.due_date}</span>`;
    if (todo.due_time) {
      dueHtml += `<span><i class="ph ph-clock"></i> ${todo.due_time.substring(0, 5)}</span>`;
    }
    if (isOverdue) {
      dueHtml += `<span class="overdue-tag"><i class="ph-bold ph-warning"></i> 기한 초과</span>`;
    }
    if (todo.memo) {
      dueHtml += `<span style="color: var(--apple-muted);"><i class="ph ph-note"></i> ${escapeHtml(todo.memo)}</span>`;
    }

    let conditionHtml = '';
    if (todo.condition_note) {
      conditionHtml = `<span class="condition-tag"><i class="ph ph-tag"></i> 조건: ${escapeHtml(todo.condition_note)}</span>`;
    }

    let checkClass = 'apple-checkbox';
    if (todo.status === 'done') checkClass += ' checked';
    else if (todo.status === 'conditional_done') checkClass += ' conditional';
    else if (todo.status === 'in_progress') checkClass += ' in-progress';

    row.innerHTML = `
      <div class="${checkClass}" data-id="${todo.id}" title="완료 토글"></div>
      <div class="todo-row-content">
        <div class="todo-row-title-line">
          <span class="todo-title ${isFinished ? 'done' : ''} ${isOverdue ? 'overdue' : ''}">${escapeHtml(todo.title)}</span>
          ${catHtml}
        </div>
        <div class="todo-metadata">
          ${dueHtml}
          ${conditionHtml}
        </div>
      </div>
      <div class="todo-row-actions">
        <button class="row-action-btn postpone" title="기한 +1일 미루기"><i class="ph-bold ph-calendar-plus"></i></button>
        <span class="status-pill ${todo.status}" data-id="${todo.id}" title="클릭하여 상태 변경">${getStatusLabel(todo.status)}</span>
        <button class="row-action-btn delete" title="즉시 삭제"><i class="ph-bold ph-trash"></i></button>
      </div>
    `;

    // 체크박스 클릭 (완료 <-> 진행중 토글)
    const checkEl = row.querySelector('.apple-checkbox');
    checkEl.addEventListener('click', async (e) => {
      e.stopPropagation();
      const next = nextTodoStatus(todo.status);
      await api.toggleStatus(todo.id, next);
      await refreshAll(false);
      const label = { planned: '예정', in_progress: '진행중', done: '완료' }[next];
      showToast(`"${todo.title}" ${label}으로 변경됨`);
    });

    // 상태 배지 클릭 (순환)
    const pillEl = row.querySelector('.status-pill');
    pillEl.addEventListener('click', async (e) => {
      e.stopPropagation();
      const next = nextTodoStatus(todo.status);
      await api.toggleStatus(todo.id, next);
      await refreshAll(false);
    });

    // +1일 미루기 퀵 액션
    const btnPostpone = row.querySelector('.row-action-btn.postpone');
    btnPostpone.addEventListener('click', async (e) => {
      e.stopPropagation();
      const nextDate = addDaysToDateStr(todo.due_date, 1);
      await api.updateTodo(todo.id, {
        title: todo.title,
        memo: todo.memo || '',
        category_id: todo.category_id,
        due_date: nextDate,
        end_date: todo.end_date ? addDaysToDateStr(todo.end_date, 1) : null,
        due_time: todo.due_time,
        status: todo.status,
        condition_note: todo.condition_note || '',
      });
      await refreshAll(false);
      showToast(`"${todo.title}" 기한이 ${nextDate}로 변경되었습니다`);
    });

    // 즉시 삭제 퀵 액션
    const btnDel = row.querySelector('.row-action-btn.delete');
    btnDel.addEventListener('click', async (e) => {
      e.stopPropagation();
      await deleteTodoFromList(todo);
    });

    // 행 클릭 -> 인스펙터 열기
    row.addEventListener('click', () => {
      openInspector(todo);
    });

    wrapper.appendChild(row);
    setupTodoSwipe(wrapper, row, todo);
    container.appendChild(wrapper);
  });
}

function getStatusLabel(s) {
  switch (s) {
    case 'planned': return '<i class="ph ph-circle"></i> 진행예정';
    case 'in_progress': return '<i class="ph ph-circle-half"></i> 진행중';
    case 'conditional_done': return '<i class="ph-bold ph-check-circle"></i> 조건부완료';
    case 'done': return '<i class="ph-fill ph-check-circle"></i> 완료';
    default: return s;
  }
}

// ── 세부사항 인스펙터 패널 ──
function setupInspector() {
  const panel = document.getElementById('inspector-panel');
  const btnClose = document.getElementById('btn-close-inspector');
  const btnEdit = document.getElementById('btn-edit-inspect');

  btnEdit.addEventListener('click', () => {
    if (!state.selectedTodo) return;
    openTodoEditor(state.selectedTodo);
  });

  const closeInspector = () => {
    panel.classList.add('hidden');
    state.selectedTodo = null;
    document.querySelectorAll('.todo-row').forEach(r => r.classList.remove('selected'));
    if (inspectorReturnFocus && inspectorReturnFocus.isConnected) inspectorReturnFocus.focus();
    inspectorReturnFocus = null;
  };
  btnClose.addEventListener('click', closeInspector);
}

let inspectorReturnFocus = null;

function renderInspectorDetails(todo) {
  const setText = (id, value) => { document.getElementById(id).textContent = value; };
  const category = state.categories.find(item => item.id === todo.category_id);
  const labels = { planned: '진행예정', in_progress: '진행중', conditional_done: '조건부완료', done: '완료' };
  setText('detail-title', todo.title);
  setText('detail-date', todo.end_date && todo.end_date !== todo.due_date
    ? `${todo.due_date} ~ ${todo.end_date}` : todo.due_date);
  setText('detail-time', todo.due_time ? todo.due_time.slice(0, 5) : '없음');
  setText('detail-category', category ? category.name : '미분류');
  setText('detail-status', labels[todo.status] || todo.status);
  setText('detail-priority', todo.priority ? '!'.repeat(todo.priority) : '낮음');
  setText('detail-memo', todo.memo || '메모가 없습니다.');
  document.getElementById('detail-condition-row').hidden = !todo.condition_note;
  setText('detail-condition', todo.condition_note || '');
  const list = document.getElementById('detail-subtasks');
  const subtasks = todo.subtasks || [];
  list.replaceChildren();
  subtasks.forEach(item => {
    const entry = document.createElement('li');
    entry.textContent = `${item.done ? '☑' : '☐'} ${item.title}`;
    list.appendChild(entry);
  });
  document.getElementById('detail-no-subtasks').hidden = subtasks.length > 0;
}

function updateInspectorPresentation() {
  const panel = document.getElementById('inspector-panel');
  if (!panel) return;
  panel.removeAttribute('role');
  panel.removeAttribute('aria-modal');
}

function openInspector(todo) {
  const panel = document.getElementById('inspector-panel');
  if (panel.classList.contains('hidden')) inspectorReturnFocus = document.activeElement;
  state.selectedTodo = todo;
  updateInspectorPresentation();
  panel.classList.remove('hidden');
  renderInspectorDetails(todo);
  panel.scrollTop = 0;
  if (state.viewMode === 'list') document.getElementById('btn-close-inspector').focus({ preventScroll: true });

  document.querySelectorAll('.todo-row').forEach(r => {
    r.classList.toggle('selected', r.querySelector('.apple-checkbox').dataset.id == todo.id);
  });
}

// ── 카테고리 추가 모달 ──
function setupCategoryModal() {
  const modal = document.getElementById('modal-add-category');
  const btnShow = document.getElementById('btn-show-add-list');
  const btnCancel = document.getElementById('btn-cancel-add-cat');
  const btnConfirm = document.getElementById('btn-confirm-add-cat');
  const inputName = document.getElementById('new-cat-name');
  const colorDots = document.querySelectorAll('.color-dot-pick');

  btnShow.addEventListener('click', () => {
    inputName.value = '';
    modal.classList.remove('hidden');
    inputName.focus();
  });

  btnCancel.addEventListener('click', () => {
    modal.classList.add('hidden');
  });

  colorDots.forEach(dot => {
    dot.addEventListener('click', () => {
      colorDots.forEach(d => d.classList.remove('selected'));
      dot.classList.add('selected');
      state.newCategoryColor = parseInt(dot.dataset.color, 16);
    });
  });

  btnConfirm.addEventListener('click', async () => {
    const name = inputName.value.trim();
    if (!name) return;

    try {
      await api.createCategory(name, state.newCategoryColor);
      modal.classList.add('hidden');
      await refreshAll(false);
      showToast(`"${name}" 목록이 생성되었습니다`);
    } catch (err) {
      console.error('Failed to create category:', err);
    }
  });
}

// ── 설정 탭 ──
const settingsFields = [
  ['setting-hotzone-center', 'hotzone_center_percent', 50],
  ['setting-hotzone-width', 'hotzone_width', 240],
  ['setting-hotzone-height', 'hotzone_height', 4],
  ['setting-hotzone-dwell', 'hotzone_dwell_ms', 150],
  ['setting-hide-delay', 'hide_delay_ms', 400],
  ['setting-panel-width', 'panel_width', 480],
];

function renderSettings(s) {
  const checkHotzone = document.getElementById('setting-hotzone-enabled');
  if (s && checkHotzone) {
    checkHotzone.checked = s.hotzone_enabled !== false;
    settingsFields.forEach(([id, key, fallback]) => {
      document.getElementById(id).value = s[key] == null ? fallback : s[key];
    });
  }
}

async function loadSettings() {
  try {
    const s = await api.getSettings();
    renderSettings(s);
  } catch (err) {
    console.warn('설정 로드 실패:', err);
    showToast('핫존 설정을 불러오지 못했습니다');
  }
}

async function setupSettings() {
  const checkHotzone = document.getElementById('setting-hotzone-enabled');
  const saveButton = document.getElementById('btn-save-hotzone');
  const resetButton = document.getElementById('btn-reset-settings');
  const memoryButton = document.getElementById('btn-refresh-memory');
  const memoryUsage = document.getElementById('settings-memory-usage');
  const cacheButton = document.getElementById('btn-clear-http-cache');
  await loadSettings();

  if (memoryButton && memoryUsage) {
    memoryButton.addEventListener('click', async () => {
      memoryButton.disabled = true;
      try {
        if (!window.electronAPI || !window.electronAPI.getElectronMemory) {
          throw new Error('Electron 환경에서만 측정할 수 있습니다.');
        }
        const kib = await window.electronAPI.getElectronMemory();
        if (!Number.isFinite(kib) || kib < 0) throw new Error('메모리 값이 올바르지 않습니다.');
        memoryUsage.textContent = `${(kib / 1024).toFixed(1)} MB`;
      } catch (error) {
        console.error('메모리 측정 실패:', error);
        memoryUsage.textContent = '측정 실패';
        showToast(`메모리 측정 실패: ${error.message}`);
      } finally {
        memoryButton.disabled = false;
      }
    });
  }

  if (cacheButton) cacheButton.addEventListener('click', async () => {
    if (!window.confirm('임시 HTTP 캐시를 비울까요? 일정과 앱 설정은 유지됩니다.')) return;
    cacheButton.disabled = true;
    try {
      if (!window.electronAPI || !window.electronAPI.clearHttpCache) {
        throw new Error('Electron 환경에서만 캐시를 비울 수 있습니다.');
      }
      await window.electronAPI.clearHttpCache();
      showToast('HTTP 캐시를 비웠습니다');
    } catch (error) {
      console.error('HTTP 캐시 삭제 실패:', error);
      showToast(`HTTP 캐시 삭제 실패: ${error.message}`);
    } finally {
      cacheButton.disabled = false;
    }
  });

  if (resetButton) resetButton.addEventListener('click', async () => {
    if (!window.confirm('앱 설정을 기본값으로 복원할까요? 일정과 목록은 삭제되지 않습니다.')) return;
    resetButton.disabled = true;
    try {
      await api.saveSettings({});
      const saved = await api.getSettings();
      if (saved.hotzone_enabled !== true || saved.autostart !== false ||
          settingsFields.some(([, key, fallback]) => saved[key] !== fallback)) {
        throw new Error('기본 설정을 확인하지 못했습니다.');
      }
      renderSettings(saved);
      if (window.electronAPI && window.electronAPI.reloadHotzoneSettings) {
        window.electronAPI.reloadHotzoneSettings();
      }
      showToast('설정을 기본값으로 복원했습니다');
    } catch (error) {
      console.error('설정 초기화 실패:', error);
      showToast(`설정 초기화 실패: ${error.message}`);
    } finally {
      resetButton.disabled = false;
    }
  });

  saveButton.addEventListener('click', async () => {
    for (const [id] of settingsFields) {
      const input = document.getElementById(id);
      if (!input.value || !input.reportValidity()) return;
    }
    saveButton.disabled = true;
    try {
      const s = await api.getSettings();
      s.hotzone_enabled = checkHotzone.checked;
      settingsFields.forEach(([id, key]) => { s[key] = Number(document.getElementById(id).value); });
      await api.saveSettings(s);
      const saved = await api.getSettings();
      if (saved.hotzone_enabled !== s.hotzone_enabled ||
          settingsFields.some(([, key]) => saved[key] !== s[key])) {
        throw new Error('핫존 설정이 저장되지 않았습니다. 엔진을 최신 버전으로 다시 시작하세요.');
      }
      if (window.electronAPI && window.electronAPI.reloadHotzoneSettings) {
        window.electronAPI.reloadHotzoneSettings();
      }
      showToast('웹캠 핫존 설정이 저장되었습니다');
    } catch (err) {
      console.error('핫존 설정 저장 실패:', err);
      showToast('핫존 설정 저장 실패: 엔진을 확인하세요');
    } finally {
      saveButton.disabled = false;
    }
  });
}

function escapeHtml(str) {
  if (!str) return '';
  return str.replace(/[&<>"']/g, m => ({
    '&': '&amp;',
    '<': '&lt;',
    '>': '&gt;',
    '"': '&quot;',
    "'": '&#039;',
  }[m]));
}
