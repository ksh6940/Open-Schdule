// Main Application State & Controller (Frameless Apple Design System v1.0)

const nowInit = new Date();

const state = {
  viewMode: 'list', // 'list' (투두리스트) | 'calendar' (캘린더 뷰)
  currentFilter: 'today', // 'today', 'scheduled', 'all', 'completed'
  selectedCategoryId: null,
  searchQuery: '',
  filterStatus: null, // 'planned', 'in_progress', 'conditional_done', 'done'

  calendarYear: nowInit.getFullYear(),
  calendarMonth: nowInit.getMonth() + 1, // 1..12
  selectedCalendarDate: getLocalDateStr(0),
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
  setupViewModeSwitch();
  setupCalendarControls();
  setupSmartCards();
  setupStatusFilters();
  setupQuickAdd();
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

  // 인라인 날짜 초기값 오늘로 설정
  const quickDate = document.getElementById('quick-add-date');
  if (quickDate) quickDate.value = getLocalDateStr(0);

  // 엔진 연결 대기 및 데이터 로드
  await initEngineConnection();

  // IPC 이벤트 수신 (엔진 준비 완료 또는 CLI/외부 데이터 변경)
  if (window.electronAPI) {
    if (window.electronAPI.onAgentReady) {
      window.electronAPI.onAgentReady(async () => {
        await initEngineConnection();
      });
    }
    if (window.electronAPI.onRefreshData) {
      window.electronAPI.onRefreshData(() => refreshAll(false));
    }
  }

});

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
    const activeTag = document.activeElement ? document.activeElement.tagName.toLowerCase() : '';
    const isTyping = activeTag === 'input' || activeTag === 'textarea' || activeTag === 'select';

    if ((e.ctrlKey || e.metaKey) && e.key === '1') {
      e.preventDefault();
      switchViewMode('list');
    } else if ((e.ctrlKey || e.metaKey) && e.key === '2') {
      e.preventDefault();
      switchViewMode('calendar');
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
      e.preventDefault();
      const inspector = document.getElementById('inspector-panel');
      if (inspector && !inspector.classList.contains('hidden')) {
        document.getElementById('btn-save-inspect').click();
      } else {
        document.getElementById('btn-sync-json').click();
      }
    } else if (!isTyping && e.key === '/') {
      e.preventDefault();
      const searchInput = document.getElementById('search-input');
      if (searchInput) searchInput.focus();
    } else if (e.key === 'Escape') {
      const inspector = document.getElementById('inspector-panel');
      if (inspector && !inspector.classList.contains('hidden')) {
        document.getElementById('btn-close-inspector').click();
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

  const ready = await api.ensureReady();
  if (ready) {
    if (pill) {
      pill.className = 'status-pill done';
      pill.innerHTML = '<i class="ph-fill ph-circle"></i> 연결됨';
    }
    await refreshAll(true);
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
    const [smartCounts, categoryCounts, categories, allTodos] = await Promise.all([
      api.getSmartCounts(),
      api.getCategoryCounts(),
      api.getCategories(),
      api.getTodos({ date: 'all' }),
    ]);

    state.smartCounts = smartCounts || { today: 0, scheduled: 0, all: 0, completed: 0 };
    state.categoryCounts = categoryCounts || {};
    state.categories = categories || [];
    state.allTodosCache = allTodos || [];

    renderSmartCounts();
    renderCategories();
    updateCategorySelect();

    if (state.viewMode === 'calendar') {
      renderCalendarGrid();
    }

    await loadTodos();
  } catch (err) {
    console.error('Failed to refresh data:', err);
  }
}

async function loadTodos() {
  if (searchDebounceTimeout) {
    clearTimeout(searchDebounceTimeout);
    searchDebounceTimeout = null;
  }
  const requestId = ++todoLoadRequestId;
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

  if (state.filterStatus) {
    params.status = state.filterStatus;
  }

  try {
    const todos = await api.getTodos(params);
    if (requestId !== todoLoadRequestId) return;
    state.todos = todos || [];
    renderTodoList();
    updateScheduleFileIndicator();
  } catch (err) {
    if (requestId === todoLoadRequestId) {
      console.error('Failed to load todos:', err);
    }
  }
}

function updateScheduleFileIndicator() {
  const indicator = document.getElementById('schedule-file-indicator');
  const calBadge = document.getElementById('cal-json-badge');
  const activeDate = state.viewMode === 'calendar' ? state.selectedCalendarDate : getLocalDateStr(0);

  if (indicator) {
    indicator.innerHTML = `<i class="ph ph-file-js"></i> schedules/${activeDate}_schedule.json 자동 동기화`;
  }
  if (calBadge) {
    calBadge.textContent = `schedules/${state.selectedCalendarDate}_schedule.json`;
  }
}

// ── [투두리스트 / 캘린더 뷰] 모드 전환 ──
function switchViewMode(mode) {
  state.viewMode = mode;
  const btns = document.querySelectorAll('#view-mode-switch .view-mode-btn');
  const calBox = document.getElementById('calendar-view-box');

  btns.forEach(b => b.classList.toggle('active', b.dataset.mode === mode));

  if (mode === 'calendar') {
    calBox.classList.remove('hidden');
    const quickDate = document.getElementById('quick-add-date');
    if (quickDate) quickDate.value = state.selectedCalendarDate;
    renderCalendarGrid();
  } else {
    calBox.classList.add('hidden');
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
  const btnToday = document.getElementById('btn-cal-today');
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

  if (btnToday) {
    btnToday.addEventListener('click', () => {
      const now = new Date();
      state.calendarYear = now.getFullYear();
      state.calendarMonth = now.getMonth() + 1;
      state.selectedCalendarDate = getLocalDateStr(0);
      const quickDate = document.getElementById('quick-add-date');
      if (quickDate) quickDate.value = state.selectedCalendarDate;
      renderCalendarGrid();
      updateViewTitle();
      loadTodos();
    });
  }
}

function renderCalendarGrid() {
  const label = document.getElementById('cal-month-label');
  const grid = document.getElementById('calendar-days-grid');
  if (!grid || !label) return;

  const year = state.calendarYear;
  const month = state.calendarMonth;
  label.textContent = `${year}년 ${month}월`;

  const firstDay = new Date(year, month - 1, 1);
  const startOffset = firstDay.getDay();
  const daysInMonth = new Date(year, month, 0).getDate();
  const todayStr = getLocalDateStr(0);

  const byDate = {};
  state.allTodosCache.forEach(t => {
    if (!byDate[t.due_date]) byDate[t.due_date] = [];
    byDate[t.due_date].push(t);
  });

  grid.innerHTML = '';

  const totalSlots = Math.ceil((startOffset + daysInMonth) / 7) * 7;
  for (let idx = 0; idx < totalSlots; idx++) {
    const cell = document.createElement('div');
    if (idx < startOffset || idx >= startOffset + daysInMonth) {
      cell.className = 'cal-day-cell empty';
      grid.appendChild(cell);
      continue;
    }

    const dayNum = idx - startOffset + 1;
    const dateStr = `${year}-${String(month).padStart(2, '0')}-${String(dayNum).padStart(2, '0')}`;
    const dow = idx % 7;

    let cls = 'cal-day-cell';
    if (dow === 0) cls += ' sun';
    if (dow === 6) cls += ' sat';
    if (dateStr === todayStr) cls += ' today';
    if (dateStr === state.selectedCalendarDate) cls += ' selected';
    cell.className = cls;

    const dayTodos = byDate[dateStr] || [];
    const totalCount = dayTodos.length;
    const doneCount = dayTodos.filter(t => t.status === 'done' || t.status === 'conditional_done').length;

    let badgeHtml = '';
    if (totalCount > 0) {
      const allDone = doneCount === totalCount;
      badgeHtml = `<span class="cal-day-badge ${allDone ? 'all-done' : ''}">${doneCount}/${totalCount}</span>`;
    }

    let previewHtml = '';
    dayTodos.slice(0, 2).forEach(t => {
      const isDone = t.status === 'done' || t.status === 'conditional_done';
      previewHtml += `<div class="cal-day-item-dot ${isDone ? 'done' : ''}">• ${escapeHtml(t.title)}</div>`;
    });

    cell.innerHTML = `
      <div class="cal-day-top">
        <span class="cal-day-num">${dayNum}</span>
        ${badgeHtml}
      </div>
      <div class="cal-day-preview">
        ${previewHtml}
      </div>
    `;

    cell.addEventListener('click', () => {
      state.selectedCalendarDate = dateStr;
      const quickDate = document.getElementById('quick-add-date');
      if (quickDate) quickDate.value = dateStr;
      renderCalendarGrid();
      updateViewTitle();
      loadTodos();
    });

    cell.addEventListener('dblclick', () => {
      state.selectedCalendarDate = dateStr;
      const btnOpen = document.getElementById('btn-open-new-todo-modal');
      if (btnOpen) btnOpen.click();
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
  const tabBtns = document.querySelectorAll('.nav-tab-btn');
  tabBtns.forEach(btn => {
    btn.addEventListener('click', () => {
      tabBtns.forEach(b => b.classList.remove('active'));
      btn.classList.add('active');

      const target = btn.dataset.tab;
      document.getElementById('tab-reminders').style.display = target === 'reminders' ? 'flex' : 'none';
      document.getElementById('tab-settings').style.display = target === 'settings' ? 'flex' : 'none';
    });
  });

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
    titleEl.textContent = `${state.selectedCalendarDate}`;
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

// ── 4대 상태 필터 알약 ──
function setupStatusFilters() {
  const pills = document.querySelectorAll('#status-filter-pills .status-pill');
  pills.forEach(pill => {
    pill.addEventListener('click', () => {
      const status = pill.dataset.status;
      if (state.filterStatus === status) {
        state.filterStatus = null;
        pill.classList.remove('active');
      } else {
        pills.forEach(p => p.classList.remove('active'));
        state.filterStatus = status;
        pill.classList.add('active');
      }
      loadTodos();
    });
  });
}

// ── 카테고리 렌더링 ──
function renderCategories() {
  const container = document.getElementById('category-lists');
  if (!container) return;

  container.innerHTML = '';
  state.categories.forEach(cat => {
    const count = state.categoryCounts[cat.id] || 0;
    const colorHex = `#${cat.color.toString(16).padStart(6, '0')}`;

    const item = document.createElement('div');
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

      const quickCat = document.getElementById('quick-add-category');
      if (quickCat) quickCat.value = String(cat.id);

      updateViewTitle();
      loadTodos();
    });

    container.appendChild(item);
  });
}

function updateCategorySelect() {
  const selects = [
    document.getElementById('inspect-category'),
    document.getElementById('quick-add-category'),
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

// ── 인라인 일정 추가 ──
function setupQuickAdd() {
  const input = document.getElementById('quick-add-input');
  const dateInput = document.getElementById('quick-add-date');
  const timeInput = document.getElementById('quick-add-time');
  const catSelect = document.getElementById('quick-add-category');
  const memoInput = document.getElementById('quick-add-memo');
  const btn = document.getElementById('btn-quick-add');

  // 인라인 원클릭 날짜 칩 (오늘, 내일, +1주)
  document.querySelectorAll('[data-inline-date]').forEach(chip => {
    chip.addEventListener('click', () => {
      const offset = parseInt(chip.dataset.inlineDate, 10) || 0;
      if (dateInput) dateInput.value = getLocalDateStr(offset);
    });
  });

  const submit = async () => {
    const title = input.value.trim();
    if (!title) {
      input.focus();
      return;
    }

    const defaultDate = state.viewMode === 'calendar' ? state.selectedCalendarDate : getLocalDateStr(0);
    const dueDate = (dateInput && dateInput.value) ? dateInput.value : defaultDate;
    const dueTime = (timeInput && timeInput.value) ? timeInput.value : null;
    const catId = (catSelect && catSelect.value)
      ? parseInt(catSelect.value, 10)
      : state.selectedCategoryId;
    const memo = (memoInput && memoInput.value) ? memoInput.value.trim() : '';

    const draft = {
      title,
      memo,
      category_id: catId || null,
      due_date: dueDate,
      due_time: dueTime,
      status: 'planned',
      condition_note: '',
    };

    try {
      await api.createTodo(draft);
      input.value = '';
      if (memoInput) memoInput.value = '';
      if (timeInput) timeInput.value = '';

      const todayStr = getLocalDateStr(0);
      if (state.viewMode === 'list' && state.selectedCategoryId == null && state.currentFilter === 'today' && dueDate > todayStr) {
        selectSmartFilter('scheduled');
      } else if (state.viewMode === 'calendar') {
        state.selectedCalendarDate = dueDate;
      }

      await refreshAll(false);
      showToast(`"${title}" 일정이 추가되었습니다 (${dueDate})`);
    } catch (err) {
      console.error('Failed to create todo:', err);
      showToast(`일정 추가 실패: ${err.message}`);
    }
  };

  if (input) {
    input.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        submit();
      }
    });
  }

  if (memoInput) {
    memoInput.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        submit();
      }
    });
  }

  if (btn) {
    btn.addEventListener('click', submit);
  }
}

// ── 새 일정 추가 모달 (상세 등록 & Ctrl+N) ──
function setupNewTodoModal() {
  const modal = document.getElementById('modal-add-todo');
  const btnOpen = document.getElementById('btn-open-new-todo-modal');
  const btnClose = document.getElementById('btn-close-todo-modal');
  const btnCancel = document.getElementById('btn-cancel-todo-modal');
  const btnConfirm = document.getElementById('btn-confirm-todo-modal');

  const titleInput = document.getElementById('modal-todo-title');
  const dateInput = document.getElementById('modal-todo-date');
  const timeInput = document.getElementById('modal-todo-time');
  const catSelect = document.getElementById('modal-todo-category');
  const statusSelect = document.getElementById('modal-todo-status');
  const memoInput = document.getElementById('modal-todo-memo');

  const openModal = () => {
    titleInput.value = '';
    dateInput.value = state.viewMode === 'calendar' ? state.selectedCalendarDate : getLocalDateStr(0);
    timeInput.value = '';
    catSelect.value = state.selectedCategoryId != null ? String(state.selectedCategoryId) : '';
    statusSelect.value = 'planned';
    memoInput.value = '';
    modal.classList.remove('hidden');
    setTimeout(() => titleInput.focus(), 50);
  };

  const closeModal = () => {
    modal.classList.add('hidden');
  };

  if (btnOpen) btnOpen.addEventListener('click', openModal);
  if (btnClose) btnClose.addEventListener('click', closeModal);
  if (btnCancel) btnCancel.addEventListener('click', closeModal);

  document.addEventListener('keydown', (e) => {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'n') {
      e.preventDefault();
      openModal();
    } else if (e.key === 'Escape' && !modal.classList.contains('hidden')) {
      closeModal();
    }
  });

  document.querySelectorAll('[data-modal-date]').forEach(chip => {
    chip.addEventListener('click', () => {
      const offset = parseInt(chip.dataset.modalDate, 10) || 0;
      dateInput.value = getLocalDateStr(offset);
    });
  });

  document.querySelectorAll('[data-modal-time]').forEach(chip => {
    chip.addEventListener('click', () => {
      timeInput.value = chip.dataset.modalTime || '';
    });
  });

  const submitModal = async () => {
    const title = titleInput.value.trim();
    if (!title) {
      titleInput.focus();
      return;
    }

    const todayStr = getLocalDateStr(0);
    const dueDate = dateInput.value || todayStr;
    const draft = {
      title,
      memo: memoInput.value.trim(),
      category_id: catSelect.value ? parseInt(catSelect.value, 10) : null,
      due_date: dueDate,
      due_time: timeInput.value || null,
      status: statusSelect.value || 'planned',
      condition_note: '',
    };

    try {
      await api.createTodo(draft);
      closeModal();

      if (state.viewMode === 'list' && state.selectedCategoryId == null && state.currentFilter === 'today' && dueDate > todayStr) {
        selectSmartFilter('scheduled');
      } else if (state.viewMode === 'calendar') {
        state.selectedCalendarDate = dueDate;
      }

      await refreshAll(false);
      showToast(`"${title}" 일정이 등록되었습니다 (${dueDate})`);
    } catch (err) {
      console.error('Failed to create todo from modal:', err);
      showToast(`일정 등록 실패: ${err.message}`);
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

  if (total === 0) {
    container.innerHTML = `
      <div style="text-align: center; padding: 52px 20px; color: var(--apple-muted);">
        <div style="font-size: 38px; margin-bottom: 8px; color: var(--apple-primary);">
          <i class="ph-bold ph-check-circle"></i>
        </div>
        <div style="font-size: 17px; font-weight: 400; color: var(--apple-foreground);">등록된 일정이 없습니다</div>
        <div style="font-size: 14px; margin-top: 4px;">위 입력란에서 Enter로 즉시 추가하거나 <kbd style="background:#f5f5f7;border:1px solid #d1d1d6;border-radius:4px;padding:1px 5px;font-size:12px;">Ctrl+N</kbd>을 눌러 상세 일정을 등록하세요.</div>
      </div>
    `;
    return;
  }

  container.innerHTML = '';
  const todayStr = getLocalDateStr(0);

  state.todos.forEach(todo => {
    const row = document.createElement('div');
    row.className = `todo-row ${state.selectedTodo && state.selectedTodo.id === todo.id ? 'selected' : ''}`;

    const isFinished = todo.status === 'done' || todo.status === 'conditional_done';
    const isOverdue = !isFinished && todo.due_date < todayStr;

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
      const next = todo.status === 'done' ? 'in_progress' : 'done';
      await api.toggleStatus(todo.id, next);
      await refreshAll(false);
      showToast(next === 'done' ? `"${todo.title}" 완료 처리됨` : `"${todo.title}" 진행중으로 변경됨`);
    });

    // 상태 배지 클릭 (순환)
    const pillEl = row.querySelector('.status-pill');
    pillEl.addEventListener('click', async (e) => {
      e.stopPropagation();
      const cycle = {
        planned: 'in_progress',
        in_progress: 'conditional_done',
        conditional_done: 'done',
        done: 'planned',
      };
      const next = cycle[todo.status] || 'planned';
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
      await api.deleteTodo(todo.id);
      if (state.selectedTodo && state.selectedTodo.id === todo.id) {
        document.getElementById('inspector-panel').classList.add('hidden');
        state.selectedTodo = null;
      }
      await refreshAll(false);
      showToast(`"${todo.title}" 일정이 삭제되었습니다`);
    });

    // 행 클릭 -> 인스펙터 열기
    row.addEventListener('click', () => {
      openInspector(todo);
    });

    container.appendChild(row);
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
  const btnSave = document.getElementById('btn-save-inspect');
  const btnDelete = document.getElementById('btn-delete-inspect');

  const titleInput = document.getElementById('inspect-title');
  const memoInput = document.getElementById('inspect-memo');
  const dateInput = document.getElementById('inspect-date');
  const timeInput = document.getElementById('inspect-time');
  const catSelect = document.getElementById('inspect-category');
  const statusSelect = document.getElementById('inspect-status');
  const conditionBox = document.getElementById('inspect-condition-box');
  const conditionInput = document.getElementById('inspect-condition');

  statusSelect.addEventListener('change', () => {
    conditionBox.style.display = statusSelect.value === 'conditional_done' ? 'flex' : 'none';
  });

  document.getElementById('chip-date-today').addEventListener('click', () => {
    dateInput.value = getLocalDateStr(0);
  });
  document.getElementById('chip-date-tomorrow').addEventListener('click', () => {
    dateInput.value = getLocalDateStr(1);
  });
  document.getElementById('chip-date-nextweek').addEventListener('click', () => {
    dateInput.value = getLocalDateStr(7);
  });

  document.getElementById('chip-time-09').addEventListener('click', () => { timeInput.value = '09:00'; });
  document.getElementById('chip-time-13').addEventListener('click', () => { timeInput.value = '13:00'; });
  document.getElementById('chip-time-18').addEventListener('click', () => { timeInput.value = '18:00'; });
  document.getElementById('chip-time-clear').addEventListener('click', () => { timeInput.value = ''; });

  btnClose.addEventListener('click', () => {
    panel.classList.add('hidden');
    state.selectedTodo = null;
    document.querySelectorAll('.todo-row').forEach(r => r.classList.remove('selected'));
  });

  btnSave.addEventListener('click', async () => {
    if (!state.selectedTodo) return;
    const title = titleInput.value.trim();
    if (!title) return;

    const draft = {
      title,
      memo: memoInput.value,
      category_id: catSelect.value ? parseInt(catSelect.value, 10) : null,
      due_date: dateInput.value || getLocalDateStr(0),
      due_time: timeInput.value || null,
      status: statusSelect.value,
      condition_note: conditionInput.value.trim(),
    };

    try {
      await api.updateTodo(state.selectedTodo.id, draft);
      panel.classList.add('hidden');
      state.selectedTodo = null;
      await refreshAll(false);
      showToast(`"${title}" 변경사항이 저장되었습니다`);
    } catch (err) {
      console.error('Failed to update todo:', err);
    }
  });

  btnDelete.addEventListener('click', async () => {
    if (!state.selectedTodo) return;
    const delTitle = state.selectedTodo.title;
    try {
      await api.deleteTodo(state.selectedTodo.id);
      panel.classList.add('hidden');
      state.selectedTodo = null;
      await refreshAll(false);
      showToast(`"${delTitle}" 일정이 삭제되었습니다`);
    } catch (err) {
      console.error('Failed to delete todo:', err);
    }
  });
}

function openInspector(todo) {
  state.selectedTodo = todo;
  const panel = document.getElementById('inspector-panel');
  panel.classList.remove('hidden');

  document.getElementById('inspect-title').value = todo.title;
  document.getElementById('inspect-memo').value = todo.memo || '';
  document.getElementById('inspect-date').value = todo.due_date;
  document.getElementById('inspect-time').value = todo.due_time ? todo.due_time.substring(0, 5) : '';
  document.getElementById('inspect-category').value = todo.category_id != null ? todo.category_id : '';
  document.getElementById('inspect-status').value = todo.status;

  const conditionBox = document.getElementById('inspect-condition-box');
  const conditionInput = document.getElementById('inspect-condition');
  conditionInput.value = todo.condition_note || '';
  conditionBox.style.display = todo.status === 'conditional_done' ? 'flex' : 'none';

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
async function setupSettings() {
  const checkHotzone = document.getElementById('setting-hotzone-enabled');

  try {
    const s = await api.getSettings();
    if (s && checkHotzone) {
      checkHotzone.checked = s.hotzone_enabled !== false;
    }
  } catch (err) {
    console.warn('설정 로드 실패:', err);
  }

  if (checkHotzone) {
    checkHotzone.addEventListener('change', async () => {
      try {
        const s = await api.getSettings();
        s.hotzone_enabled = checkHotzone.checked;
        await api.saveSettings(s);
        showToast('웹캠 핫존 설정이 저장되었습니다');
      } catch (e) {
        console.error(e);
      }
    });
  }
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
