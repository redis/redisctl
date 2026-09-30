# @redis/redisctl

Onboard a project to Redis services and make its AI coding agent Redis-fluent:

```bash
npx @redis/redisctl init
```

This package is a thin wrapper over [`redisctl init`](https://github.com/redis/redisctl):
it maps npm muscle memory (`-y`/`--yes` after `init`) to `redisctl`'s `--defaults`, inherits the
terminal (the wizard and banner work), and forwards exit codes verbatim
(0 success / 1 failure / 2 usage / 6 invalid input / 10 connection / 12 cancelled). Everything after
`npx @redis/redisctl` goes to `redisctl` unchanged, so other subcommands work too.

Note the position of `-y`: `npx @redis/redisctl init -y` reaches the wrapper (and
becomes `--defaults`); `npx -y @redis/redisctl init` is npx's own skip-prompt flag
and never reaches it. Both are fine - they just answer different questions.

## Requirements

The wrapper does not bundle the binary: it runs the `redisctl` already on PATH
(skipping its own npm bin entry). Install it with any of:

```bash
brew install redis/homebrew-tap/redisctl
cargo install redisctl
# or a binary from https://github.com/redis/redisctl/releases
```

Without one, the wrapper prints those install options and exits 1. It never runs
through a shell, so pasted connection URLs with `&`, `|`, `^` or spaces stay
single arguments on every platform.

## Publishing checklist

1. Add `"npm"` to `installers` in the workspace `[workspace.metadata.dist]` and
   rerun `dist generate --mode ci`, so every release also publishes the `redisctl`
   binary package.
2. Add that package here as an exact-version dependency (lockstep with the binary),
   and set this package's `version` from the release pipeline.
3. Probe for the `init` subcommand: a `redisctl` on PATH older than `init` exits 2,
   which the install hint must explain.
4. Include the repository LICENSE files in the tarball (`files` ships `bin/` only).
5. `npm publish --tag alpha` from the release workflow for a dress rehearsal,
   then promote to `latest`.

Do not publish before steps 1-2 exist: `npx @redis/redisctl init` on a clean
machine must work end to end, not exit with an install hint.

Test the wrapper itself with `npm pack` +
`npx --yes --package=./redis-redisctl-0.0.0.tgz redisctl init --dry-run` against
a `redisctl` on PATH (that `--yes` belongs to npx, not the wrapper).
