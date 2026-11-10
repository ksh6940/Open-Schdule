const assert = require('node:assert/strict');
const fs = require('node:fs');
const http = require('node:http');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');
const { startGuiControl } = require('../gui-control');

function request(port, token, method, url, extraHeaders = {}) {
  return new Promise((resolve, reject) => {
    const req = http.request({
      host: '127.0.0.1', port, method, path: url,
      headers: { Authorization: `Bearer ${token}`, ...extraHeaders },
    }, response => {
      let body = '';
      response.setEncoding('utf8');
      response.on('data', chunk => { body += chunk; });
      response.on('end', () => resolve({ status: response.statusCode, body: JSON.parse(body) }));
    });
    req.on('error', reject);
    req.end();
  });
}

test('GUI control authenticates requests and exposes memory and cache operations', async () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'openschedule-control-'));
  let clears = 0;
  const stop = await startGuiControl({
    directory, getMemory: () => 2048, clearCache: async () => { clears++; },
  });
  try {
    const info = JSON.parse(fs.readFileSync(path.join(directory, 'gui-control.json'), 'utf8'));
    assert.equal((await request(info.port, 'invalid', 'GET', '/memory')).status, 401);
    assert.equal((await request(info.port, info.token, 'GET', '/memory', { Origin: 'https://example.com' })).status, 401);
    assert.deepEqual(await request(info.port, info.token, 'GET', '/memory'), { status: 200, body: { working_set_kib: 2048 } });
    assert.equal((await request(info.port, info.token, 'GET', '/http-cache/clear')).status, 404);
    assert.equal(clears, 0);
    assert.deepEqual(await request(info.port, info.token, 'POST', '/http-cache/clear'), { status: 200, body: { ok: true } });
    assert.equal(clears, 1);
  } finally {
    stop();
    assert.equal(fs.existsSync(path.join(directory, 'gui-control.json')), false);
    fs.rmdirSync(directory);
  }
});
