// Wrapper contract tests. The fake `redisctl` on PATH records its argv as JSON
// (token by token - a shell-collapsed record could not catch quoting bugs), so
// these pin the -y mapping, single-token passthrough of shell metacharacters,
// and exit-code forwarding without the real binary. Unix-only: the fake is a
// shell shim.
'use strict';

const { test } = require('node:test');
const assert = require('node:assert');
const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const WRAPPER = path.join(__dirname, '..', 'bin', 'redis-init.js');

function fakeRedisctl(exitCode) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'redis-init-test-'));
  const argsFile = path.join(dir, 'args.json');
  const recorder = path.join(dir, 'record.js');
  fs.writeFileSync(
    recorder,
    `require('node:fs').writeFileSync(${JSON.stringify(argsFile)}, JSON.stringify(process.argv.slice(2))); process.exit(${exitCode});`
  );
  const bin = path.join(dir, 'redisctl');
  fs.writeFileSync(bin, `#!/bin/sh\nexec "${process.execPath}" "${recorder}" "$@"\n`);
  fs.chmodSync(bin, 0o755);
  return { dir, argsFile };
}

function childArgs(argsFile) {
  return JSON.parse(fs.readFileSync(argsFile, 'utf8'));
}

function run(extraArgs, pathDir) {
  return spawnSync(process.execPath, [WRAPPER, ...extraArgs], {
    encoding: 'utf8',
    env: { ...process.env, PATH: pathDir ?? '/nonexistent' },
  });
}

test('the README one-liner shape: zero args become exactly `init`', () => {
  const { dir, argsFile } = fakeRedisctl(0);
  assert.strictEqual(run([], dir).status, 0);
  assert.deepStrictEqual(childArgs(argsFile), ['init']);
});

test('maps -y and --yes to --defaults and forwards the rest verbatim', () => {
  const { dir, argsFile } = fakeRedisctl(0);
  const result = run(['-y', '--agent', 'claude', '--yes', '--dry-run'], dir);
  assert.strictEqual(result.status, 0);
  assert.deepStrictEqual(childArgs(argsFile), [
    'init',
    '--defaults',
    '--agent',
    'claude',
    '--defaults',
    '--dry-run',
  ]);
});

test('shell metacharacters in a pasted URL stay single argv tokens', () => {
  const { dir, argsFile } = fakeRedisctl(0);
  const url = 'redis://user:p^a|s%s@host:6379?timeout=5&clientName=my app';
  const result = run(['--url', url, '--name', 'two words'], dir);
  assert.strictEqual(result.status, 0);
  assert.deepStrictEqual(childArgs(argsFile), [
    'init',
    '--url',
    url,
    '--name',
    'two words',
  ]);
});

test('forwards the exit code verbatim', () => {
  const { dir } = fakeRedisctl(12);
  assert.strictEqual(run(['--cloud'], dir).status, 12);
});

test('a missing redisctl gets the branch-install hint, not a stack trace', () => {
  const result = run(['--dry-run']);
  assert.strictEqual(result.status, 1);
  assert.match(result.stderr, /redisctl is not installed/);
  assert.match(result.stderr, /cargo install --git .* --branch feat\/init-command/);
  assert.doesNotMatch(result.stderr, /at (Object|Module)\./);
});
