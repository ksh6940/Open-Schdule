const assert = require('node:assert/strict');
const { EventEmitter } = require('node:events');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');

async function startMain({ trayFails = false, existingAgent = false, settings = null, lock = true, argv = [], schedulesDir = null, engineStatus = 200 } = {}) {
  const windows = [];
  const intervals = [];
  const timers = new Map();
  let nextTimer = 1;
  let now = 1000;
  let cursor = { x: 960, y: 0 };
  const ipc = new EventEmitter();
  ipc.handle = () => {};
  const app = new EventEmitter();
  app.requestSingleInstanceLock = () => lock;
  app.whenReady = () => Promise.resolve();
  let quitCount = 0;
  app.quit = () => {
    quitCount++;
    app.emit('before-quit');
    app.emit('will-quit');
  };
  class Window extends EventEmitter {
    constructor(options) {
      super();
      this.options = options;
      this.webContents = new EventEmitter();
      this.messages = [];
      this.webContents.send = (...args) => { this.messages.push(args); };
      this.visible = options.show !== false;
      windows.push(this);
    }
    setMenuBarVisibility() {}
    loadFile() {}
    isDestroyed() { return false; }
    isVisible() { return this.visible; }
    hide() { this.visible = false; }
    show() { this.visible = true; }
    showInactive() { this.visible = true; }
    getBounds() { return this.options; }
    setPosition(x, y) { this.options.x = x; this.options.y = y; }
    focus() {}
    isMinimized() { return false; }
    close() {
      let prevented = false;
      this.emit('close', { preventDefault: () => { prevented = true; } });
      if (!prevented) this.emit('closed');
    }
  }
  let tray;
  class Tray extends EventEmitter {
    constructor(image) {
      super();
      if (trayFails) throw new Error('tray unavailable');
      assert.equal(image.readUInt32BE(0), 0x89504e47);
      tray = this;
    }
    setToolTip() {}
    setContextMenu(menu) { this.menu = menu; }
  }
  const electron = {
    app,
    BrowserWindow: Window,
    ipcMain: ipc,
    Tray,
    Menu: { setApplicationMenu() {}, buildFromTemplate: items => items },
    nativeImage: {
      createFromDataURL: url => Buffer.from(url.split(',')[1], 'base64'),
    },
    screen: {
      getPrimaryDisplay: () => ({ bounds: { x: 0, y: 0, width: 1920 } }),
      getCursorScreenPoint: () => cursor,
      on() {},
    },
  };
  const child = new EventEmitter();
  child.pid = 1234;
  child.exitCode = null;
  child.stdout = new EventEmitter();
  child.stderr = new EventEmitter();
  let spawned = 0;
  let stopped;
  const watchers = new Map();
  const mockFs = {
    existsSync: file => (existingAgent && file.endsWith('agent.json')) || (settings && file.endsWith('settings.json')) || (schedulesDir && file === path.resolve(schedulesDir)),
    readFileSync: file => file.endsWith('settings.json') ? JSON.stringify(settings) : JSON.stringify({ pid: 4321, port: 123, token: 'test', schedules_dir: schedulesDir }),
    mkdirSync() {},
    watch: (dir, callback) => { watchers.set(dir, callback); },
  };
  const source = fs.readFileSync(path.join(__dirname, '..', 'main.js'), 'utf8');
  vm.runInNewContext(source, {
    require: name => {
      if (name === 'electron') return electron;
      if (name === 'fs') return mockFs;
      if (name === 'http') return {
        get: (_options, callback) => {
          queueMicrotask(() => callback({ statusCode: engineStatus, resume() {} }));
          return { on() {} };
        },
      };
      if (name === 'child_process') return {
        spawn: () => { spawned++; return child; },
        spawnSync: (command, args) => { stopped = { command, args }; return { status: 0 }; },
      };
      return require(name);
    },
    __dirname: path.join(__dirname, '..'),
    process: { env: {}, platform: 'win32', execPath: 'electron.exe', argv, kill() {} },
    console: { log() {}, warn() {}, error() {} },
    Date: class extends Date { static now() { return now; } },
    setInterval: callback => { intervals.push(callback); },
    clearInterval() {},
    setTimeout: callback => { const id = nextTimer++; timers.set(id, callback); return id; },
    clearTimeout: id => timers.delete(id),
  });
  await new Promise(resolve => setImmediate(resolve));
  return { app, ipc, windows, tray, watchers, getQuitCount: () => quitCount,
    getSpawned: () => spawned, getStopped: () => stopped,
    move: point => { cursor = point; }, advance: ms => { now += ms; },
    poll: () => intervals[intervals.length - 1](),
    runTimers: () => { for (const [id, callback] of timers) { timers.delete(id); callback(); } },
  };
}

test('watches the actual agent schedule directory and refreshes on changes', async () => {
  const dir = path.resolve('test-agent-schedules');
  const main = await startMain({ existingAgent: true, schedulesDir: dir });
  assert.ok(main.watchers.has(dir));
  main.watchers.get(dir)('change', '2026-01-01_schedule.json');
  main.runTimers();
  assert.deepEqual(main.windows[0].messages.at(-1), ['refresh-data']);
});

test('close hides the window, tray restores it, and quit stops the owned engine', async () => {
  const main = await startMain();
  main.ipc.emit('window-close');
  assert.equal(main.windows[0].visible, false);
  assert.equal(main.getQuitCount(), 0);
  main.tray.emit('click');
  assert.equal(main.windows[0].visible, true);
  main.ipc.emit('app-quit');
  assert.equal(main.getQuitCount(), 1);
  assert.equal(main.getStopped().command, 'taskkill');
  assert.deepEqual(Array.from(main.getStopped().args), ['/PID', '1234', '/T', '/F']);
});

test('tray creation failure makes close quit rather than strand the app', async () => {
  const main = await startMain({ trayFails: true });
  main.ipc.emit('window-close');
  assert.equal(main.getQuitCount(), 1);
});

test('quitting does not terminate an independently running engine', async () => {
  const main = await startMain({ existingAgent: true });
  assert.equal(main.getSpawned(), 0);
  main.ipc.emit('app-quit');
  assert.equal(main.getStopped(), undefined);
});

test('unhealthy existing engine does not report ready or spawn a duplicate', async () => {
  const main = await startMain({ existingAgent: true, engineStatus: 401 });
  assert.equal(main.getSpawned(), 0);
  assert.equal(main.windows[0].messages.some(message => message[0] === 'agent-ready'), false);
});

test('overlay transitions on every show and cancels an automatic hide on re-entry', async () => {
  const main = await startMain({ existingAgent: true });
  assert.equal(main.windows.length, 1);
  main.poll();
  main.advance(160);
  main.poll();
  const overlay = main.windows[1];
  assert.equal(overlay.options.x, 720);
  assert.equal(overlay.options.hasShadow, false);
  assert.equal(overlay.visible, true);
  assert.deepEqual(overlay.messages.at(-2), ['overlay-visibility', true]);

  main.move({ x: 100, y: 500 });
  main.poll();
  main.advance(410);
  main.poll();
  assert.deepEqual(overlay.messages.at(-1), ['overlay-visibility', false]);
  assert.equal(overlay.visible, true);
  main.move({ x: 960, y: 100 });
  main.poll();
  main.runTimers();
  assert.equal(overlay.visible, true);
  assert.deepEqual(overlay.messages.at(-2), ['overlay-visibility', true]);

  main.ipc.emit('hide-overlay');
  main.poll();
  main.runTimers();
  assert.equal(overlay.visible, false);
});

test('duplicate application exits before creating windows', async () => {
  const main = await startMain({ lock: false });
  assert.equal(main.getQuitCount(), 1);
  assert.equal(main.windows.length, 0);
  assert.equal(main.getSpawned(), 0);
});

test('quit request from second instance exits without restoring the window', async () => {
  const main = await startMain();
  main.ipc.emit('window-close');
  main.app.emit('second-instance', {}, ['electron.exe', '--openlab-quit']);
  assert.equal(main.getQuitCount(), 1);
  assert.equal(main.windows[0].visible, false);
  assert.equal(main.getStopped().command, 'taskkill');
});

test('quit request with no existing instance does not start the app or engine', async () => {
  const main = await startMain({ argv: ['electron.exe', '--openlab-quit'] });
  assert.equal(main.getQuitCount(), 1);
  assert.equal(main.windows.length, 0);
  assert.equal(main.getSpawned(), 0);
});

test('custom hotzone uses configured location, dimensions and dwell time', async () => {
  const main = await startMain({ existingAgent: true, settings: {
    hotzone_enabled: true, hotzone_center_percent: 25, hotzone_width: 100,
    hotzone_height: 20, hotzone_dwell_ms: 300,
  } });
  main.move({ x: 960, y: 1 });
  main.poll();
  main.advance(500);
  main.poll();
  assert.equal(main.windows.length, 1);
  main.move({ x: 480, y: 10 });
  main.poll();
  assert.equal(main.windows.length, 2);
  main.advance(299);
  main.poll();
  assert.equal(main.windows[1].visible, false);
  main.advance(1);
  main.poll();
  const overlay = main.windows[1];
  assert.equal(overlay.options.x, 240);
  assert.equal(overlay.visible, true);
  main.ipc.emit('hide-overlay');
  main.runTimers();
  main.advance(500);
  main.poll();
  assert.equal(overlay.visible, false);
  main.move({ x: 480, y: 21 });
  main.poll();
  main.move({ x: 480, y: 10 });
  main.poll();
  main.advance(300);
  main.poll();
  assert.equal(overlay.visible, true);
});

test('disabled hotzone never opens', async () => {
  const main = await startMain({ existingAgent: true, settings: { hotzone_enabled: false } });
  main.poll();
  main.advance(1000);
  main.poll();
  assert.equal(main.windows.length, 1);
});
