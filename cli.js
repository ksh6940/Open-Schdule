#!/usr/bin/env node
/**
 * OpenSchedule / OpenLab CLI 런처 (`node cli.js ...` / `npm run cli -- ...` / `openschedule ...`)
 * 빌드된 Rust CLI 바이너리(`openlab.exe` / `openschedule.exe`)를 즉시 호출하거나, 없을 경우 cargo로 실행합니다.
 */
const { spawnSync } = require('child_process');
const path = require('path');
const fs = require('fs');

const args = process.argv.slice(2);

const candidates = [
  path.join(__dirname, 'target', 'debug', 'openlab.exe'),
  path.join(__dirname, 'target', 'release', 'openlab.exe'),
  path.join(__dirname, 'target', 'debug', 'openschedule.exe'),
  path.join(__dirname, 'target', 'release', 'openschedule.exe'),
  path.join(__dirname, 'target', 'debug', 'openschedule-agent.exe'),
  path.join(__dirname, 'target', 'release', 'openschedule-agent.exe'),
];

const exe = candidates.find(p => fs.existsSync(p));

if (exe) {
  const res = spawnSync(exe, args, { stdio: 'inherit' });
  process.exit(res.status ?? 0);
} else {
  const res = spawnSync(
    'cargo',
    ['run', '-q', '--bin', 'openlab', '--', ...args],
    { cwd: __dirname, stdio: 'inherit', shell: true }
  );
  process.exit(res.status ?? 0);
}

