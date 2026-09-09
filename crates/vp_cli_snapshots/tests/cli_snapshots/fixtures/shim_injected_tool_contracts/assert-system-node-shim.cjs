const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const { mkdirSync, symlinkSync, writeFileSync } = require('node:fs');
const path = require('node:path');

assert.equal(process.version, 'v22.18.0');
if (process.argv[2] === 'setup') {
  mkdirSync('manager');
  mkdirSync('system-shims');
  const runtime = path.dirname(process.execPath).replaceAll("'", "'\\''");
  writeFileSync(
    'manager/tool-manager',
    `#!/bin/sh\nexec '${runtime}/'"\${0##*/}" "$@"\n`,
    { mode: 0o755 },
  );
  for (const tool of ['node', 'npm', 'npx']) {
    symlinkSync('../manager/tool-manager', `system-shims/${tool}`);
  }
} else {
  for (const tool of ['npm', 'npx']) {
    const options = { encoding: 'utf8', timeout: 10000 };
    assert.equal(execFileSync(tool, ['--version'], options).trim(), '10.9.3');
    const args = ['--offline', '--call', 'node --version'];
    if (tool === 'npm') args.unshift('exec');
    assert.equal(execFileSync(tool, args, options).trim(), process.version);
  }
  console.log('Bundled npm/npx use the runtime behind the system Node shim');
}
