const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');

const source = fs.readFileSync(path.join(__dirname, '..', '..', 'cli.js'), 'utf8');

function runLauncher(dirname, existing, result) {
  let call;
  let exit;
  let error;
  vm.runInNewContext(source, {
    __dirname: dirname,
    require(name) {
      if (name === 'fs') return { existsSync: file => file === existing };
      if (name === 'child_process') return { spawnSync: (...args) => { call = args; return result; } };
      return require(name);
    },
    process: { argv: ['node', 'cli.js', 'quit'], exit: code => { exit = code; } },
    console: { error: (...args) => { error = args; } },
  });
  return { call, exit, error };
}

test('packaged CLI uses its sibling openlab executable', () => {
  const dir = path.join('C:', 'package', 'resources', 'app');
  const exe = path.join(dir, '..', '..', 'openlab.exe');
  const result = runLauncher(dir, exe, { status: 0 });
  assert.equal(result.call[0], exe);
  assert.deepEqual(Array.from(result.call[1]), ['quit']);
  assert.equal(result.exit, 0);
});

test('launcher reports spawn failures instead of returning success', () => {
  const dir = path.join('C:', 'project');
  const result = runLauncher(dir, null, { status: null, error: new Error('not found') });
  assert.equal(result.exit, 1);
  assert.match(result.error[1], /not found/);
});
