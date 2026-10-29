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

## Which redisctl runs

The wrapper bundles no binary. It runs the first `redisctl` it finds on PATH, then
in its own install folder, then in `~/.cargo/bin` (the official installer's
default), skipping its own npm bin entry. For `init`, that copy must have the
`init` command; an older one is skipped and left where it is.

With no usable copy, the wrapper downloads the official installer of the redisctl
release matching its own version, from GitHub releases, and runs it in unmanaged
mode: the installer checks the archive's sha256 (where `sha256sum` exists) and puts
the binary in the wrapper's folder, without editing PATH, shell profiles or the
Windows registry:

- Linux and macOS: `$XDG_DATA_HOME/redisctl/bin` (`~/.local/share/redisctl/bin`)
- Windows: `%LOCALAPPDATA%\redisctl\bin`

The next run finds that copy, and a wrapper of another version replaces it with its
own release; add the folder to PATH to run `redisctl` directly. A copy you installed
yourself is never replaced. The installer's output goes to stderr, so stdout stays
redisctl's (`-o json`). `REDISCTL_RELEASES_URL` replaces
`https://github.com/redis/redisctl/releases/download` for a mirror of the releases.

The installer needs `curl` or `wget` and, on Linux and macOS, a `tar` that reads
`.tar.xz`. Releases cover macOS (arm64, x64), Linux x64 and Windows x64.

If the install fails, the wrapper prints the other ways to install and exits 1:

```bash
brew install redis/homebrew-tap/redisctl
cargo install redisctl
# or a binary from https://github.com/redis/redisctl/releases
```

It never runs redisctl through a shell, so pasted connection URLs with `&`, `|`,
`^` or spaces stay single arguments on every platform.

## Publishing

The release workflow publishes this package for each `redisctl-v*` release, after
the GitHub release exists, with `version` set to the release version: the wrapper
downloads the installer of the release with its own version. The tarball carries
the repository LICENSE files. The job signs in to npm with
[trusted publishing](https://docs.npmjs.com/trusted-publishers), so no npm token
is stored, and npm adds provenance.

npm takes a trusted publisher only for a package that exists, so the job skips
until the first version is on npm. Set it up just before a release, since a
release between steps 1 and 2 fails the npm job:

1. From an account that can publish to `@redis`, in this directory, publish a
   released version by hand:

   ```sh
   cp ../../LICENSE-MIT ../../LICENSE-APACHE .
   npm pkg set version=<released version>
   npm publish --access public
   git checkout package.json && rm LICENSE-MIT LICENSE-APACHE
   ```

2. In the `@redis/redisctl` settings on npmjs.com, add a GitHub Actions trusted
   publisher: organization `redis`, repository `redisctl`, workflow
   `release.yml`. npm checks the workflow that calls `publish-npm.yml`.
3. Release within 2 days. A trusted publisher that publishes nothing in 2 days
   expires and must be added again.

Test the wrapper itself with `npm pack` +
`npx --yes --package=./redis-redisctl-0.0.0.tgz redisctl init --dry-run` against
a `redisctl` on PATH (that `--yes` belongs to npx, not the wrapper).
