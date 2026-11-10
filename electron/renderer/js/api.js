// OpenSchedule Rust Engine REST Client

class ApiClient {
  constructor() {
    this.baseUrl = 'http://127.0.0.1:0';
    this.token = '';
    this.initialized = false;
    this.overlayEndpointAvailable = true;
    this.dashboardEndpointAvailable = true;
  }

  init() {
    if (window.electronAPI && window.electronAPI.getAgentInfo) {
      const info = window.electronAPI.getAgentInfo();
      if (info && info.port && info.token) {
        const nextUrl = `http://127.0.0.1:${info.port}`;
        if (this.baseUrl !== nextUrl || this.token !== info.token) {
          this.overlayEndpointAvailable = true;
          this.dashboardEndpointAvailable = true;
        }
        this.baseUrl = nextUrl;
        this.token = info.token;
        this.initialized = true;
        console.log(`[API] Connected to Rust Engine at ${this.baseUrl} (PID ${info.pid})`);
        return true;
      }
    }
    this.initialized = false;
    return false;
  }

  async waitForReady(retries = 25, delay = 300) {
    for (let i = 0; i < retries; i++) {
      if (this.init()) return true;
      await new Promise(r => setTimeout(r, delay));
    }
    return false;
  }

  async ensureReady() {
    if (!this.initialized) {
      if (!this.init()) {
        await this.waitForReady();
      }
    }
    return this.initialized;
  }

  async request(path, options = {}) {
    if (!await this.ensureReady()) throw new Error('Rust 엔진 연결 정보를 찾을 수 없습니다.');
    const requestOnce = async () => {
      const headers = {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${this.token}`,
        ...(options.headers || {}),
      };
      const resp = await fetch(`${this.baseUrl}${path}`, {
        ...options,
        headers,
      });

      if (!resp.ok) {
        const text = await resp.text();
        const error = new Error(`API Error ${resp.status}: ${text}`);
        error.status = resp.status;
        throw error;
      }

      const contentType = resp.headers.get('content-type') || '';
      const result = contentType.includes('application/json') ? await resp.json() : await resp.text();
      if (options.method && options.method.toUpperCase() !== 'GET' &&
          window.electronAPI && window.electronAPI.notifyDataChanged) {
        window.electronAPI.notifyDataChanged();
      }
      return result;
    };
    const previousUrl = this.baseUrl;
    const previousToken = this.token;
    try {
      return await requestOnce();
    } catch (err) {
      const changed = this.init() && (this.baseUrl !== previousUrl || this.token !== previousToken);
      if (changed && (!options.method || options.method.toUpperCase() === 'GET')) {
        return await requestOnce();
      }
      console.warn(`Request to ${path} failed:`, err);
      throw err;
    }
  }

  // 투두 CRUD
  async getOverlayData() {
    if (this.overlayEndpointAvailable) {
      try {
        return await this.request('/api/overlay');
      } catch (err) {
        if (err.status !== 404) throw err;
        this.overlayEndpointAvailable = false;
      }
    }
    const [todos, categories] = await Promise.all([
      this.getTodos({ date: 'today_and_overdue' }),
      this.getCategories(),
    ]);
    return { todos, categories };
  }

  async getDashboardData(params = {}) {
    if (this.dashboardEndpointAvailable) {
      try {
        const q = new URLSearchParams();
        if (params.date) q.set('date', params.date);
        if (params.category_id != null) q.set('category_id', params.category_id);
        if (params.status) q.set('status', params.status);
        if (params.search) q.set('search', params.search);
        return await this.request(`/api/dashboard?${q.toString()}`);
      } catch (err) {
        if (err.status !== 404) throw err;
        this.dashboardEndpointAvailable = false;
      }
    }
    const [smart_counts, category_counts, categories, todos] = await Promise.all([
      this.getSmartCounts(),
      this.getCategoryCounts(),
      this.getCategories(),
      this.getTodos({ date: 'all' }),
    ]);
    return { smart_counts, category_counts, categories, todos };
  }

  async getTodos(params = {}) {
    const q = new URLSearchParams();
    if (params.date) q.set('date', params.date);
    if (params.category_id != null) q.set('category_id', params.category_id);
    if (params.status) q.set('status', params.status);
    if (params.search) q.set('search', params.search);
    return await this.request(`/api/todos?${q.toString()}`);
  }

  async createTodo(draft) {
    return await this.request('/api/todos', {
      method: 'POST',
      body: JSON.stringify(draft),
    });
  }

  async requireTodoDetailsSupport() {
    let capabilities;
    try {
      capabilities = await this.request('/api/capabilities');
    } catch (error) {
      if (error.status !== 404) throw error;
      throw new Error('세부 할 일을 사용하려면 엔진을 업데이트하고 다시 실행해주세요.');
    }
    if (!capabilities.reminders_and_subtasks) {
      throw new Error('세부 할 일을 사용하려면 엔진을 업데이트하고 다시 실행해주세요.');
    }
  }

  async requirePrioritySupport() {
    let capabilities;
    try {
      capabilities = await this.request('/api/capabilities');
    } catch (error) {
      if (error.status !== 404) throw error;
      throw new Error('긴급도를 사용하려면 엔진을 업데이트하고 다시 실행해주세요.');
    }
    if (!capabilities.priority) throw new Error('긴급도를 사용하려면 엔진을 업데이트하고 다시 실행해주세요.');
  }

  async updateTodo(id, draft) {
    return await this.request(`/api/todos/${id}`, {
      method: 'PUT',
      body: JSON.stringify(draft),
    });
  }

  async deleteTodo(id) {
    return await this.request(`/api/todos/${id}`, {
      method: 'DELETE',
    });
  }

  async toggleStatus(id, nextStatus = null) {
    return await this.request(`/api/todos/${id}/status`, {
      method: 'POST',
      body: JSON.stringify({ status: nextStatus }),
    });
  }

  // JSON <-> SQLite 수동 동기화
  async syncJson() {
    return await this.request('/api/sync', {
      method: 'POST',
    });
  }

  // 카테고리 CRUD
  async getCategories() {
    return await this.request('/api/categories');
  }

  async createCategory(name, color = 0x0071E3) {
    return await this.request('/api/categories', {
      method: 'POST',
      body: JSON.stringify({ name, color }),
    });
  }

  async updateCategory(id, name, color) {
    return await this.request(`/api/categories/${id}`, {
      method: 'PUT',
      body: JSON.stringify({ name, color }),
    });
  }

  async deleteCategory(id) {
    return await this.request(`/api/categories/${id}`, {
      method: 'DELETE',
    });
  }

  // 스마트 카운트
  async getSmartCounts() {
    return await this.request('/api/smart_counts');
  }

  async getCategoryCounts() {
    return await this.request('/api/category_counts');
  }

  // 설정
  async getSettings() {
    return await this.request('/api/settings');
  }

  async saveSettings(settings) {
    return await this.request('/api/settings', {
      method: 'PUT',
      body: JSON.stringify(settings),
    });
  }

  async checkHealth() {
    try {
      const res = await this.request('/api/settings');
      return res !== null && typeof res === 'object';
    } catch (e) {
      return false;
    }
  }
}

window.api = new ApiClient();
function nextTodoStatus(status) {
  return ({ planned: 'in_progress', in_progress: 'done', done: 'planned', conditional_done: 'planned' })[status] || 'planned';
}
