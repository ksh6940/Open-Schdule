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
        this.baseUrl = `http://127.0.0.1:${info.port}`;
        this.token = info.token;
        this.initialized = true;
        console.log(`[API] Connected to Rust Engine at ${this.baseUrl} (PID ${info.pid})`);
        return true;
      }
    }
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
    await this.ensureReady();
    const headers = {
      'Content-Type': 'application/json',
      'Authorization': `Bearer ${this.token}`,
      ...(options.headers || {}),
    };

    try {
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
      if (contentType.includes('application/json')) {
        return await resp.json();
      }
      return await resp.text();
    } catch (err) {
      console.warn(`Request to ${path} failed, retrying init...`, err);
      this.init();
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

  async getDashboardData() {
    if (this.dashboardEndpointAvailable) {
      try {
        return await this.request('/api/dashboard');
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
      const res = await this.request('/health');
      return res === 'ok';
    } catch (e) {
      return false;
    }
  }
}

window.api = new ApiClient();
