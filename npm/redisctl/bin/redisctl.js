#!/usr/bin/env node
// `npx @redis/redisctl init` -> `redisctl init`. The wrapper owns npm muscle
// memory (-y/--yes after `init` -> --defaults); redisctl owns everything else.
// Exit codes forward verbatim (0 success / 1 failure / 2 usage / 6 invalid input
// / 10 connection / 12 cancelled).
'use strict';

const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

const INSTALL_HINT = `redisctl is not installed. Install it with one of:
    brew install redis/homebrew-tap/redisctl
    cargo install redisctl
    https://github.com/redis/redisctl/releases
  Then re-run, or call redisctl directly.`;

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

// Never through a shell: a pasted connection URL can carry & | ^ %, which
// cmd.exe would run as syntax. Windows resolves .exe from PATH without one.
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
// npx puts this wrapper's own bin entry first on PATH.
const command = (process.env.PATH || '')
  .split(path.delimiter)
  .map((dir) => path.resolve(dir, binaryName))
  .find((candidate) => {
    try {
      fs.accessSync(candidate, fs.constants.X_OK);
      return fs.statSync(candidate).isFile() && !isWrapper(fs.realpathSync(candidate));
    } catch {
      return false;
    }
  });

if (!command) {
  console.error(INSTALL_HINT);
  process.exit(1);
}

// npm exec exports its flags as npm_config_* to every descendant; the package
// pinning (`--package=@redis/redisctl`) would make redisctl's own npm/npx calls
// (client install, skills add) resolve THIS package instead of their real
// target. Drop only the resolution-changing keys - user npm config such as
// registry and proxy must survive.
const env = { ...process.env };
delete env.npm_config_package;
delete env.npm_config_call;

const result = spawnSync(command, args, { stdio: 'inherit', env });

if (result.error && result.error.code === 'ENOENT') {
  console.error(INSTALL_HINT);
  process.exit(1);
}
if (result.signal) {
  // Die the way the child did, so shells see the real interrupt.
  process.kill(process.pid, result.signal);
}
process.exit(result.status ?? 1);
