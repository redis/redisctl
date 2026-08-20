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

const WRAPPER = path.join(__dirname, '..', 'bin', 'redisctl.js');

function fakeRedisctl(exitCode) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'redis-init-test-'));
  const argsFile = path.join(dir, 'args.json');
  const envFile = path.join(dir, 'env.json');
  const recorder = path.join(dir, 'record.js');
  fs.writeFileSync(
    recorder,
    `const fs = require('node:fs');
fs.writeFileSync(${JSON.stringify(argsFile)}, JSON.stringify(process.argv.slice(2)));
const { npm_config_package, npm_config_call, npm_config_registry } = process.env;
fs.writeFileSync(${JSON.stringify(envFile)}, JSON.stringify({ npm_config_package, npm_config_call, npm_config_registry }));
process.exit(${exitCode});`
  );
  const bin = path.join(dir, 'redisctl');
  fs.writeFileSync(bin, `#!/bin/sh\nexec "${process.execPath}" "${recorder}" "$@"\n`);
  fs.chmodSync(bin, 0o755);
  return { dir, argsFile, envFile };
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

test('the README one-liner shape: `init` forwards as exactly `init`', () => {
  const { dir, argsFile } = fakeRedisctl(0);
  assert.strictEqual(run(['init'], dir).status, 0);
  assert.deepStrictEqual(childArgs(argsFile), ['init']);
});

test('zero args reach redisctl bare, so its own help answers', () => {
  const { dir, argsFile } = fakeRedisctl(0);
  assert.strictEqual(run([], dir).status, 0);
  assert.deepStrictEqual(childArgs(argsFile), []);
});

test('maps -y and --yes to --defaults after init and forwards the rest verbatim', () => {
  const { dir, argsFile } = fakeRedisctl(0);
  const result = run(['init', '-y', '--agent', 'claude', '--yes', '--dry-run'], dir);
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

test('outside init, -y passes through untouched', () => {
  const { dir, argsFile } = fakeRedisctl(0);
  assert.strictEqual(run(['profile', 'list', '-y'], dir).status, 0);
  assert.deepStrictEqual(childArgs(argsFile), ['profile', 'list', '-y']);
});

test('shell metacharacters in a pasted URL stay single argv tokens', () => {
  const { dir, argsFile } = fakeRedisctl(0);
  const url = 'redis://user:p^a|s%s@host:6379?timeout=5&clientName=my app';
  const result = run(['init', '--url', url, '--name', 'two words'], dir);
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
  assert.strictEqual(run(['init', '--cloud'], dir).status, 12);
});

test('npm exec package pinning never reaches redisctl child processes', () => {
  // Under `npm exec --package=@redis/init`, npm exports npm_config_package to
  // every descendant; redisctl's own npm/npx calls (client install, skills add)
  // would then resolve THIS package instead of their real target. User npm
  // config (registry, proxy) must survive.
  const { dir, envFile } = fakeRedisctl(0);
  const result = spawnSync(process.execPath, [WRAPPER, 'init', '--dry-run'], {
    encoding: 'utf8',
    env: {
      ...process.env,
      PATH: dir,
      npm_config_package: '/tmp/redis-init.tgz',
      npm_config_call: 'redis-init',
      npm_config_registry: 'https://registry.example',
    },
  });
  assert.strictEqual(result.status, 0);
  const env = JSON.parse(fs.readFileSync(envFile, 'utf8'));
  assert.strictEqual(env.npm_config_package, undefined);
  assert.strictEqual(env.npm_config_call, undefined);
  assert.strictEqual(env.npm_config_registry, 'https://registry.example');
});

test('a missing redisctl gets the branch-install hint, not a stack trace', () => {
  const result = run(['init', '--dry-run']);
  assert.strictEqual(result.status, 1);
  assert.match(result.stderr, /redisctl is not installed/);
  assert.match(result.stderr, /cargo install --git .* --branch feat\/init-command/);
  assert.doesNotMatch(result.stderr, /at (Object|Module)\./);
});
