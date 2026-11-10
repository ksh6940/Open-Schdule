#!/usr/bin/env node
/**
 * OpenSchedule / OpenLab CLI 런처 (`node cli.js ...` / `npm run cli -- ...` / `openschedule ...`)
 * 빌드된 Rust CLI 바이너리(`openlab.exe` / `openschedule.exe`)를 즉시 호출하거나, 없을 경우 cargo로 실행합니다.
 */
const { spawnSync } = require('child_process');
const path = require('path');
const fs = require('fs');

const args = process.argv.slice(2);

const packaged = path.basename(__dirname) === 'app' && path.basename(path.dirname(__dirname)) === 'resources';
const candidates = [
  path.join(__dirname, 'target', 'debug', 'openlab.exe'),
  path.join(__dirname, 'target', 'release', 'openlab.exe'),
  path.join(__dirname, 'target', 'debug', 'openschedule.exe'),
  path.join(__dirname, 'target', 'release', 'openschedule.exe'),
  path.join(__dirname, 'target', 'debug', 'openschedule-agent.exe'),
  path.join(__dirname, 'target', 'release', 'openschedule-agent.exe'),
];
const packagedExe = packaged ? path.join(__dirname, '..', '..', 'openlab.exe') : null;
const exe = packagedExe && fs.existsSync(packagedExe) ? packagedExe :
  candidates.filter(p => fs.existsSync(p))
    .map(p => ({ path: p, modified: fs.statSync(p).mtimeMs }))
    .sort((a, b) => b.modified - a.modified)[0]?.path;

if (exe) {
  const res = spawnSync(exe, args, { stdio: 'inherit' });
  if (res.error) console.error('CLI 실행 실패:', res.error.message);
  process.exit(res.status ?? 1);
} else {
  const res = spawnSync(
    'cargo',
    ['run', '-q', '--bin', 'openlab', '--', ...args],
    { cwd: __dirname, stdio: 'inherit', shell: true }
  );
  if (res.error) console.error('CLI 실행 실패:', res.error.message);
  process.exit(res.status ?? 1);
}
