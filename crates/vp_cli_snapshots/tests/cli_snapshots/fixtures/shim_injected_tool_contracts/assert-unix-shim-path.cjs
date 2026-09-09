const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const { copyFileSync, mkdirSync, symlinkSync } = require('node:fs');
const path = require('node:path');

assert.equal(process.version, 'v22.18.0');
const bin = path.join(process.env.VP_HOME, 'bin');
const paths = [];
if (process.argv[2] === 'multiple') {
  for (const name of ['installation-a', 'installation-b']) {
    const dir = path.resolve(name);
    mkdirSync(dir);
    copyFileSync(path.join(bin, 'vp'), path.join(dir, 'vp'));
    symlinkSync('vp', path.join(dir, 'node'));
    paths.push(dir);
  }
  paths.push(path.dirname(process.execPath));
} else {
  const dir = path.resolve('node-only');
  mkdirSync(dir);
  symlinkSync(process.execPath, path.join(dir, 'node'));
  paths.push(bin, dir, '/usr/bin', '/bin');
}
const env = { ...process.env, PATH: paths.join(path.delimiter) };
const version = execFileSync('node', ['--version'], {
  env,
  encoding: 'utf8',
  timeout: 10000,
}).trim();
assert.equal(version, process.version);
if (process.argv[2] === 'partial') {
  for (const tool of ['npm', 'npx']) {
    assert.equal(
      execFileSync(tool, ['--version'], { env, encoding: 'utf8', timeout: 10000 }).trim(),
      process.argv[3] || '10.9.3',
    );
  }
}
console.log(`Node and its tools survive ${process.argv[2]} PATH`);
