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

const { app, BrowserWindow, screen, ipcMain, Tray, Menu } = electronModule;
const path = require('path');
const { spawn } = require('child_process');
const fs = require('fs');
const os = require('os');

let mainWindow = null;
let overlayWindow = null;
let tray = null;
let rustProcess = null;

let hotzoneEnteredAt = null;
let leaveHotzoneAt = null;
let isOverlayVisible = false;

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

// Rust 에이전트 프로세스 기동
function ensureRustEngine() {
  const existingInfo = readAgentInfo();
  if (existingInfo && existingInfo.pid) {
    try {
      process.kill(existingInfo.pid, 0);
      console.log(`[Rust Engine] Already running at PID ${existingInfo.pid}, port ${existingInfo.port}`);
      notifyWindowsAgentReady();
      return;
    } catch (e) {
      console.log('[Rust Engine] Previous process dead, starting new instance...');
    }
  }

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
    rustProcess = spawn('cargo', ['run', '--release', '-p', 'openschedule-agent', '--', '--headless'], {
      cwd: path.join(__dirname, '..'),
      detached: false,
      stdio: ['ignore', 'pipe', 'pipe'],
      shell: true,
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

  let checkCount = 0;
  const pollInterval = setInterval(() => {
    checkCount++;
    const info = readAgentInfo();
    if (info && info.pid) {
      try {
        process.kill(info.pid, 0);
        clearInterval(pollInterval);
        console.log(`[Rust Engine] Ready! Port ${info.port}, Token: ${info.token.substring(0, 8)}...`);
        notifyWindowsAgentReady();
      } catch (e) {}
    } else if (checkCount > 60) {
      clearInterval(pollInterval);
      console.warn('[Rust Engine] Timeout waiting for agent.json to be created.');
    }
  }, 250);
}

function notifyWindowsAgentReady() {
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send('agent-ready');
    mainWindow.webContents.send('refresh-data');
  }
  if (overlayWindow && !overlayWindow.isDestroyed()) {
    overlayWindow.webContents.send('agent-ready');
    overlayWindow.webContents.send('refresh-data');
  }
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

  mainWindow.on('closed', () => {
    mainWindow = null;
  });
}

// 2. 상단 웹캠 핫존 드롭다운 오버레이 (Apple Dynamic Island Liquid Glass)
function createOverlayWindow() {
  const primaryDisplay = screen.getPrimaryDisplay();
  const { width: screenWidth } = primaryDisplay.bounds;
  const overlayWidth = 480;
  const overlayHeight = 440;
  const x = Math.round((screenWidth - overlayWidth) / 2);

  overlayWindow = new BrowserWindow({
    width: overlayWidth,
    height: overlayHeight,
    x: x,
    y: 0,
    transparent: true,
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

  overlayWindow.on('closed', () => {
    overlayWindow = null;
  });
}

// 웹캠 상단 핫존 마우스 커서 폴링
function startHotzonePolling() {
  setInterval(() => {
    if (!overlayWindow || overlayWindow.isDestroyed()) return;

    const primaryDisplay = screen.getPrimaryDisplay();
    const { width: screenWidth } = primaryDisplay.bounds;
    const cursor = screen.getCursorScreenPoint();

    // 웹캠 핫존: 화면 중앙 ±120px, 상단 Y <= 6px
    const centerX = screenWidth / 2;
    const inHotzone = cursor.y <= 6 && Math.abs(cursor.x - centerX) <= 120;

    const overlayBounds = overlayWindow.getBounds();
    const inOverlayArea =
      cursor.x >= overlayBounds.x - 10 &&
      cursor.x <= overlayBounds.x + overlayBounds.width + 10 &&
      cursor.y >= 0 &&
      cursor.y <= overlayBounds.y + overlayBounds.height + 15;

    const now = Date.now();

    if (inHotzone) {
      leaveHotzoneAt = null;
      if (!hotzoneEnteredAt) {
        hotzoneEnteredAt = now;
      } else if (now - hotzoneEnteredAt >= 150) {
        if (!isOverlayVisible) {
          isOverlayVisible = true;
          overlayWindow.show();
          overlayWindow.webContents.send('refresh-data');
        }
      }
    } else {
      hotzoneEnteredAt = null;
      if (isOverlayVisible) {
        if (!inOverlayArea) {
          if (!leaveHotzoneAt) {
            leaveHotzoneAt = now;
          } else if (now - leaveHotzoneAt >= 350) {
            isOverlayVisible = false;
            overlayWindow.hide();
          }
        } else {
          leaveHotzoneAt = null;
        }
      }
    }
  }, 35);
}

// 시스템 트레이 설정
function setupTray() {
  try {
    const iconPath = path.join(__dirname, '..', 'crates', 'core', 'resources', 'icon.ico');
    if (fs.existsSync(iconPath)) {
      tray = new Tray(iconPath);
    }
  } catch (e) {}

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
      click: () => {
        app.quit();
      },
    },
  ]);

  if (tray) {
    tray.setToolTip('OpenSchedule v1.0');
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
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.close();
  }
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
  if (overlayWindow && !overlayWindow.isDestroyed()) {
    isOverlayVisible = false;
    overlayWindow.hide();
  }
});

// 앱 시작
app.whenReady().then(() => {
  ensureRustEngine();
  createMainWindow();
  createOverlayWindow();
  setupTray();
  startHotzonePolling();
  startDbWatcher();

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createMainWindow();
    }
  });
});

// CLI 등 외부 프로세스에서 SQLite DB 또는 schedules/*.json을 변경했을 때 실시간 감지하여 UI 갱신
function startDbWatcher() {
  let debounceTimer = null;
  const triggerRefresh = () => {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      if (mainWindow && !mainWindow.isDestroyed()) {
        mainWindow.webContents.send('refresh-data');
      }
      if (overlayWindow && !overlayWindow.isDestroyed()) {
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
      if (filename && (filename.startsWith('todos.db') || filename.startsWith('agent'))) {
        triggerRefresh();
      }
    });
  } catch (e) {
    console.warn('[DB Watcher] Failed to watch dataDir:', e);
  }

  try {
    const schedulesDir = path.join(__dirname, '..', 'schedules');
    if (!fs.existsSync(schedulesDir)) {
      fs.mkdirSync(schedulesDir, { recursive: true });
    }
    fs.watch(schedulesDir, (eventType, filename) => {
      if (filename && filename.endsWith('_schedule.json')) {
        triggerRefresh();
      }
    });
  } catch (e) {
    console.warn('[Schedules Watcher] Failed to watch schedulesDir:', e);
  }
}

app.on('will-quit', () => {
  if (rustProcess) {
    try {
      rustProcess.kill();
    } catch (e) {}
  }
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin' && !tray) {
    app.quit();
  }
});
