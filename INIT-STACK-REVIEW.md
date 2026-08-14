# Init stack review handoff

**Do not commit this file.** It is a local review note for the agent that will
fix the findings. Delete it when the work is done.

## Goal

Each stacked PR must review clean **on its own unique diff** (`base...head`).
Do **not** dump fixes on the tip. A reviewer of #1100 should not still see
the dry-run restart bug, and #1102 should not be the PR that secretly fixes
#1100's `free_port`.

Fixes land on the PR that **introduced** the bug. Patches that already live
in a later slice get **moved down** into the introducing PR, then the later
slice's unique diff drops them on rebase.

---

## Stack

Bottom → top. Each PR targets the previous slice, not `main`.

| PR | Branch | Base | Slice |
|---|---|---|---|
| #1093 | `feat/init/1-skeleton` | `feat/init-command` | hidden `init` skeleton |
| #1098 | `feat/init/2-engine-crate` | `feat/init/1-skeleton` | `redisctl-init` crate |
| #1100 | `feat/init/3-docker-database` | `feat/init/2-engine-crate` | Docker plan/apply |
| #1101 | `feat/init/4-project-wiring` | `feat/init/3-docker-database` | .env.example, client SDK, redis-cli |
| #1102 | `feat/init/5-skills-install` | `feat/init/4-project-wiring` | official skills |
| #1103 | `feat/init/6-project-skill` | `feat/init/5-skills-install` | generated skill + Claude symlinks |
| #1104 | `feat/init/7-mcp` | `feat/init/6-project-skill` | MCP registration, unhide command |
| #1105 | `feat/init/8-wizard` | `feat/init/7-mcp` | wizard + `--defaults` |

Repo: `/Users/vasil.atanasov/.supacode/repos/redisctl/feat/init-command`

Nothing has been posted to GitHub.

---

## Prompt for the implementing agent

Copy everything in this fenced block:

```
You are implementing review fixes for the redisctl `init` PR stack. Read
INIT-STACK-REVIEW.md in the repo root and follow it exactly.

Goal: every stacked PR's unique diff (git diff <base>...HEAD) reviews clean.
Fix each finding on the branch that introduced it, then rebase the rest of
the stack up. Do not pile fixes onto feat/init/8-wizard.

How:
- Work bottom-up: #1093 → #1098 → #1100 → #1101 → #1102 → #1103 → #1104 → #1105.
- On each branch, land only that PR's items (including MOVE-DOWN items).
- Commit on that branch (Conventional Commits, scope init/cli as listed).
- Rebase the next branch onto the updated parent:
    git checkout <child>
    git rebase <parent>
  Prefer --force-with-lease when pushing the rewritten feature branches.
  Never force-push main, feat/init-command, or any protected branch.
- After rebase, the child must not still contain the hunks you moved down.
  Check with: git diff <parent>...HEAD
- Use git worktrees if it helps (one per branch); do not bounce the user's
  current checkout if you can avoid it. Do not create extra feature branches.

Constraints:
- Do not post GitHub review comments, PR comments, or review replies.
- Do not retarget PRs.
- Fix only the items listed per PR. No drive-by refactors.
- Follow AGENTS.md: cargo fmt --all, cargo clippy --all-targets --all-features -- -D warnings,
  no unwrap() in production.
- Tests: add a regression for every MUST item. Prefer unit tests in
  redisctl-init; CLI tests for output/masking. Negative whole-stdout asserts
  use s3cret, not pw.
- Credentials: any echo of user-supplied URL/paste text goes through mask_url.
- Do not modify INIT-STACK-REVIEW.md except to check off items you finished.

After each PR: fmt + clippy + cargo test -p redisctl-init and
cargo test -p redisctl --test init_cli_tests (on that branch).
When the whole stack is restacked, report per PR: commits added, unique-diff
sanity (no leaked later-slice fixes, no remaining listed bugs), and what you
verified.
```

---

## Restack procedure

Do this after each slice's commits, before starting the next slice.

```bash
# example after finishing feat/init/3-docker-database:
git checkout feat/init/4-project-wiring
git rebase feat/init/3-docker-database
# resolve conflicts by keeping the parent version of moved-down hunks
git push --force-with-lease
```

Then the next child, all the way to `feat/init/8-wizard`.

`#1098` has no new findings — rebase it onto the updated `#1093` so the
JSON/YAML tip fix stays the only error-surface change in `#1098`.

Push each feature branch with `--force-with-lease` after its rebase so the
open PRs update. Do not push `main` or `feat/init-command`.

**Unique-diff check** (must pass before leaving a PR):

```bash
git diff origin/feat/init/<parent-branch>...HEAD --stat
```

The child's unique files should be that slice's feature, plus its own fixes,
minus any hunks that now belong to the parent.

---

## MOVE-DOWN (currently fixed in a later PR — put them where they belong)

These are the stacked-hygiene defects. The later PR should not be the one
that “also happens to fix” the earlier bug.

| Bug | Introduced | Currently sitting in | Action |
|---|---|---|---|
| `free_port` IPv4-only, misses Docker Desktop `[::]` | #1100 | #1102 `docker.rs` `port_is_free` | Re-apply on #1100. After rebase, #1102's unique diff must not be the `free_port` fix. |
| Client install used process cwd (`sh` not `sh_in`) | #1101 | #1102 `install.rs` / `util.rs` | Re-apply on #1101 (`perform` takes cwd, `sh_in`). After rebase, #1102 should only *use* `sh_in` for skills, not introduce it for client installs. |
| Skills `--dry-run` always said `npx` even with `--skills-repo` | #1102 | #1105 `skills.rs` `preview()` | Re-apply on #1102 (`would copy skills from …` + unit test). After rebase, #1105 must not own that hunk. |
| Invalid `--url` printed JSON/YAML file-format tips | #1093 | #1098 | **Leave.** The tip fix is an error-boundary change that belongs with the crate split (`From<InitError>`). Do not move it back into #1093. |

When re-applying a moved-down fix, copy the **current tip behavior** (it is
already correct at HEAD). Do not re-invent a weaker version.

---

## Work by PR

### #1093 `feat/init/1-skeleton` — SHOULD

Josh's two comments (mask rejected URL, unquoted paste) are already fixed in
`fd6631a`. Do not reopen. JSON/YAML tips stay in #1098 (see MOVE-DOWN).

#### S1. `Debug` / `trace!` dumps `--url` — DONE (manual Debug redacts url/pasted + regression test)

**Where:**
- `crates/redisctl/src/cli/init.rs` `InitArgs` derives `Debug`
- `crates/redisctl/src/main.rs` `execute_command`:
  `trace!("Executing command: {:?}", cli.command)`
- `format_command` already redacts init (`"init [args redacted]"`)

`-vvv` / `RUST_LOG=trace` still dumps the raw password. The info log is fine.

**Fix:** custom `Debug` for `InitArgs` that omits or masks `url`/`pasted`,
**or** log `format_command` instead of `{:?}`. A `"<redacted>"` field is
enough; do not pull `mask_url` into `cli` if that creates a layering mess.

At this SHA, `InitArgs` only has `url` / `pasted` / `name` / `agents` /
`dry_run`. Implement against *this* struct, not the tip's extra flags.
Later rebases will carry the `Debug` impl forward; you may need to add
fields as later slices introduce them (keep them redacted too).

**Test:** `format!("{:?}", args)` does not contain the secret.

**Commit:** `fix(cli): stop tracing raw init URLs`

---

### #1098 `feat/init/2-engine-crate` — rebase only

No new findings. Rebase onto updated `#1093`. Confirm CLI tests still pass
and the JSON/YAML tip omission is still in this unique diff.

---

### #1100 `feat/init/3-docker-database`

#### M1. `--dry-run` hides the common container restart — MUST — DONE

**Where:** `crates/redisctl-init/src/docker.rs`
- `DatabaseAction::preview`: `ExistingEnv` → `None`
- `apply_database` ExistingEnv arm: still `docker start`s when `restart` is set

After the first run, `.env` has `REDIS_URL`, so a stopped container is
`ExistingEnv { restart: Some(name) }`. `--dry-run` hides the only mutation
that path performs. `StartExisting` previews correctly but is almost
unreachable once `.env` exists.

**Fix:** if `restart` is `Some(name)`, `preview()` should return `Planned` /
“would start existing container” (same as `StartExisting`).

**Test:** unit test on `preview()` for `ExistingEnv { restart: Some(..), .. }`.

**Commit:** `fix(init): preview docker restart on the existing-.env path`

#### M2. Restart is reported as success even when it failed — SHOULD — DONE

**Where:** `crates/redisctl-init/src/docker.rs` ExistingEnv apply

`docker start` status is ignored; `wait_for_ping` is `let _ = …`; the change
is always `Updated` / “restarted stopped container”. `StartExisting` correctly
returns `DockerCommand`.

**Fix:** if start fails, omit the change (or status `Unchanged` + note) and
let validation tell the truth; do not claim a restart.

**Commit:** `fix(init): do not report a failed container restart as updated`

#### M3. Unauthenticated Redis published on all host interfaces — SHOULD — DONE

**Where:** `RunNew` apply (`-p {port}:6379`) and the matching `preview()` string.

Image has no `requirepass`. On Wi-Fi/LAN that is a writable Redis.

**Fix:** `-p 127.0.0.1:{port}:6379`. Update the preview string. Keep
`REDIS_URL` as `redis://localhost:{port}`.

**Commit:** `fix(init): bind the local Redis container to loopback`

#### M4. MOVE-DOWN: `free_port` dual-stack probe — MUST (belongs here, currently in #1102) — DONE

At this SHA, `free_port` is IPv4-loopback-only
(`TcpListener::bind(("127.0.0.1", p))`). Docker Desktop publishes via `[::]`,
so 6379 looks free, `docker run -p 6379:6379` fails, and init never walks
6380–6478.

**Fix:** copy the tip's `port_is_free` (probe `127.0.0.1`, `0.0.0.0`, and `::`;
treat only `AddrInUse` as taken). Bring the unit test that holds `("::", 0)`.

**Commit:** `fix(init): probe dual-stack listeners when picking a free port`

---

### #1101 `feat/init/4-project-wiring`

#### N1. `redis-cli` reinstalls every run if it landed in `~/.local/bin` — MUST — DONE

**Where:** `crates/redisctl-init/src/install.rs`
- `decide_redis_cli` only checks `has("redis-cli")` (PATH)
- `perform` already has the `~/.local/bin` branch

After the official installer falls back to `~/.local/bin`, the next plan is
`InstallCli` again — `curl | sh` every `redisctl init`.

**Fix:** in `decide_redis_cli` (and the post-install check), treat
`~/.local/bin/redis-cli` as present — `Unchanged` with the PATH hint.

**Test:** inject a `has` seam and/or a fake `~/.local/bin/redis-cli` so a
second plan is `Unchanged`, not `InstallCli`.

**Commit:** `fix(init): treat ~/.local/bin/redis-cli as already installed`

#### N2. Live Docker tests can `curl | sh` redis-cli — SHOULD — DONE

**Where:** `crates/redisctl/tests/init_docker_tests.rs`

The suite dropped `package.json` so it cannot `npm install`, but `install_cli`
still defaults true.

**Fix:** `--no-install-cli` on every live `init` invocation in that file.

**Commit:** `test(init): skip redis-cli install in the live Docker suite`

#### N3. MOVE-DOWN: client installs must run in plan cwd (`sh_in`) — MUST (currently in #1102) — DONE

At this SHA, `Command::perform` takes `cwd` but runs `sh(cmd, &args)` in the
**process** cwd. The CLI today sets `cwd = current_dir()`, so `cd && redisctl init`
works; any caller that plans a different directory (tests, later MCP) mutates
the wrong tree.

**Fix:** copy the tip's `sh_in` (if it does not yet exist on this branch, add
it in `util.rs` here — that is the right home). Run client commands with
`current_dir(cwd)`. Keep the unit test that `touch` only succeeds in the
plan cwd.

**Commit:** `fix(init): run client install commands in the plan cwd`

---

### #1102 `feat/init/5-skills-install`

After rebase, drop duplicate `free_port` / `sh_in`-for-clients hunks. Skills
may still call `sh_in`; they should not be the commit that *introduced* it.

#### P1. Solo-Claude skills layout reports the wrong tree — MUST — DONE

**Where:** `crates/redisctl-init/src/skills.rs`

- `fallback_dir` puts a Claude-only checkout in `.claude/skills/`, but the
  project subject is always `{SKILLS_DIR}/{name}/` (`.agents/skills/…`).
- Unmanaged Kept/Updated only looks at `.agents/skills`. Solo Claude skips
  the unmanaged warning and can report **Updated** for a skill the installer
  left in place.

Tests only cover Claude+Codex.

**Fix:** report the actual destination. Scan both `target_dirs`; compare
SKILL.md from the probed path. Add a Claude-only unit test.

**Commit:** `fix(init): report Claude-only skill installs against the real path`

#### P2. MOVE-DOWN: skills dry-run must name the checkout — MUST (currently in #1105) — DONE

At this SHA, `preview()` always emits `would run: npx …`. `perform()` copies
from `repo` whenever `skills_repo` / `$REDISCTL_INIT_SKILLS_REPO` is set.

**Fix:** branch in `preview()` the same way as `perform()`:
`would copy skills from {repo}` vs the npx command. Add
`preview_names_the_checkout_when_one_is_given`.

**Commit:** `fix(init): dry-run skills checkout instead of pretending to npx`

`GENERATED_SKILL = "redis-project-setup"` in this slice with no generator yet
is a minor leak into #1103. Leave it unless the rebase makes it trivial to
defer; do not spend a detour on it.

---

### #1103 `feat/init/6-project-skill`

#### Q1. Leftover local container poisons the generated skill — MUST — DONE (incl. nice-to-have asserts)

**Where:**
- `crates/redisctl-init/src/docker.rs` `plan_local_database`
- `crates/redisctl-init/src/project_skill.rs` `db_hint`

`ExistingEnv.container` is `Some` whenever `docker inspect` finds
`redis-init-<slug>`, while `restart` is correctly gated on a localhost URL.
`db_hint` treats any `Some(container)` as “this project’s local Docker DB”
and wins over `--name`.

Re-run after moving `.env` to Cloud (container still present): the skill says
`docker start` / `docker exec` against the leftover instance. `--url`
(Provided) does not have this bug — only the existing-`.env` path.

**Fix:**

```rust
let local = url.contains("localhost") || url.contains("127.0.0.1");
let restart = info.as_ref().is_some_and(|i| !i.running && local);
let container = info.filter(|_| local).map(|_| name);
```

**Test:** `.env` has a remote URL + a matching leftover container →
`container()` is `None` → skill uses the external/Cloud hint, not `docker exec`.

**Commit:** `fix(init): ignore leftover containers when .env points elsewhere`

Nice-to-have (same PR if cheap): assert generated `SKILL.md` does not contain
`redis://` / `s3cret`; assert AGENTS.md is not *created* when absent.

---

### #1104 `feat/init/7-mcp`

Docs page is born here. `--defaults` does not exist yet — do **not** add it
to the options table in this PR (that is #1105).

#### R1. MCP tests assume Claude without pinning `--agent` — MUST — DONE

**Where:**
- `crates/redisctl/tests/init_cli_tests.rs` `dry_run_detects_a_node_project_and_plans_the_env_wiring`
- `crates/redisctl/tests/init_docker_tests.rs` `full_run_provisions_validates_and_rerun_is_unchanged`

Neither passes `--agent`. A machine with only Cursor writes `.cursor/mcp.json`;
CI with empty detection lucks out and writes all four.

**Fix:** `--agent claude` (or `all`) in every test that asserts `.mcp.json`.
Audit both test files.

**Commit:** `test(init): pin --agent in MCP assertions`

#### R2. Docs claim nothing is ever overwritten — MUST — DONE

**Where:** `docs/docs/getting-started/init.md` (~“nothing is ever overwritten”)

True for `.env`, not for MCP: a different existing `redis` server is
**replaced** (`Updated`), old command masked in the note.

**Fix:** say env/skill files are kept; a different `redis` MCP entry is replaced.

**Commit:** `docs(cli): describe MCP replace vs env kept`

#### R3. Docker MCP fallback cannot reach host Redis on Linux — SHOULD — DONE

**Where:** `crates/redisctl-init/src/mcp.rs` `server_entry`

Rewrites `localhost` / `127.0.0.1` to `host.docker.internal` without
`--add-host=host.docker.internal:host-gateway`. Linux Engine does not define
that name. Unanchored `sed s/localhost/.../` can rewrite passwords and
`foo.localhost`.

**Fix:** add `--add-host=host.docker.internal:host-gateway`. Prefer host-only
rewrites (`://localhost`, `://127.0.0.1`).

**Test:** launcher string contains the add-host flag and does not use a bare
`s/localhost/`.

**Commit:** `fix(init): make the docker MCP launcher work on Linux`

Codex-only runs still claiming a project MCP server in the skill/epilogue is
minor; fix here if you are already in `project_skill.rs` / the epilogue.

---

### #1105 `feat/init/8-wizard`

After rebase, drop the skills-preview hunk (now in #1102). Add `--defaults`
to the docs table here — the flag is born in this slice.

#### T1. Wizard confirmation reprints a pasted password — MUST — DONE

**Where:** `crates/redisctl/src/workflows/init/wizard.rs`
`format_input_prompt_selection`

Forwards `sel` unchanged. Flag-path errors and the plan line use `mask_url`.

**Fix:** `self.format_select_prompt_selection(f, prompt, &engine::mask_url(sel))`.
`mask_url` is a no-op on strings without `user:pass@`.

**Test:** a selection of `redis://default:s3cret@host:6379` renders `****`
and does not contain `s3cret`.

**Commit:** `fix(init): mask pasted URLs in the wizard confirmation`

#### T2. Docs missing `--defaults` — MUST — DONE

**Where:** `docs/docs/getting-started/init.md` options table

**Fix:** add `--defaults` (takes defaults instead of asking; piped stdin never
prompts).

**Commit:** `docs(cli): document --defaults`

#### T3. Wizard Esc tells the user to pass `--force` — SHOULD — DONE

**Where:** `crates/redisctl/src/error.rs` `RedisCtlError::Cancelled` tips

Init has no `--force`. Do not change cancel tips for other commands that
really do use `--force`.

**Fix:** specialize tips when the prompt is a wizard question, or point at
`--defaults` / a TTY re-run.

**Commit:** `fix(init): stop suggesting --force when the wizard is cancelled`

#### T4. `piped_stdin_never_prompts` is TTY-dependent — SHOULD — DONE

**Where:** `crates/redisctl/src/workflows/init/wizard.rs` tests

`applies()` reads the **test process** stdin. `cargo test` from a terminal
inherits a TTY, so this fails locally and only passes in CI.

**Fix:** drop the ambient-TTY assertion. Add `defaults: true` ⇒ `!applies`
and empty `pending` ⇒ `!applies`. Keep child-process coverage (assert_cmd
already pipes stdin).

**Commit:** `test(init): make the wizard applies() tests TTY-proof`

---

## Out of scope / do not flag again

- `#1093` / `#1098` already approved; Josh's two #1093 comments are done.
- Missing cloud option / “skip no database” in the wizard — deferred by design.
- MCP `serde_json` sorted keys — accepted deviation.
- `redisctl-init` unpublished-crate vs `cargo publish` — #1098 body, not this pass.
- Clone-cache fallback from the PoC — deliberately not ported.
- Esc on paste `Input` vs Select/MultiSelect (Ctrl+C → 130) — minor, skip.

---

## Verification (every slice, then once at the tip)

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test -p redisctl-init
cargo test -p redisctl --test init_cli_tests
```

Per-PR unique diff must not contain another slice's feature, and must not
still contain that PR's listed bugs.

```bash
# example
git diff feat/init/2-engine-crate...feat/init/3-docker-database --stat
```
