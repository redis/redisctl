# Agent Error Codes

The agent-native surface — `cloud auth login | status | logout` and `cloud workflow
quick-database | database-credentials` — reports failures in a **machine-branchable** form so
scripts and agents can branch on a stable code instead of parsing prose.

## The contract

In `-o json` / `-o yaml` mode, a failure on these commands prints an error envelope to
**stdout** and exits with a mapped code:

```json
{ "status": "error", "error": { "code": "free_db_exists", "message": "…", "retryable": true } }
```

- `code` — a stable, append-only identifier. Branch on this.
- `message` — human, actionable, and always safe to show (never contains secrets).
- `retryable` — whether re-running the same command unchanged might succeed.

Human (non-JSON) mode is unchanged: the usual diagnostic goes to **stderr** and the process
keeps today's `0` / `1` exit behavior.

## Exit codes

| Exit | Class | What the caller should do |
|------|-------|---------------------------|
| `1` | unknown / backend failure | Treat as a bug or an unexpected backend state; inspect `message`. |
| `2` | usage / precondition | Fix the input or state (bad name, not signed in), then retry. |
| `3` | transient / retryable | Back off and retry the same command. |
| `4` | quota / limit reached | A limit was hit; nothing to retry without changing the account. |

`retryable` is `true` for exactly the exit-`3` class.

## Code inventory

| Code | Exit | Retryable | Meaning |
|------|------|-----------|---------|
| `not_authenticated` | 2 | no | No usable credentials for the profile. Run `redisctl cloud auth login`. |
| `auth_denied` | 2 | no | The sign-in request was denied. |
| `keyring_unavailable` | 2 | no | No OS keyring available to store the key. Re-run with `--allow-plaintext`. |
| `migration_required` | 2 | no | The account signs in with a password and must be linked to Google/GitHub once in the Redis Cloud console. |
| `mfa_required` | 2 | no | The account requires multi-factor authentication and there was no terminal to prompt on. Re-run `cloud auth login` interactively. |
| `mfa_invalid_code` | 2 | no | The multi-factor code was rejected. Start login again. |
| `invalid_name` | 2 | no | The database name doesn't match the naming rules (see below). |
| `name_conflict` | 2 | no | The name maps to a conflicting existing resource. |
| `device_code_expired` | 3 | yes | The device login code expired before approval. Start login again. |
| `auth_pending` | 3 | yes | Login hasn't been approved yet. Approve it and retry. |
| `transient_api_error` | 3 | yes | A transient backend error. Retry. |
| `rate_limited` | 3 | yes | Too many requests. Back off and retry. |
| `free_db_exists` | 4 | no | The account already has a free database. |
| `quota_exceeded` | 4 | no | An account quota or limit was reached. |
| `mfa_quota_exceeded` | 4 | no | Too many multi-factor attempts. Wait before trying again. |
| `sm_exchange_failed` | 1 | no | The sign-in / key-mint exchange failed unexpectedly. |
| `unknown` | 1 | no | Unclassified failure; inspect `message`. |

The list is append-only: existing codes never change meaning or exit class, so a branch written
today keeps working.

!!! note "Database naming"
    `quick-database` names must match `^[a-z][a-z0-9-]{1,38}[a-z0-9]$` — 3–40 characters,
    lowercase letters / digits / hyphens, starting with a letter, no leading, trailing, or
    doubled hyphen. A violation is reported as `invalid_name` (exit `2`) before any API call.

## Branching example

```bash
out=$(redisctl cloud workflow quick-database --name my-app -o json) || true
case "$(printf '%s' "$out" | jq -r '.error.code // empty')" in
  "")                               echo "ready" ;;                       # success
  not_authenticated)                redisctl cloud auth login ;;          # then retry
  transient_api_error|rate_limited) sleep 5; exec "$0" ;;                 # back off
  free_db_exists)                   echo "account already has a free DB" ;;
  *)                                printf '%s\n' "$out" >&2; exit 1 ;;
esac
```

## See also

- [Authentication commands](../cloud/commands/auth.md)
- [Cloud workflows](../cloud/workflows.md) — `quick-database`, `database-credentials`
