# @redis/init

> **Not yet self-contained:** until the `redisctl` binary npm package ships as a
> dependency, this wrapper requires `redisctl` on PATH (see the checklist below).

Onboard a project to Redis services and make its AI coding agent Redis-fluent:

```bash
npx @redis/init
```

This package is a thin wrapper over [`redisctl init`](https://github.com/redis/redisctl):
it maps npm muscle memory (`-y`/`--yes`) to `redisctl`'s `--defaults`, inherits the
terminal (the wizard and banner work), and forwards exit codes verbatim
(0 success / 2 usage / 6 validation / 10 network / 12 cancelled). Every flag after
`npx @redis/init` goes to `redisctl init` unchanged.

Note the position of `-y`: `npx @redis/init -y` reaches the wrapper (and becomes
`--defaults`); `npx -y @redis/init` is npx's own skip-prompt flag and never
reaches it. Both are fine - they just answer different questions.

## How the binary is found

`redisctl` resolves like any command: a dependency-shipped binary in the install's
`node_modules/.bin` wins (npx puts it first on PATH), otherwise whatever `redisctl`
is already installed. Until the first release with `init` ships, that means the
branch build:

```bash
cargo install --git https://github.com/redis/redisctl --branch feat/init-command redisctl
```

Without one, the wrapper prints that install line and exits 1. It never runs
through a shell, so pasted connection URLs with `&`, `|`, `^` or spaces stay
single arguments on every platform.

## Publishing checklist (blocked on org decisions)

1. Add `"npm"` to `installers` in the workspace `[workspace.metadata.dist]` and
   rerun `dist generate --mode ci`, so every release also publishes the `redisctl`
   binary package (name/scope: open question).
2. Add that package here as an exact-version dependency (lockstep with the binary),
   and set this package's `version` from the release pipeline.
3. Swap the install hint (in `bin/redis-init.js` and above) to the released
   channels (`brew install redis/homebrew-tap/redisctl`, `cargo install redisctl`),
   and probe for the `init` subcommand - a pre-init 0.11.x binary on PATH exits 2,
   which the hint must explain.
4. Include the repository LICENSE files in the tarball (`files` currently ships
   `bin/` only).
5. `npm publish --tag alpha` from the release workflow for a dress rehearsal;
   promote to `latest` when the init stack ships.

Do not publish before step 1-2 exist: `npx @redis/init` on a clean machine must
work end to end, not exit with an install hint.

Until then, test the wrapper itself with `npm pack` +
`npx --yes --package=./redis-init-0.0.0.tgz redis-init --dry-run` against a
`redisctl` on PATH (that `--yes` belongs to npx, not the wrapper).
