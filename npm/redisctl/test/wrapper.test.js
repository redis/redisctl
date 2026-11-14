// Wrapper contract tests. The fake `redisctl` on PATH records its argv as JSON
// (token by token - a shell-collapsed record could not catch quoting bugs), so
// these pin the -y mapping, single-token passthrough of shell metacharacters,
// and exit-code forwarding without the real binary. Install tests serve a fake
// release installer from a local server. Unix-only: the fakes are shell shims.
'use strict';

const { test } = require('node:test');
const assert = require('node:assert');
const { spawn, spawnSync } = require('node:child_process');
const fs = require('node:fs');
const http = require('node:http');
const os = require('node:os');
const path = require('node:path');

const WRAPPER = path.join(__dirname, '..', 'bin', 'redisctl.js');
const VERSION = require('../package.json').version;
const DEAD_URL = 'http://127.0.0.1:9';

function tempDir() {
  return fs.mkdtempSync(path.join(os.tmpdir(), 'redisctl-wrapper-test-'));
}

// A redisctl that records each run. `--version` is the wrapper's probe: it
// answers without recording.
function fakeRedisctl(exitCode, { dir = tempDir(), version = VERSION } = {}) {
  fs.mkdirSync(dir, { recursive: true });
  const argsFile = path.join(dir, 'args.json');
  const envFile = path.join(dir, 'env.json');
  const recorder = path.join(dir, 'record.js');
  fs.writeFileSync(
    recorder,
    `const fs = require('node:fs');
const argv = process.argv.slice(2);
if (argv.join(' ') === '--version') {
  console.log('redisctl ${version}');
  process.exit(0);
}
fs.writeFileSync(${JSON.stringify(argsFile)}, JSON.stringify(argv));
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

function testEnv(pathDir, extra = {}) {
  return {
    ...process.env,
    PATH: pathDir ?? '/nonexistent',
    CARGO_HOME: tempDir(),
    XDG_DATA_HOME: tempDir(),
    REDISCTL_RELEASES_URL: DEAD_URL,
    ...extra,
  };
}

function run(extraArgs, pathDir, wrapper = WRAPPER) {
  return spawnSync(process.execPath, [wrapper, ...extraArgs], {
    encoding: 'utf8',
    env: testEnv(pathDir),
  });
}

// Async, so the in-process release server can answer the wrapper's download.
function runAsync(extraArgs, env, [command, ...before] = [process.execPath, WRAPPER]) {
  return new Promise((resolve) => {
    const child = spawn(command, [...before, ...extraArgs], { env });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (chunk) => (stdout += chunk));
    child.stderr.on('data', (chunk) => (stderr += chunk));
    child.on('close', (status) => resolve({ status, stdout, stderr }));
  });
}

// Serves `installer` as this version's release installer; returns the base URL.
async function releaseServer(t, installer, version = VERSION) {
  const requests = [];
  const server = http.createServer((req, res) => {
    requests.push(req.url);
    if (installer !== undefined && req.url === `/redisctl-v${version}/redisctl-installer.sh`) {
      res.end(installer);
    } else {
      res.statusCode = 404;
      res.end('Not Found');
    }
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  t.after(() => server.close());
  return { url: `http://127.0.0.1:${server.address().port}`, requests };
}

// An installer that drops a recording redisctl where REDISCTL_UNMANAGED_INSTALL
// points, the way the release installer does, and talks on stdout like it. It
// records the install-location settings it was given.
function installerScript(version = VERSION) {
  const fake = fakeRedisctl(0, { dir: path.join(tempDir(), 'payload'), version });
  const settings = path.join(tempDir(), 'settings');
  return {
    fake,
    settings,
    script: `#!/bin/sh
printf '%s|%s|%s|%s' "$REDISCTL_UNMANAGED_INSTALL" "\${REDISCTL_INSTALL_DIR:-}" "\${CARGO_DIST_FORCE_INSTALL_DIR:-}" "\${REDISCTL_NO_MODIFY_PATH:-}" > "${settings}"
echo "installing to $REDISCTL_UNMANAGED_INSTALL"
mkdir -p "$REDISCTL_UNMANAGED_INSTALL" && cp "${fake.dir}/redisctl" "$REDISCTL_UNMANAGED_INSTALL/redisctl"
`,
  };
}

// This package at a release version. The checked-in package.json says 0.0.0, and
// no installed copy is older than that.
function packageAt(version) {
  const dir = tempDir();
  fs.cpSync(path.dirname(WRAPPER), path.join(dir, 'bin'), { recursive: true });
  fs.writeFileSync(path.join(dir, 'package.json'), JSON.stringify({ ...require('../package.json'), version }));
  return dir;
}

function packed(t, dir = path.join(__dirname, '..')) {
  const work = tempDir();
  t.after(() => fs.rmSync(work, { recursive: true, force: true }));
  const npmCache = { npm_config_cache: path.join(work, 'cache'), npm_config_update_notifier: 'false' };
  const result = spawnSync('npm', ['pack', '--silent', '--pack-destination', work], {
    cwd: dir,
    encoding: 'utf8',
    env: { ...process.env, ...npmCache },
  });
  assert.strictEqual(result.status, 0, result.stderr);
  // Only node and npx: the host's npx folder can also hold a real redisctl (Homebrew).
  const npxDir = path.join(work, 'npx-bin');
  fs.mkdirSync(npxDir);
  const npx = process.env.PATH.split(path.delimiter).map((d) => path.join(d, 'npx')).find((f) => fs.existsSync(f));
  fs.symlinkSync(fs.realpathSync(npx), path.join(npxDir, 'npx'));
  fs.symlinkSync(process.execPath, path.join(npxDir, 'node'));
  return {
    tarball: path.join(work, result.stdout.trim()),
    npxDir,
    npmEnv: { ...npmCache, npm_config_offline: 'true' },
  };
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

test('global flags before init still get -y mapped after init', () => {
  const { dir, argsFile } = fakeRedisctl(0);
  const argv = ['--config-file', 'x.toml', '-vp', 'dev', '-o=json', '--query=a', 'init', '-y'];
  assert.strictEqual(run(argv, dir).status, 0);
  assert.deepStrictEqual(childArgs(argsFile), [...argv.slice(0, -1), '--defaults']);
});

test('a global flag value named init is not the subcommand', () => {
  const { dir, argsFile } = fakeRedisctl(0);
  for (const argv of [
    ['-p', 'init', 'profile', 'list', '-y'],
    ['--profile', 'init', 'profile', 'list', '--yes'],
    ['-q', 'init', 'profile', 'list', '-y'],
  ]) {
    assert.strictEqual(run(argv, dir).status, 0);
    assert.deepStrictEqual(childArgs(argsFile), argv);
  }
});

test('flag-only tokens before init and attached short values are skipped', () => {
  const { dir, argsFile } = fakeRedisctl(0);
  assert.strictEqual(run(['-v', 'init', '--yes'], dir).status, 0);
  assert.deepStrictEqual(childArgs(argsFile), ['-v', 'init', '--defaults']);
  assert.strictEqual(run(['-pinit', 'profile', '-y'], dir).status, 0);
  assert.deepStrictEqual(childArgs(argsFile), ['-pinit', 'profile', '-y']);
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
  // Under `npm exec --package=@redis/redisctl`, npm exports npm_config_package to
  // every descendant; redisctl's own npm/npx calls (client install, skills add)
  // would then resolve THIS package instead of their real target. User npm
  // config (registry, proxy) must survive.
  const { dir, envFile } = fakeRedisctl(0);
  const result = spawnSync(process.execPath, [WRAPPER, 'init', '--dry-run'], {
    encoding: 'utf8',
    env: testEnv(dir, {
      npm_config_package: '/tmp/redis-redisctl-0.0.0.tgz',
      npm_config_call: 'redisctl',
      npm_config_registry: 'https://registry.example',
    }),
  });
  assert.strictEqual(result.status, 0);
  const env = JSON.parse(fs.readFileSync(envFile, 'utf8'));
  assert.strictEqual(env.npm_config_package, undefined);
  assert.strictEqual(env.npm_config_call, undefined);
  assert.strictEqual(env.npm_config_registry, 'https://registry.example');
});

test('a failed download gets the install hint, not a stack trace', async (t) => {
  const { url } = await releaseServer(t);
  const result = await runAsync(['init', '--dry-run'], testEnv('/bin:/usr/bin', { REDISCTL_RELEASES_URL: url }));
  assert.strictEqual(result.status, 1);
  assert.match(
    result.stderr,
    new RegExp(`could not download ${url}/redisctl-v${VERSION}/redisctl-installer\\.sh: HTTP 404\\n`)
  );
  assert.match(result.stderr, /brew install redis\/homebrew-tap\/redisctl\n/);
  assert.match(result.stderr, /cargo install redisctl\n/);
  assert.match(result.stderr, /https:\/\/github\.com\/redis\/redisctl\/releases\n/);
  assert.doesNotMatch(result.stderr, /at (Object|Module)\./);
});

test('with no redisctl the release installer runs, then its binary runs the command', async (t) => {
  const { fake, settings, script } = installerScript();
  const { url, requests } = await releaseServer(t, script);
  const env = testEnv('/bin:/usr/bin', { REDISCTL_RELEASES_URL: url });
  const installDir = path.join(env.XDG_DATA_HOME, 'redisctl', 'bin');
  const result = await runAsync(['init', '--dry-run'], env);
  assert.strictEqual(result.status, 0, result.stderr);
  assert.deepStrictEqual(requests, [`/redisctl-v${VERSION}/redisctl-installer.sh`]);
  assert.deepStrictEqual(childArgs(fake.argsFile), ['init', '--dry-run']);
  assert.ok(fs.existsSync(path.join(installDir, 'redisctl')));
  assert.match(result.stderr, new RegExp(`redisctl is not installed - installing redisctl ${VERSION}\\n`));
  assert.match(
    result.stderr,
    new RegExp(`redisctl ${VERSION} installed to ${installDir} - add it to PATH to run redisctl directly\\n`)
  );
  // The installer's chatter goes to stderr: stdout belongs to redisctl (-o json).
  assert.strictEqual(result.stdout, '');
  assert.match(result.stderr, /installing to /);
});

test('the installer gets one folder of its own and never edits PATH', async (t) => {
  const { settings, script } = installerScript();
  const { url } = await releaseServer(t, script);
  const env = testEnv('/bin:/usr/bin', {
    REDISCTL_RELEASES_URL: url,
    REDISCTL_INSTALL_DIR: '/elsewhere',
    CARGO_DIST_FORCE_INSTALL_DIR: '/elsewhere',
  });
  const result = await runAsync(['init'], env);
  assert.strictEqual(result.status, 0, result.stderr);
  // The installer reads REDISCTL_UNMANAGED_INSTALL as a flat install that skips
  // its PATH and shell-profile edits; the two other settings would win over it.
  assert.strictEqual(
    fs.readFileSync(settings, 'utf8'),
    `${path.join(env.XDG_DATA_HOME, 'redisctl', 'bin')}|||`
  );
});

test('the installer does not see the npm bin entry as a redisctl', async (t) => {
  const { fake, script } = installerScript();
  const seen = path.join(tempDir(), 'seen');
  const { url } = await releaseServer(t, `${script}command -v redisctl > "${seen}" || echo none > "${seen}"\n`);
  const npmBin = path.join(tempDir(), 'node_modules', '.bin');
  fs.mkdirSync(npmBin, { recursive: true });
  fs.symlinkSync(WRAPPER, path.join(npmBin, 'redisctl'));
  const env = testEnv(`${npmBin}:${path.dirname(process.execPath)}:/bin:/usr/bin`, { REDISCTL_RELEASES_URL: url });
  const result = await new Promise((resolve) => {
    const child = spawn(path.join(npmBin, 'redisctl'), ['init'], { env });
    child.on('close', (status) => resolve({ status }));
  });
  assert.strictEqual(result.status, 0);
  assert.deepStrictEqual(childArgs(fake.argsFile), ['init']);
  assert.strictEqual(fs.readFileSync(seen, 'utf8').trim(), 'none');
});

test('a redisctl an earlier run or the official installer left is used even when PATH lacks it', async (t) => {
  const { url, requests } = await releaseServer(t);
  for (const where of [(env) => path.join(env.XDG_DATA_HOME, 'redisctl', 'bin'), (env) => path.join(env.CARGO_HOME, 'bin')]) {
    const env = testEnv('/bin:/usr/bin', { REDISCTL_RELEASES_URL: url });
    const installed = fakeRedisctl(0, { dir: where(env) });
    const result = await runAsync(['init', '--dry-run'], env);
    assert.strictEqual(result.status, 0, result.stderr);
    assert.deepStrictEqual(childArgs(installed.argsFile), ['init', '--dry-run']);
  }
  assert.deepStrictEqual(requests, []);
});

test('a copy in the wrapper folder at another version is replaced by this release', async (t) => {
  const { fake, script } = installerScript();
  const { url, requests } = await releaseServer(t, script);
  const env = testEnv('/bin:/usr/bin', { REDISCTL_RELEASES_URL: url });
  const installDir = path.join(env.XDG_DATA_HOME, 'redisctl', 'bin');
  const stale = fakeRedisctl(0, { dir: installDir, version: '0.0.1' });
  const result = await runAsync(['profile', 'list'], env);
  assert.strictEqual(result.status, 0, result.stderr);
  assert.deepStrictEqual(requests, [`/redisctl-v${VERSION}/redisctl-installer.sh`]);
  assert.deepStrictEqual(childArgs(fake.argsFile), ['profile', 'list']);
  assert.ok(!fs.existsSync(stale.argsFile), 'the stale copy must not run');
  assert.match(result.stderr, new RegExp(`updating ${installDir}/redisctl to redisctl ${VERSION}\\n`));
});

test('a redisctl the user installed runs at this release or newer; an older one is skipped and kept', () => {
  const wrapper = path.join(packageAt('0.14.0'), 'bin', 'redisctl.js');
  for (const [own, runs] of [['0.14.0', true], ['1.0.0', true], ['0.13.9', false], ['0.9.0', false], ['dev', false]]) {
    const copy = fakeRedisctl(0, { version: own });
    const result = run(['init'], copy.dir, wrapper);
    assert.strictEqual(fs.existsSync(copy.argsFile), runs, `${own}: ${result.stderr}`);
    if (!runs) {
      assert.match(
        result.stderr,
        new RegExp(`${copy.dir}/redisctl is not redisctl 0\\.14\\.0 or newer - installing redisctl 0\\.14\\.0; that copy stays as it is\\n`)
      );
    }
  }
});

test('the installed version floor follows SemVer prerelease and build precedence', () => {
  for (const [release, installed, runs] of [
    ['0.14.0', '0.14.0-rc.1', false],
    ['0.14.0-rc.2', '0.14.0-rc.1', false],
    ['0.14.0-rc.2', '0.14.0-rc.10', true],
    ['0.14.0-rc.10', '0.14.0-rc.2', false],
    ['0.14.0-rc.1', '0.14.0', true],
    ['0.14.0-rc.1', '0.14.0-rc.1', true],
    ['0.14.0-alpha.1', '0.14.0-alpha.beta', true],
    ['0.14.0-alpha.beta', '0.14.0-alpha.1', false],
    ['0.14.0-alpha', '0.14.0-alpha.1', true],
    ['0.14.0-alpha.1', '0.14.0-alpha', false],
    ['0.14.0-rc.A', '0.14.0-rc.a', true],
    ['0.14.0-rc.a', '0.14.0-rc.A', false],
    ['0.14.0+wrapper.1', '0.14.0+native.2', true],
    ['0.14.0-rc.1+wrapper', '0.14.0-rc.1+native', true],
    ['0.14.0', '0.15.0-alpha', true],
    ['0.14.0-rc.9007199254740993', '0.14.0-rc.9007199254740992', false],
    ['0.14.0', '0.14.0garbage', false],
    ['0.14.0', '00.14.0', false],
    ['0.14.0-rc.1', '0.14.0-rc.01', false],
    ['0.14.0-rc.1', '0.14.0-rc..2', false],
  ]) {
    const wrapper = path.join(packageAt(release), 'bin', 'redisctl.js');
    const copy = fakeRedisctl(0, { version: installed });
    const result = run(['init', '--dry-run'], copy.dir, wrapper);
    const label = `${installed} against ${release}: ${result.stderr}`;
    assert.strictEqual(fs.existsSync(copy.argsFile), runs, label);
    assert.strictEqual(result.status, runs ? 0 : 1, label);
    assert.ok(fs.existsSync(path.join(copy.dir, 'redisctl')), 'the installed copy is kept');
    if (runs) assert.deepStrictEqual(childArgs(copy.argsFile), ['init', '--dry-run']);
    else assert.ok(result.stderr.includes(`installing redisctl ${release}`), label);
  }
});

test('an installer that leaves no runnable redisctl exits 1 with the hint', async (t) => {
  for (const [script, reason] of [
    ['#!/bin/sh\nexit 0\n', /installer left no usable redisctl in .*\/bin\n/],
    ['#!/bin/sh\necho "unsupported platform" >&2\nexit 3\n', /unsupported platform\n.*installer failed\n/s],
  ]) {
    const { url } = await releaseServer(t, script);
    const result = await runAsync(['init'], testEnv('/bin:/usr/bin', { REDISCTL_RELEASES_URL: url }));
    assert.strictEqual(result.status, 1, script);
    assert.match(result.stderr, reason);
    assert.match(result.stderr, /Install redisctl another way, then re-run:\n/);
  }
});

test('the npm bin entry is skipped when locating the native binary', (t) => {
  const { dir, argsFile } = fakeRedisctl(0);
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const npmBin = path.join(dir, 'node_modules', '.bin');
  fs.mkdirSync(npmBin, { recursive: true });
  fs.symlinkSync(WRAPPER, path.join(npmBin, 'redisctl'));
  fs.symlinkSync(process.execPath, path.join(npmBin, 'node'));
  const globalPackage = path.join(dir, 'global', 'node_modules', '@redis', 'redisctl');
  const globalBin = path.join(dir, 'global', 'bin');
  fs.mkdirSync(path.join(globalPackage, 'bin'), { recursive: true });
  fs.mkdirSync(globalBin, { recursive: true });
  fs.copyFileSync(WRAPPER, path.join(globalPackage, 'bin', 'redisctl.js'));
  fs.writeFileSync(path.join(globalPackage, 'package.json'), JSON.stringify({ name: '@redis/redisctl' }));
  fs.symlinkSync(path.join(globalPackage, 'bin', 'redisctl.js'), path.join(globalBin, 'redisctl'));
  const guard = path.join(dir, 'guard.cjs');
  fs.writeFileSync(guard, `
const depth = Number(process.env.REDISCTL_TEST_DEPTH || 0) + 1;
if (depth > 2) process.exit(99);
process.env.REDISCTL_TEST_DEPTH = String(depth);
`);
  for (const nativePresent of [true, false]) {
    const result = spawnSync(path.join(npmBin, 'redisctl'), ['init', '--dry-run'], {
      encoding: 'utf8',
      env: testEnv([npmBin, globalBin, ...(nativePresent ? [dir] : [])].join(path.delimiter), {
        NODE_OPTIONS: `--require=${guard}`,
        REDISCTL_TEST_DEPTH: '0',
      }),
    });
    assert.strictEqual(result.status, nativePresent ? 0 : 1, result.stderr);
    if (nativePresent) assert.deepStrictEqual(childArgs(argsFile), ['init', '--dry-run']);
    else assert.match(result.stderr, /redisctl is not installed/);
  }
});

test('npx runs the packed wrapper the way the README does', (t) => {
  const { dir, argsFile, envFile } = fakeRedisctl(0);
  const { tarball, npxDir, npmEnv } = packed(t);
  const result = spawnSync('npx', ['--yes', `--package=${tarball}`, 'redisctl', 'init', '-y'], {
    encoding: 'utf8',
    env: testEnv([dir, npxDir, '/bin'].join(path.delimiter), npmEnv),
  });
  assert.strictEqual(result.status, 0, result.stderr);
  assert.deepStrictEqual(childArgs(argsFile), ['init', '--defaults']);
  assert.strictEqual(JSON.parse(fs.readFileSync(envFile, 'utf8')).npm_config_package, undefined);
});

test('npx with an older redisctl on PATH, like a stale brew install, runs its own release', async (t) => {
  const release = '0.14.0';
  const brew = fakeRedisctl(0, { version: '0.13.0' });
  const { fake, script } = installerScript(release);
  const { url, requests } = await releaseServer(t, script, release);
  const { tarball, npxDir, npmEnv } = packed(t, packageAt(release));
  const env = testEnv([brew.dir, npxDir, '/bin'].join(path.delimiter), { ...npmEnv, REDISCTL_RELEASES_URL: url });
  const npx = (...argv) => runAsync(['--yes', `--package=${tarball}`, 'redisctl', ...argv], env, ['npx']);
  const result = await npx('init', '--dry-run');
  assert.strictEqual(result.status, 0, result.stderr);
  assert.deepStrictEqual(requests, [`/redisctl-v${release}/redisctl-installer.sh`]);
  assert.deepStrictEqual(childArgs(fake.argsFile), ['init', '--dry-run']);
  assert.ok(!fs.existsSync(brew.argsFile), 'the older copy must not run');
  assert.match(
    result.stderr,
    new RegExp(`${brew.dir}/redisctl is not redisctl ${release} or newer - installing redisctl ${release}; that copy stays as it is\\n`)
  );
  // The next run, with the older copy still first on PATH, reuses that install.
  const again = await npx('init', '--defaults');
  assert.strictEqual(again.status, 0, again.stderr);
  assert.strictEqual(requests.length, 1);
  assert.deepStrictEqual(childArgs(fake.argsFile), ['init', '--defaults']);
  assert.ok(!fs.existsSync(brew.argsFile), 'the older copy must not run');
});
