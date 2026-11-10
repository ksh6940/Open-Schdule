// Webcam Hotzone Overlay Controller

let overlayTodos = [];
let overlayCategories = [];
let overlayVisible = false;
let overlayLoadPromise = null;
let overlayLoadPending = false;
let overlayRenderKey = null;
let overlayRefreshTimer = null;
let overlayRevision = 0;
const overlayPendingTodos = new Set();
let overlayAdding = false;

function getLocalDateStr() {
  const d = new Date();
  const yyyy = d.getFullYear();
  const mm = String(d.getMonth() + 1).padStart(2, '0');
  const dd = String(d.getDate()).padStart(2, '0');
  return `${yyyy}-${mm}-${dd}`;
}

document.addEventListener('DOMContentLoaded', () => {
  updateHeaderDate();
  setupQuickAdd();

  document.getElementById('btn-open-manager').addEventListener('click', () => {
    if (window.electronAPI && window.electronAPI.openManager) {
      window.electronAPI.openManager();
      window.electronAPI.hideOverlay();
    }
  });

  if (window.electronAPI) {
    if (window.electronAPI.onOverlayVisibility) {
      window.electronAPI.onOverlayVisibility((visible) => {
        overlayVisible = visible;
        document.getElementById('overlay-card').classList.toggle('is-visible', visible);
        if (visible) updateHeaderDate();
      });
    }
    if (window.electronAPI.onAgentReady) {
      window.electronAPI.onAgentReady(() => {
        if (overlayVisible) loadOverlayData();
      });
    }
    if (window.electronAPI.onRefreshData) {
      window.electronAPI.onRefreshData(() => {
        if (overlayVisible) scheduleOverlayRefresh();
      });
    }
  }

});

function updateHeaderDate() {
  const now = new Date();
  const day = now.getDate();
  document.getElementById('overlay-calendar-day').textContent = day;

  const weekdays = ['일요일', '월요일', '화요일', '수요일', '목요일', '금요일', '토요일'];
  const month = now.getMonth() + 1;
  const weekday = weekdays[now.getDay()];
  document.getElementById('overlay-date-text').textContent = `${month}월 ${day}일 ${weekday}`;
}

async function loadOverlayData() {
  overlayLoadPending = true;
  if (overlayLoadPromise) return overlayLoadPromise;

  overlayLoadPromise = (async () => {
    while (overlayLoadPending) {
      overlayLoadPending = false;
      await performOverlayLoad();
    }
  })();
  try {
    await overlayLoadPromise;
  } finally {
    overlayLoadPromise = null;
  }
}

function scheduleOverlayRefresh() {
  if (overlayRefreshTimer) clearTimeout(overlayRefreshTimer);
  overlayRefreshTimer = setTimeout(() => {
    overlayRefreshTimer = null;
    if (overlayVisible) loadOverlayData();
  }, 200);
}

async function performOverlayLoad() {
  try {
    const revision = overlayRevision;
    const { todos, categories } = await api.getOverlayData();
    if (revision !== overlayRevision) {
      overlayLoadPending = true;
      return;
    }

    overlayTodos = todos || [];
    overlayCategories = categories || [];

    const renderKey = JSON.stringify([getLocalDateStr(), overlayTodos, overlayCategories]);
    if (renderKey === overlayRenderKey) return;
    renderOverlayList();
    renderProgressPill();
    overlayRenderKey = renderKey;
  } catch (err) {
    console.error('Failed to load overlay data:', err);
  }
}

function applyOverlayTodo(todo) {
  const today = getLocalDateStr();
  overlayTodos = overlayTodos.filter(item => item.id !== todo.id);
  if (todo.due_date === today ||
      (todo.due_date < today && todo.status !== 'done' && todo.status !== 'conditional_done')) {
    overlayTodos.push(todo);
  }
  overlayTodos.sort((a, b) => a.due_date.localeCompare(b.due_date) ||
    (a.due_time == null) - (b.due_time == null) ||
    (a.due_time || '').localeCompare(b.due_time || '') || a.id - b.id);
  overlayRenderKey = JSON.stringify([today, overlayTodos, overlayCategories]);
  renderOverlayList();
  renderProgressPill();
}

async function updateOverlayStatus(todo, next) {
  if (overlayPendingTodos.has(todo.id)) return;
  overlayPendingTodos.add(todo.id);
  overlayRevision++;
  try {
    const updated = await api.toggleStatus(todo.id, next);
    overlayRevision++;
    applyOverlayTodo(updated);
    scheduleOverlayRefresh();
  } catch (err) {
    overlayRevision++;
    console.error('Failed to update overlay todo status:', err);
    showOverlayError('상태를 변경하지 못했습니다.');
    scheduleOverlayRefresh();
  } finally {
    overlayPendingTodos.delete(todo.id);
  }
}

function showOverlayError(message) {
  const error = document.getElementById('overlay-error');
  if (error) error.textContent = message;
}

function renderProgressPill() {
  const pill = document.getElementById('overlay-progress-pill');
  if (!pill) return;

  const total = overlayTodos.length;
  const done = overlayTodos.filter(t => t.status === 'done' || t.status === 'conditional_done').length;

  if (total === 0) {
    pill.textContent = '할 일 없음';
    pill.classList.remove('completed');
    return;
  }

  const pct = Math.round((done / total) * 100);
  pill.textContent = `${done}/${total} 완료 (${pct}%)`;

  if (pct === 100) {
    pill.classList.add('completed');
  } else {
    pill.classList.remove('completed');
  }
}

function renderOverlayList() {
  const container = document.getElementById('overlay-list-container');
  if (!container) return;

  if (overlayTodos.length === 0) {
    container.innerHTML = `
      <div style="text-align: center; padding: 30px 10px; color: var(--apple-muted); font-size: 13.5px; display: flex; align-items: center; justify-content: center; gap: 6px;">
        <i class="ph-bold ph-check-circle" style="color: var(--apple-primary); font-size: 20px;"></i> 오늘 예정된 미리알림이 없습니다
      </div>
    `;
    return;
  }

  container.innerHTML = '';
  const todayStr = getLocalDateStr();
  const categoriesById = new Map(overlayCategories.map(category => [category.id, category]));
  const fragment = document.createDocumentFragment();

  overlayTodos.forEach(todo => {
    const isFinished = todo.status === 'done' || todo.status === 'conditional_done';
    const isOverdue = !isFinished && todo.due_date < todayStr;

    let catHtml = '';
    if (todo.category_id != null) {
      const cat = categoriesById.get(todo.category_id);
      if (cat) {
        const hex = `#${cat.color.toString(16).padStart(6, '0')}`;
        catHtml = `
          <div class="cat-chip" style="font-size: 11px;">
            <div class="cat-dot" style="background-color: ${hex}; width: 6px; height: 6px;"></div>
            <span>${escapeHtml(cat.name)}</span>
          </div>
        `;
      }
    }

    let metaHtml = '';
    if (todo.due_time) {
      metaHtml += `<span><i class="ph ph-clock"></i> ${todo.due_time}</span>`;
    }
    if (isOverdue) {
      metaHtml += `<span style="color: var(--apple-danger); font-weight: 600;"><i class="ph-bold ph-warning"></i> 기한초과</span>`;
    }
    if (todo.condition_note) {
      metaHtml += `<span class="condition-tag" style="font-size: 10.5px;">조건: ${escapeHtml(todo.condition_note)}</span>`;
    }

    let checkClass = 'apple-checkbox';
    if (todo.status === 'done') checkClass += ' checked';
    else if (todo.status === 'conditional_done') checkClass += ' conditional';
    else if (todo.status === 'in_progress') checkClass += ' in-progress';

    const row = document.createElement('div');
    row.className = 'overlay-item';
    row.innerHTML = `
      <div class="${checkClass}" data-id="${todo.id}" style="width: 19px; height: 19px;"></div>
      <div class="overlay-item-text">
        <span class="overlay-item-title ${isFinished ? 'done' : ''}">${escapeHtml(todo.title)}</span>
        <div class="overlay-item-meta">
          ${catHtml}
          ${metaHtml}
        </div>
      </div>
      <span class="status-pill ${todo.status}" data-id="${todo.id}" style="font-size: 11px; padding: 2px 7px;">${getStatusLabel(todo.status)}</span>
    `;

    const checkEl = row.querySelector('.apple-checkbox');
    checkEl.addEventListener('click', async () => {
      const next = todo.status === 'done' ? 'in_progress' : 'done';
      await updateOverlayStatus(todo, next);
    });

    const pillEl = row.querySelector('.status-pill');
    pillEl.addEventListener('click', async () => {
      const cycle = {
        planned: 'in_progress',
        in_progress: 'conditional_done',
        conditional_done: 'done',
        done: 'planned',
      };
      const next = cycle[todo.status] || 'planned';
      await updateOverlayStatus(todo, next);
    });

    fragment.appendChild(row);
  });
  container.appendChild(fragment);
}

function setupQuickAdd() {
  const input = document.getElementById('overlay-quick-input');
  const timeInput = document.getElementById('overlay-quick-time');
  const btn = document.getElementById('btn-overlay-quick-add');
  if (!input) return;

  const submit = async () => {
    const title = input.value.trim();
    if (!title || overlayAdding) return;

    const todayStr = getLocalDateStr();
    const draft = {
      title,
      memo: '',
      category_id: null,
      due_date: todayStr,
      due_time: (timeInput && timeInput.value) ? timeInput.value : null,
      status: 'planned',
      condition_note: '',
    };

    overlayAdding = true;
    if (btn) btn.disabled = true;
    overlayRevision++;
    try {
      const todo = await api.createTodo(draft);
      overlayRevision++;
      if (input.value.trim() === title) input.value = '';
      if (timeInput && (timeInput.value || null) === draft.due_time) timeInput.value = '';
      applyOverlayTodo(todo);
      showOverlayError('');
      scheduleOverlayRefresh();
    } catch (err) {
      overlayRevision++;
      console.error('Failed to create todo in overlay:', err);
      showOverlayError('일정을 추가하지 못했습니다.');
      scheduleOverlayRefresh();
    } finally {
      overlayAdding = false;
      if (btn) btn.disabled = false;
    }
  };

  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      submit();
    }
  });

  if (btn) {
    btn.addEventListener('click', submit);
  }
}

function getStatusLabel(s) {
  switch (s) {
    case 'planned': return '<i class="ph ph-circle"></i> 진행예정';
    case 'in_progress': return '<i class="ph ph-circle-half"></i> 진행중';
    case 'conditional_done': return '<i class="ph-bold ph-check-circle"></i> 조건부';
    case 'done': return '<i class="ph-fill ph-check-circle"></i> 완료';
    default: return escapeHtml(s);
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
