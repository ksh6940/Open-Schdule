const electronModule = require('electron');
if (typeof electronModule === 'string') {
  // IDE 통합 터미널 등에서 ELECTRON_RUN_AS_NODE=1 환경변수가 상속된 경우 제거 후 재기동
  const { spawn } = require('child_process');
  const env = { ...process.env };
  delete env.ELECTRON_RUN_AS_NODE;
  const child = spawn(electronModule, process.argv.slice(1), {
    env,
    detached: true,
    stdio: 'ignore',
  });
  child.unref();
  process.exit(0);
}

const { app, BrowserWindow, screen, ipcMain, Tray, Menu, nativeImage, session } = electronModule;
const { startGuiControl } = require('./gui-control');
const path = require('path');
const { spawn, spawnSync } = require('child_process');
const fs = require('fs');
const os = require('os');
const http = require('http');

let mainWindow = null;
let overlayWindow = null;
let tray = null;
let rustProcess = null;
let isQuitting = false;
let refreshAgentWatchers = null;
let stopGuiControl = null;

let hotzoneEnteredAt = null;
let leaveHotzoneAt = null;
let isOverlayVisible = false;
let overlayHideTimer = null;
let overlayHideCanCancel = false;
let hotzoneNeedsExit = false;
let hotzoneSettings = {
  enabled: true, width: 240, height: 4, centerPercent: 50, dwellMs: 150, hideDelayMs: 400, panelWidth: 480,
};

function reloadHotzoneSettings() {
  const appData = process.env.APPDATA || path.join(os.homedir(), 'AppData', 'Roaming');
  const file = path.join(appData, 'OpenSchedule', 'settings.json');
  if (!fs.existsSync(file)) return;
  try {
    const settings = JSON.parse(fs.readFileSync(file, 'utf8'));
    const number = (value, fallback, min, max) => {
      if (value == null) return fallback;
      if (!Number.isFinite(value) || value < min || value > max) {
        throw new Error(`Invalid hotzone setting: ${value}`);
      }
      return value;
    };
    if (settings.hotzone_enabled != null && typeof settings.hotzone_enabled !== 'boolean') {
      throw new Error('Invalid hotzone enabled setting');
    }
    hotzoneSettings = {
      enabled: settings.hotzone_enabled !== false,
      width: number(settings.hotzone_width, 240, 20, 4000),
      height: number(settings.hotzone_height, 4, 1, 200),
      centerPercent: number(settings.hotzone_center_percent, 50, 0, 100),
      dwellMs: number(settings.hotzone_dwell_ms, 150, 0, 5000),
      hideDelayMs: number(settings.hide_delay_ms, 400, 0, 10000),
      panelWidth: number(settings.panel_width, 480, 320, 1000),
    };
    hotzoneEnteredAt = null;
    leaveHotzoneAt = null;
    if (!hotzoneSettings.enabled && isOverlayVisible) hideOverlay();
    if (overlayWindow && !overlayWindow.isDestroyed()) positionOverlay();
  } catch (error) {
    console.error('[Hotzone] Failed to load settings; keeping previous settings:', error);
  }
}

function getAgentInfoPaths() {
  const appData = process.env.APPDATA || path.join(os.homedir(), 'AppData', 'Roaming');
  return [
    path.join(appData, 'OpenSchedule', 'agent.json'),
    path.join(appData, 'OpenSchedule', 'agent_info.json'),
  ];
}

function readAgentInfo() {
  const paths = getAgentInfoPaths();
  for (const p of paths) {
    if (fs.existsSync(p)) {
      try {
        const info = JSON.parse(fs.readFileSync(p, 'utf8'));
        if (info && info.port && info.token) {
          return info;
        }
      } catch (e) {}
    }
  }
  return null;
}

function engineResponds(info) {
  return new Promise(resolve => {
    const request = http.get({
      host: '127.0.0.1', port: info.port, path: '/api/settings',
      headers: { Authorization: `Bearer ${info.token}` }, timeout: 1000,
    }, response => {
      response.resume();
      resolve(response.statusCode === 200);
    });
    request.on('timeout', () => request.destroy(new Error('Engine health check timed out')));
    request.on('error', () => resolve(false));
  });
}

// Rust 에이전트 프로세스 기동
async function ensureRustEngine() {
  const existingInfo = readAgentInfo();
  let existingProcessAlive = false;
  if (existingInfo && existingInfo.pid) {
    try {
      process.kill(existingInfo.pid, 0);
      existingProcessAlive = true;
      if (await engineResponds(existingInfo)) {
        console.log(`[Rust Engine] Already running at PID ${existingInfo.pid}, port ${existingInfo.port}`);
        notifyWindowsAgentReady();
        return;
      }
      console.warn('[Rust Engine] Existing process did not authenticate; waiting without starting a duplicate.');
    } catch (e) {
      console.log('[Rust Engine] Previous process dead, starting new instance...');
    }
  }

  if (!existingProcessAlive) spawnRustEngine();

  let checkCount = 0;
  let checking = false;
  const pollInterval = setInterval(async () => {
    if (checking) return;
    checking = true;
    checkCount++;
    const info = readAgentInfo();
    if (info && info.pid) {
      try {
        process.kill(info.pid, 0);
        if (await engineResponds(info)) {
          clearInterval(pollInterval);
          console.log(`[Rust Engine] Ready! Port ${info.port}`);
          notifyWindowsAgentReady();
          checking = false;
          return;
        }
      } catch (e) {}
    }
    if (checkCount > 60) {
      clearInterval(pollInterval);
      console.warn('[Rust Engine] Timeout waiting for authenticated engine.');
    }
    checking = false;
  }, 250);
}

function spawnRustEngine() {
  const execDir = path.dirname(process.execPath);
  const candidates = [
    path.join(__dirname, '..', 'target', 'release', 'openschedule-agent.exe'),
    path.join(__dirname, '..', 'target', 'release', 'openlab.exe'),
    path.join(__dirname, '..', 'target', 'debug', 'openschedule-agent.exe'),
    path.join(__dirname, '..', 'target', 'debug', 'openlab.exe'),
    path.join(execDir, 'openschedule-agent.exe'),
    path.join(execDir, 'openlab.exe'),
    path.join(process.resourcesPath || '', 'openschedule-agent.exe'),
  ];

  const exePath = candidates.find(p => fs.existsSync(p));
  if (exePath) {
    console.log(`[Rust Engine] Spawning binary: ${exePath}`);
    rustProcess = spawn(exePath, ['--headless'], {
      cwd: path.join(__dirname, '..'),
      detached: false,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
  } else {
    console.log('[Rust Engine] Binary not found, spawning via cargo...');
    rustProcess = spawn('cargo', ['run', '--release', '-p', 'openschedule-agent', '--bin', 'openschedule-agent', '--', '--headless'], {
      cwd: path.join(__dirname, '..'),
      detached: false,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
  }

  if (rustProcess) {
    rustProcess.stdout.on('data', (d) => {
      console.log(`[Rust Engine stdout] ${d.toString().trim()}`);
    });
    rustProcess.stderr.on('data', (d) => {
      console.error(`[Rust Engine stderr] ${d.toString().trim()}`);
    });
    rustProcess.on('error', (err) => {
      console.error('[Rust Engine] Failed to spawn:', err);
    });
    rustProcess.on('exit', (code, sig) => {
      console.log(`[Rust Engine] Exited with code ${code}, signal ${sig}`);
    });
  }
}

function notifyWindowsAgentReady() {
  if (refreshAgentWatchers) refreshAgentWatchers();
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send('agent-ready');
    mainWindow.webContents.send('refresh-data');
  }
  if (overlayWindow && !overlayWindow.isDestroyed()) {
    overlayWindow.webContents.send('agent-ready');
    overlayWindow.webContents.send('refresh-data');
  }
}

function quitApplication() {
  if (isQuitting) return;
  isQuitting = true;
  app.quit();
}

// 1. 메인 관리 앱 윈도우 (Frameless + 메뉴바 제거 + Apple Design System)
function createMainWindow() {
  Menu.setApplicationMenu(null);

  mainWindow = new BrowserWindow({
    width: 1200,
    height: 780,
    minWidth: 920,
    minHeight: 600,
    title: 'OpenSchedule',
    frame: false,
    autoHideMenuBar: true,
    backgroundColor: '#f5f5f7',
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false,
    },
  });

  mainWindow.setMenuBarVisibility(false);
  mainWindow.loadFile(path.join(__dirname, 'renderer', 'index.html'));

  mainWindow.on('maximize', () => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      mainWindow.webContents.send('window-maximized', true);
    }
  });

  mainWindow.on('unmaximize', () => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      mainWindow.webContents.send('window-maximized', false);
    }
  });

  mainWindow.webContents.on('did-finish-load', () => {
    const info = readAgentInfo();
    if (info) {
      mainWindow.webContents.send('agent-ready');
      mainWindow.webContents.send('refresh-data');
    }
  });

  mainWindow.on('show', () => {
    if (mainWindow && !mainWindow.isDestroyed()) mainWindow.webContents.send('refresh-data');
  });

  mainWindow.on('close', (event) => {
    if (!isQuitting) {
      event.preventDefault();
      if (tray) {
        mainWindow.hide();
      } else {
        quitApplication();
      }
    }
  });

  mainWindow.on('closed', () => {
    mainWindow = null;
  });
}

// 2. 상단 웹캠 핫존 드롭다운 오버레이 (Apple Dynamic Island Liquid Glass)
function overlayX(bounds, width) {
  const desired = bounds.x + bounds.width * hotzoneSettings.centerPercent / 100 - width / 2;
  return Math.round(Math.max(bounds.x, Math.min(desired, bounds.x + bounds.width - width)));
}

function positionOverlay() {
  const bounds = screen.getPrimaryDisplay().bounds;
  const width = Math.min(Math.round(hotzoneSettings.panelWidth), bounds.width);
  const x = overlayX(bounds, width);
  const current = overlayWindow.getBounds();
  if (current.x !== x || current.y !== bounds.y || current.width !== width) {
    overlayWindow.setBounds({ x, y: bounds.y, width });
  }
}

function createOverlayWindow() {
  const primaryDisplay = screen.getPrimaryDisplay();
  const { y: screenY } = primaryDisplay.bounds;
  const overlayWidth = Math.min(Math.round(hotzoneSettings.panelWidth), primaryDisplay.bounds.width);
  const overlayHeight = 440;
  const x = overlayX(primaryDisplay.bounds, overlayWidth);

  overlayWindow = new BrowserWindow({
    width: overlayWidth,
    height: overlayHeight,
    x: x,
    y: screenY,
    transparent: true,
    backgroundColor: '#00000000',
    hasShadow: false,
    frame: false,
    alwaysOnTop: true,
    skipTaskbar: true,
    resizable: false,
    show: false,
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false,
    },
  });

  overlayWindow.loadFile(path.join(__dirname, 'renderer', 'overlay.html'));
  overlayWindow.webContents.on('did-finish-load', () => {
    if (overlayWindow && !overlayWindow.isDestroyed()) {
      overlayWindow.webContents.send('overlay-visibility', isOverlayVisible);
      if (isOverlayVisible) overlayWindow.webContents.send('refresh-data');
    }
  });

  overlayWindow.on('closed', () => {
    overlayWindow = null;
  });
}

function showOverlay() {
  if (!overlayWindow || overlayWindow.isDestroyed()) createOverlayWindow();
  if (overlayHideTimer) {
    clearTimeout(overlayHideTimer);
    overlayHideTimer = null;
  }
  overlayHideCanCancel = false;
  isOverlayVisible = true;
  overlayWindow.showInactive();
  overlayWindow.webContents.send('overlay-visibility', true);
  overlayWindow.webContents.send('refresh-data');
}

function hideOverlay(canCancel = false) {
  if (!overlayWindow || overlayWindow.isDestroyed() || overlayHideTimer) return;
  isOverlayVisible = false;
  overlayHideCanCancel = canCancel;
  overlayWindow.webContents.send('overlay-visibility', false);
  overlayHideTimer = setTimeout(() => {
    overlayHideTimer = null;
    overlayHideCanCancel = false;
    if (overlayWindow && !overlayWindow.isDestroyed()) overlayWindow.hide();
  }, 230);
}

// 웹캠 상단 핫존 마우스 커서 폴링
function startHotzonePolling() {
  let displayBounds = screen.getPrimaryDisplay().bounds;
  const updateDisplayBounds = () => {
    displayBounds = screen.getPrimaryDisplay().bounds;
    if (overlayWindow && !overlayWindow.isDestroyed()) {
      positionOverlay();
    }
  };
  screen.on('display-added', updateDisplayBounds);
  screen.on('display-removed', updateDisplayBounds);
  screen.on('display-metrics-changed', updateDisplayBounds);

  setInterval(() => {
    if (!hotzoneSettings.enabled) return;

    const { x: screenX, y: screenY, width: screenWidth } = displayBounds;
    const cursor = screen.getCursorScreenPoint();

    // 웹캠 핫존: 화면 중앙 ±120px, 상단 Y <= 6px
    const centerX = screenX + screenWidth * hotzoneSettings.centerPercent / 100;
    const inHotzone = cursor.x >= screenX && cursor.x < screenX + screenWidth &&
      cursor.y >= screenY && cursor.y < screenY + hotzoneSettings.height &&
      Math.abs(cursor.x - centerX) <= hotzoneSettings.width / 2;
    if (!inHotzone) hotzoneNeedsExit = false;

    let inOverlayArea = false;
    if (overlayWindow && !overlayWindow.isDestroyed() && (isOverlayVisible || overlayHideTimer)) {
      const overlayBounds = overlayWindow.getBounds();
      inOverlayArea =
        cursor.x >= overlayBounds.x - 10 &&
        cursor.x <= overlayBounds.x + overlayBounds.width + 10 &&
        cursor.y >= overlayBounds.y &&
        cursor.y <= overlayBounds.y + overlayBounds.height + 15;
    }

    if (overlayHideTimer) {
      if (overlayHideCanCancel && (inHotzone || inOverlayArea)) {
        leaveHotzoneAt = null;
        showOverlay();
      }
      return;
    }

    const now = Date.now();

    if (inHotzone) {
      leaveHotzoneAt = null;
      if (hotzoneNeedsExit) return;
      if (hotzoneEnteredAt === null) {
        hotzoneEnteredAt = now;
        if (!overlayWindow || overlayWindow.isDestroyed()) createOverlayWindow();
      }
      if (now - hotzoneEnteredAt >= hotzoneSettings.dwellMs) {
        if (!isOverlayVisible) {
          hotzoneNeedsExit = true;
          showOverlay();
        }
      }
    } else {
      hotzoneEnteredAt = null;
      if (isOverlayVisible) {
        if (!inOverlayArea) {
          if (!leaveHotzoneAt) {
            leaveHotzoneAt = now;
          } else if (now - leaveHotzoneAt >= hotzoneSettings.hideDelayMs) {
            hideOverlay(true);
          }
        } else {
          leaveHotzoneAt = null;
        }
      }
    }
  }, 100);
}

// 시스템 트레이 설정
function setupTray() {
  try {
    const icon = nativeImage.createFromDataURL('data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAACAAAAAgCAYAAABzenr0AAAAnUlEQVR4nNXUSxKAIAyDYa7smT2I7hzlIWmb0CEzbvm/BVLKdjvO6/dLiUox3ngYEQ2HIOy4CaGKwwhy8L05QBh/AEOEOP4BdBEr40oAFG8Aq+MNIiPOBLjiMwB6mDtuAYwODcWtgPrwcBy5A6MIJY5eQmTuC4y+Bfo48CtS4x5AjQjFuwDik+yLr0JMlw5QIkxLjTMhlKXGrZjddgOypyNATiZ/9QAAAABJRU5ErkJggg==');
    tray = new Tray(icon);
  } catch (e) {
    console.error('[Tray] Failed to create system tray icon:', e);
  }

  const contextMenu = Menu.buildFromTemplate([
    {
      label: 'OpenSchedule 열기',
      click: () => {
        if (mainWindow) {
          mainWindow.show();
          mainWindow.focus();
        } else {
          createMainWindow();
        }
      },
    },
    { type: 'separator' },
    {
      label: '종료',
      click: quitApplication,
    },
  ]);

  if (tray) {
    tray.setToolTip('OpenSchedule v1.10');
    tray.setContextMenu(contextMenu);
    tray.on('click', () => {
      if (mainWindow) {
        mainWindow.show();
        mainWindow.focus();
      } else {
        createMainWindow();
      }
    });
  }
}

// IPC 메시지 처리 (에이전트 정보 & 커스텀 프레임리스 윈도우 컨트롤)
ipcMain.on('get-agent-info-sync', (event) => {
  event.returnValue = readAgentInfo();
});

ipcMain.handle('get-agent-info', () => {
  return readAgentInfo();
});

function getElectronMemory() {
  const metrics = app.getAppMetrics();
  if (!metrics.length || metrics.some(metric => !metric.memory || !Number.isFinite(metric.memory.workingSetSize))) {
    throw new Error('프로세스 메모리 정보를 가져올 수 없습니다.');
  }
  return metrics.reduce((total, metric) => total + metric.memory.workingSetSize, 0);
}

async function clearHttpCache() {
  await session.defaultSession.clearCache();
}
ipcMain.handle('get-electron-memory', getElectronMemory);
ipcMain.handle('clear-http-cache', clearHttpCache);

ipcMain.on('window-minimize', () => {
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.minimize();
  }
});

ipcMain.on('window-maximize', () => {
  if (mainWindow && !mainWindow.isDestroyed()) {
    if (mainWindow.isMaximized()) {
      mainWindow.unmaximize();
    } else {
      mainWindow.maximize();
    }
  }
});

ipcMain.on('window-close', () => {
  if (mainWindow && !mainWindow.isDestroyed()) mainWindow.close();
});

ipcMain.on('app-quit', () => {
  quitApplication();
});

ipcMain.on('open-manager', () => {
  if (mainWindow) {
    mainWindow.show();
    mainWindow.focus();
  } else {
    createMainWindow();
  }
});

ipcMain.on('hide-overlay', () => {
  hideOverlay();
});

ipcMain.on('reload-hotzone-settings', reloadHotzoneSettings);

ipcMain.on('data-changed', event => {
  for (const window of [mainWindow, overlayWindow]) {
    if (window && !window.isDestroyed() && window.isVisible() && window.webContents !== event.sender) {
      window.webContents.send('refresh-data');
    }
  }
});

// 앱 시작
const hasInstanceLock = app.requestSingleInstanceLock();
if (!hasInstanceLock) {
  app.quit();
} else if (process.argv.includes('--openlab-quit')) {
  app.quit();
} else {
  app.on('second-instance', (_event, argv) => {
    if (argv.includes('--openlab-quit')) {
      quitApplication();
      return;
    }
    if (mainWindow && !mainWindow.isDestroyed()) {
      if (mainWindow.isMinimized()) mainWindow.restore();
      mainWindow.show();
      mainWindow.focus();
    }
  });
  app.whenReady().then(() => {
    reloadHotzoneSettings();
    ensureRustEngine().catch(error => console.error('[Rust Engine] Startup failed:', error));
    createMainWindow();
    setupTray();
    const appData = process.env.APPDATA || path.join(os.homedir(), 'AppData', 'Roaming');
    startGuiControl({ directory: path.join(appData, 'OpenSchedule'), getMemory: getElectronMemory, clearCache: clearHttpCache })
      .then(stop => { if (isQuitting) stop(); else stopGuiControl = stop; })
      .catch(error => console.error('[GUI control] Startup failed:', error));
    startHotzonePolling();
    startDbWatcher();

    app.on('activate', () => {
      if (BrowserWindow.getAllWindows().length === 0) {
        createMainWindow();
      }
    });
  });
}

// CLI 등 외부 프로세스에서 SQLite DB 또는 schedules/*.json을 변경했을 때 실시간 감지하여 UI 갱신
function startDbWatcher() {
  let debounceTimer = null;
  const scheduleWatchers = new Map();
  const triggerRefresh = () => {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      if (mainWindow && !mainWindow.isDestroyed() && mainWindow.isVisible()) {
        mainWindow.webContents.send('refresh-data');
      }
      if (isOverlayVisible && overlayWindow && !overlayWindow.isDestroyed()) {
        overlayWindow.webContents.send('refresh-data');
      }
    }, 120);
  };

  try {
    const appData = process.env.APPDATA || path.join(os.homedir(), 'AppData', 'Roaming');
    const dataDir = path.join(appData, 'OpenSchedule');
    if (!fs.existsSync(dataDir)) {
      fs.mkdirSync(dataDir, { recursive: true });
    }
    fs.watch(dataDir, (eventType, filename) => {
      filename = filename && filename.toString();
      if (filename === 'settings.json') reloadHotzoneSettings();
      if (filename && filename.startsWith('agent')) {
        refreshAgentWatchers();
        notifyWindowsAgentReady();
      }
      if (filename === 'todos.db' || (filename === 'todos.db-wal' && eventType === 'change') ||
          (filename && filename.startsWith('agent'))) {
        triggerRefresh();
      }
    });
  } catch (e) {
    console.warn('[DB Watcher] Failed to watch dataDir:', e);
  }

  refreshAgentWatchers = () => {
    const appData = process.env.APPDATA || path.join(os.homedir(), 'AppData', 'Roaming');
    const info = readAgentInfo();
    const directories = [
      path.join(__dirname, '..', 'schedules'),
      path.join(appData, 'OpenSchedule', 'schedules'),
    ];
    if (info && typeof info.schedules_dir === 'string') directories.push(info.schedules_dir);
    for (const dir of new Set(directories.map(value => path.resolve(value)))) {
      if (scheduleWatchers.has(dir)) continue;
      try {
        if (!fs.existsSync(dir)) continue;
        const watcher = fs.watch(dir, (eventType, filename) => {
          if (!filename || filename.toString().endsWith('_schedule.json')) triggerRefresh();
        });
        if (watcher && watcher.on) watcher.on('error', error => {
          console.error('[Schedules Watcher] Failed:', dir, error);
          scheduleWatchers.delete(dir);
        });
        scheduleWatchers.set(dir, watcher);
      } catch (error) {
        console.error('[Schedules Watcher] Failed:', dir, error);
      }
    }
  };
  refreshAgentWatchers();
}

app.on('before-quit', () => {
  isQuitting = true;
});

app.on('will-quit', () => {
  if (stopGuiControl) {
    stopGuiControl();
    stopGuiControl = null;
  }
  if (rustProcess && rustProcess.exitCode === null && rustProcess.pid) {
    try {
      if (process.platform === 'win32') {
        const result = spawnSync('taskkill', ['/PID', String(rustProcess.pid), '/T', '/F'], { windowsHide: true });
        if (result.error || result.status !== 0) {
          console.error('[Rust Engine] Failed to stop process tree:', result.error || result.stderr.toString());
        }
      } else {
        rustProcess.kill();
      }
    } catch (e) {
      console.error('[Rust Engine] Failed to stop:', e);
    }
  }
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin' && !tray) {
    quitApplication();
  }
});
