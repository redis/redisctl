#!/usr/bin/env node
// `npx @redis/init` -> `redisctl init`. The wrapper owns npm muscle memory
// (-y/--yes -> --defaults); redisctl owns everything else. Exit codes forward
// verbatim (0 success / 2 usage / 6 validation / 10 network / 12 cancelled).
'use strict';

const { spawnSync } = require('node:child_process');

// Until the first release ships init, brew/cargo installs are 0.11.x and the
// only working install is the branch build (swap this at GA - see the README
// publishing checklist).
const INSTALL_HINT = `redisctl is not installed.
  Install the branch build:
    cargo install --git https://github.com/redis/redisctl --branch feat/init-command redisctl
  Then re-run, or call it directly: redisctl init`;

const args = process.argv
  .slice(2)
  .map((arg) => (arg === '-y' || arg === '--yes' ? '--defaults' : arg));

// Never through a shell: a pasted connection URL can carry & | ^ %, which
// cmd.exe would run as syntax. Windows resolves .exe from PATH without one.
const command = process.platform === 'win32' ? 'redisctl.exe' : 'redisctl';

// npm exec exports its flags as npm_config_* to every descendant; the package
// pinning (`--package=@redis/init`) would make redisctl's own npm/npx calls
// (client install, skills add) resolve THIS package instead of their real
// target. Drop only the resolution-changing keys - user npm config such as
// registry and proxy must survive.
const env = { ...process.env };
delete env.npm_config_package;
delete env.npm_config_call;

const result = spawnSync(command, ['init', ...args], { stdio: 'inherit', env });

if (result.error && result.error.code === 'ENOENT') {
  console.error(INSTALL_HINT);
  process.exit(1);
}
if (result.signal) {
  // Die the way the child did, so shells see the real interrupt.
  process.kill(process.pid, result.signal);
}
process.exit(result.status ?? 1);
