const crypto = require('crypto');
const fs = require('fs');
const http = require('http');
const path = require('path');

async function startGuiControl({ directory, getMemory, clearCache, log = console }) {
  const token = crypto.randomBytes(32).toString('hex');
  const infoPath = path.join(directory, 'gui-control.json');
  const server = http.createServer(async (request, response) => {
    response.setHeader('Content-Type', 'application/json; charset=utf-8');
    response.setHeader('Cache-Control', 'no-store');
    const send = (status, data) => {
      const body = JSON.stringify(data);
      response.setHeader('Content-Length', Buffer.byteLength(body));
      response.writeHead(status).end(body);
    };
    const auth = request.headers.authorization || '';
    const expected = `Bearer ${token}`;
    const actualBytes = Buffer.from(auth);
    const expectedBytes = Buffer.from(expected);
    if (request.headers.origin || actualBytes.length !== expectedBytes.length ||
        !crypto.timingSafeEqual(actualBytes, expectedBytes)) {
      send(401, { error: '인증 실패' });
      return;
    }
    try {
      let result;
      if (request.method === 'GET' && request.url === '/memory') {
        result = { working_set_kib: getMemory() };
      } else if (request.method === 'POST' && request.url === '/http-cache/clear') {
        await clearCache();
        result = { ok: true };
      } else {
        send(404, { error: '알 수 없는 명령' });
        return;
      }
      send(200, result);
    } catch (error) {
      log.error('[GUI control] Command failed:', error);
      send(500, { error: error.message });
    }
  });
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  try {
    fs.mkdirSync(directory, { recursive: true });
    fs.writeFileSync(infoPath, JSON.stringify({ port: server.address().port, token, pid: process.pid }), { mode: 0o600 });
  } catch (error) {
    server.close();
    throw error;
  }
  server.on('error', error => log.error('[GUI control] Server error:', error));
  return () => {
    server.close();
    try {
      const current = JSON.parse(fs.readFileSync(infoPath, 'utf8'));
      if (current.token === token) fs.unlinkSync(infoPath);
    } catch (error) {
      if (error.code !== 'ENOENT') log.error('[GUI control] Failed to remove connection info:', error);
    }
  };
}

module.exports = { startGuiControl };
