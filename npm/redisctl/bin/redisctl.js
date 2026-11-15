#!/usr/bin/env node
// `npx @redis/redisctl init` -> `redisctl init`. The wrapper owns npm muscle
// memory (-y/--yes after `init` -> --defaults) and installs redisctl when no
// usable copy exists; redisctl owns everything else. Exit codes forward verbatim.
'use strict';

const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { version } = require('../package.json');

const INSTALL_HINT = `Install redisctl another way, then re-run:
    brew install redis/homebrew-tap/redisctl
    cargo install redisctl
    https://github.com/redis/redisctl/releases`;

// redisctl's global flags that take a value, so `-p init` is not read as the
// subcommand.
const VALUE_FLAGS = new Set(['--profile', '--config-file', '--output', '--query']);
const VALUE_SHORTS = new Set(['p', 'o', 'q']);

function subcommandIndex(argv) {
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--') return i + 1;
    if (arg.startsWith('--')) {
      if (VALUE_FLAGS.has(arg)) i++;
    } else if (arg.startsWith('-') && arg.length > 1) {
      // In a short cluster (-vp dev, -pdev) the first value flag takes the rest
      // of the token, or the next token when nothing follows it.
      const cluster = arg.slice(1);
      if ([...cluster].findIndex((c) => VALUE_SHORTS.has(c)) === cluster.length - 1) i++;
    } else {
      return i;
    }
  }
  return argv.length;
}

// -y/--yes only means "take the defaults" for init; other subcommands keep
// their argv untouched.
function mapArgs(argv) {
  const sub = subcommandIndex(argv);
  if (argv[sub] !== 'init') return argv;
  return argv.map((arg, i) => (i > sub && (arg === '-y' || arg === '--yes') ? '--defaults' : arg));
}

const args = mapArgs(process.argv.slice(2));

// npm exec exports its flags as npm_config_* to every descendant; the package
// pinning (`--package=@redis/redisctl`) would make redisctl's own npm/npx calls
// (client install, skills add) resolve THIS package instead of their real
// target. Drop only the resolution-changing keys - user npm config such as
// registry and proxy must survive.
const env = { ...process.env };
delete env.npm_config_package;
delete env.npm_config_call;

const binaryName = process.platform === 'win32' ? 'redisctl.exe' : 'redisctl';
const wrapperPath = fs.realpathSync(__filename);
function isWrapper(file) {
  if (file === wrapperPath) return true;
  try {
    const manifest = path.resolve(path.dirname(file), '..', 'package.json');
    return JSON.parse(fs.readFileSync(manifest, 'utf8')).name === '@redis/redisctl';
  } catch {
    return false;
  }
}

const installDir =
  process.platform === 'win32'
    ? path.join(process.env.LOCALAPPDATA || path.join(os.homedir(), 'AppData', 'Local'), 'redisctl', 'bin')
    : path.join(process.env.XDG_DATA_HOME || path.join(os.homedir(), '.local', 'share'), 'redisctl', 'bin');
// The official installer's default folder.
const cargoBin = path.join(process.env.CARGO_HOME || path.join(os.homedir(), '.cargo'), 'bin');

// npx puts this wrapper's own bin entry first on PATH.
function candidates(dirs) {
  const files = dirs.map((dir) => path.resolve(dir, binaryName));
  return [...new Set(files)].filter((candidate) => {
    try {
      fs.accessSync(candidate, fs.constants.X_OK);
      return fs.statSync(candidate).isFile() && !isWrapper(fs.realpathSync(candidate));
    } catch {
      return false;
    }
  });
}

function owned(command) {
  return path.dirname(command) === installDir;
}

function parseVersion(text) {
  const match = /^redisctl (0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/.exec(text);
  if (!match) return null;
  const prerelease = match[4] ? match[4].split('.') : [];
  if (prerelease.some((part) => /^0\d+$/.test(part))) return null;
  return { core: match.slice(1, 4).map(BigInt), prerelease };
}

function compareVersions(left, right) {
  for (let i = 0; i < left.core.length; i++) {
    if (left.core[i] !== right.core[i]) return left.core[i] < right.core[i] ? -1 : 1;
  }
  // A release is newer than any prerelease with the same core version.
  if (!left.prerelease.length || !right.prerelease.length) {
    if (left.prerelease.length === right.prerelease.length) return 0;
    return left.prerelease.length ? -1 : 1;
  }
  for (let i = 0; i < Math.min(left.prerelease.length, right.prerelease.length); i++) {
    const a = left.prerelease[i];
    const b = right.prerelease[i];
    if (a === b) continue;
    const aNumeric = /^\d+$/.test(a);
    const bNumeric = /^\d+$/.test(b);
    if (aNumeric && bNumeric) return BigInt(a) < BigInt(b) ? -1 : 1;
    if (aNumeric !== bNumeric) return aNumeric ? -1 : 1;
    return a < b ? -1 : 1;
  }
  return Math.sign(left.prerelease.length - right.prerelease.length);
}
const ours = parseVersion(`redisctl ${version}`);

// The wrapper's own copy follows the wrapper's version, since nothing else updates
// it. A copy the user installed runs at this version or newer, so
// `npx @redis/redisctl@X` never runs a redisctl older than X.
function usable(command) {
  const ran = spawnSync(command, ['--version'], { encoding: 'utf8', env });
  const reported = ran.status === 0 ? ran.stdout.trim() : '';
  if (owned(command)) return reported === `redisctl ${version}`;
  const theirs = parseVersion(reported);
  return !!theirs && !!ours && compareVersions(theirs, ours) >= 0;
}

function fail(message) {
  console.error(message);
  console.error(INSTALL_HINT);
  process.exit(1);
}

// The release installer checks the archive's sha256 where sha256sum exists.
async function install() {
  const file = process.platform === 'win32' ? 'redisctl-installer.ps1' : 'redisctl-installer.sh';
  const base = process.env.REDISCTL_RELEASES_URL || 'https://github.com/redis/redisctl/releases/download';
  const url = `${base}/redisctl-v${version}/${file}`;
  let script;
  try {
    const response = await fetch(url);
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    script = await response.text();
  } catch (e) {
    fail(`could not download ${url}: ${e.cause?.message ?? e.message}`);
  }
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'redisctl-install-'));
  const saved = path.join(dir, file);
  fs.writeFileSync(saved, script);
  const [shell, shellArgs] =
    process.platform === 'win32'
      ? ['powershell', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', saved]]
      : ['sh', [saved]];
  // Without this wrapper's bin dirs, which the installer would report as a
  // redisctl shadowing the one it installs.
  const PATH = (env.PATH || '')
    .split(path.delimiter)
    .filter((dir) => {
      try {
        return !isWrapper(fs.realpathSync(path.resolve(dir, binaryName)));
      } catch {
        return true;
      }
    })
    .join(path.delimiter);
  // An unmanaged install is flat and skips the installer's PATH, shell-profile and
  // registry edits; the two other location settings would win over it.
  const installerEnv = { ...env, PATH, REDISCTL_UNMANAGED_INSTALL: installDir };
  delete installerEnv.REDISCTL_INSTALL_DIR;
  delete installerEnv.CARGO_DIST_FORCE_INSTALL_DIR;
  // Its progress goes to stderr: stdout belongs to redisctl (-o json).
  const ran = spawnSync(shell, shellArgs, { stdio: ['ignore', 2, 2], env: installerEnv });
  fs.rmSync(dir, { recursive: true, force: true });
  if (ran.status !== 0) fail(`the redisctl ${version} installer failed`);
  const installed = candidates([installDir]).find(usable);
  if (!installed) fail(`the redisctl ${version} installer left no usable redisctl in ${installDir}`);
  console.error(`redisctl ${version} installed to ${installDir} - add it to PATH to run redisctl directly`);
  return installed;
}

async function main() {
  const found = candidates([...(process.env.PATH || '').split(path.delimiter), installDir, cargoBin]);
  let command = found.find(usable);
  if (!command) {
    console.error(
      found.length === 0
        ? `redisctl is not installed - installing redisctl ${version}`
        : owned(found[0])
          ? `updating ${found[0]} to redisctl ${version}`
          : `${found[0]} is not redisctl ${version} or newer - installing redisctl ${version}; that copy stays as it is`
    );
    command = await install();
  }

  // Never through a shell: a pasted connection URL can carry & | ^ %, which
  // cmd.exe would run as syntax.
  const result = spawnSync(command, args, { stdio: 'inherit', env });

  if (result.error && result.error.code === 'ENOENT') fail(`${command} could not be started`);
  if (result.signal) {
    // Die the way the child did, so shells see the real interrupt.
    process.kill(process.pid, result.signal);
  }
  process.exit(result.status ?? 1);
}

main().catch((e) => fail(e.message));
