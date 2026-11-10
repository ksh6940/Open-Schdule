const { contextBridge, ipcRenderer } = require('electron');
const path = require('path');
const fs = require('fs');
const os = require('os');

// AgentInfo 로드 (AppData/Roaming/OpenSchedule/agent.json 또는 IPC 동기 호출 폴백)
function loadAgentInfo() {
  try {
    const viaIpc = ipcRenderer.sendSync('get-agent-info-sync');
    if (viaIpc && viaIpc.port && viaIpc.token) {
      return viaIpc;
    }
  } catch (e) {}

  try {
    const appData = process.env.APPDATA || path.join(os.homedir(), 'AppData', 'Roaming');
    const candidates = [
      path.join(appData, 'OpenSchedule', 'agent.json'),
      path.join(appData, 'OpenSchedule', 'agent_info.json'),
    ];

    for (const p of candidates) {
      if (fs.existsSync(p)) {
        const raw = fs.readFileSync(p, 'utf8');
        const data = JSON.parse(raw);
        if (data && data.port && data.token) {
          return data;
        }
      }
    }
  } catch (err) {
    console.error('Failed to load agent info:', err);
  }
  return null;
}

contextBridge.exposeInMainWorld('electronAPI', {
  getAgentInfo: () => loadAgentInfo(),
  minimizeWindow: () => ipcRenderer.send('window-minimize'),
  maximizeWindow: () => ipcRenderer.send('window-maximize'),
  closeWindow: () => ipcRenderer.send('window-close'),
  quitApplication: () => ipcRenderer.send('app-quit'),
  onWindowMaximized: (callback) => ipcRenderer.on('window-maximized', (_e, isMax) => callback(isMax)),
  openManager: () => ipcRenderer.send('open-manager'),
  hideOverlay: () => ipcRenderer.send('hide-overlay'),
  onOverlayVisibility: (callback) => ipcRenderer.on('overlay-visibility', (_e, visible) => callback(visible)),
  onRefreshData: (callback) => ipcRenderer.on('refresh-data', callback),
  onAgentReady: (callback) => ipcRenderer.on('agent-ready', callback),
});
