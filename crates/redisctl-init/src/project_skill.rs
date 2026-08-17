//! The generated `redis-project-setup` skill: project-specific facts in a skill so
//! they load only for Redis-related prompts. The skill's description is the trigger -
//! no AGENTS.md or CLAUDE.md is ever written or touched.

use std::path::Path;

use crate::change::{Change, Status};
use crate::env::{FileAction, read_for_planning};
use crate::project::Runtime;
use crate::{InitError, SKILLS_DIR};

pub(crate) const NAME: &str = "redis-project-setup";
const NOTE: &str = "project-specific facts, loaded only for Redis-related prompts";

/// Everything the template adapts to, known only after apply.
pub(crate) struct SkillFacts<'a> {
    pub(crate) runtime: Runtime,
    pub(crate) name: Option<&'a str>,
    pub(crate) cloud: Option<&'a crate::CloudFacts>,
    pub(crate) database: bool,
    pub(crate) container: Option<&'a str>,
    pub(crate) products: &'a [crate::WiredProduct],
    pub(crate) skills: &'a [String],
    pub(crate) client_installed: bool,
    pub(crate) cli_available: bool,
    pub(crate) docker: bool,
}

/// The commands the skill offers depend on what this machine can actually run: a
/// native redis-cli, else one inside the project's container, else a throwaway
/// container.
fn handy_commands(facts: &SkillFacts) -> String {
    const NATIVE_INSTALL_HINT: &str =
        "native install: macOS `brew install redis`, Debian/Ubuntu `apt install redis-tools`";
    let pair = |prefix: &str, target: &str| {
        format!(
            "- `{prefix} PING` - connection check\n- `{prefix} MONITOR` - watch live commands while debugging{target}"
        )
    };
    if facts.cli_available {
        return pair("redis-cli -u \"$REDIS_URL\"", "");
    }
    let via_docker = format!(
        "\n- redis-cli is not installed on this machine; the docker forms above work as-is ({NATIVE_INSTALL_HINT})"
    );
    if let Some(container) = facts.container
        && facts.docker
    {
        return pair(
            &format!("docker exec -it {container} redis-cli"),
            &via_docker,
        );
    }
    if facts.docker {
        return pair(
            "docker run --rm -it redis:8-alpine redis-cli -u \"$REDIS_URL\"",
            &via_docker,
        );
    }
    format!(
        "- redis-cli is not installed ({NATIVE_INSTALL_HINT}); once installed: `redis-cli -u \"$REDIS_URL\" PING`\n- Until then, prefer the `redis` MCP server tools for inspection"
    )
}

fn client_hint(facts: &SkillFacts) -> &'static str {
    match (facts.runtime, facts.client_installed) {
        (Runtime::Node, true) => {
            "- The `redis` Node client is installed: `createClient({ url: process.env.REDIS_URL })`."
        }
        (Runtime::Node, false) => {
            "- Node client: `npm install redis`, then `createClient({ url: process.env.REDIS_URL })`."
        }
        (Runtime::Python, true) => {
            "- The `redis` (redis-py) client is installed: `redis.Redis.from_url(os.environ[\"REDIS_URL\"])`."
        }
        (Runtime::Python, false) => {
            "- Python client: `pip install redis`, then `redis.Redis.from_url(os.environ[\"REDIS_URL\"])`."
        }
        (Runtime::Go, true) => {
            "- go-redis is installed: `opt, _ := redis.ParseURL(os.Getenv(\"REDIS_URL\")); rdb := redis.NewClient(opt)`."
        }
        (Runtime::Go, false) => {
            "- Go client: `go get github.com/redis/go-redis/v9`, then `redis.ParseURL(os.Getenv(\"REDIS_URL\"))`."
        }
        (Runtime::Rust, true) => {
            "- The `redis` crate is installed: `redis::Client::open(env::var(\"REDIS_URL\")?)`."
        }
        (Runtime::Rust, false) => {
            "- Rust client: `cargo add redis`, then `redis::Client::open(env::var(\"REDIS_URL\")?)`."
        }
        (Runtime::Java, true) => {
            "- A Redis Java client is already a dependency; connect with `new JedisPooled(System.getenv(\"REDIS_URL\"))` (or your existing client)."
        }
        (Runtime::Java, false) => {
            "- Java client: add Jedis to the build file - Maven: `<dependency><groupId>redis.clients</groupId><artifactId>jedis</artifactId><version>[6.0.0,)</version></dependency>`; Gradle: `implementation 'redis.clients:jedis:6.+'` - then `new JedisPooled(System.getenv(\"REDIS_URL\"))`."
        }
        (Runtime::Unknown, _) => {
            "- Use your language's standard Redis client; connect with the `REDIS_URL` env var."
        }
    }
}

fn db_hint(facts: &SkillFacts) -> String {
    if let (Some(cloud), None) = (facts.cloud, facts.container) {
        return format!(
            "- The database is `{name}` on Redis Cloud: {tier} subscription `{sub}`, database `{db}`; credentials live only in `.env`.",
            name = cloud.name,
            tier = cloud.tier.as_str(),
            sub = cloud.subscription_id,
            db = cloud.database_id,
        );
    }
    match (facts.container, facts.name) {
        (Some(container), _) => format!(
            "- Local database in Docker container `{container}` - `docker start {container}` if it is down; `docker exec -it {container} redis-cli` for a REPL."
        ),
        (None, Some(name)) => format!(
            "- The database is `{name}` (external, e.g. Redis Cloud); credentials live only in `.env`."
        ),
        (None, None) => "- The database is external (not managed by this project); credentials live only in `.env`.".to_string(),
    }
}

/// The control-plane bullet under Tooling; empty for non-cloud databases.
fn control_plane_hint(facts: &SkillFacts) -> String {
    let Some(cloud) = facts.cloud else {
        return String::new();
    };
    let profile = cloud
        .profile
        .as_deref()
        .map(|p| format!(" -p {p}"))
        .unwrap_or_default();
    format!(
        "\n- Control plane (subscription, plan, memory use, modules): `redisctl api cloud get {base}/{sub}/databases/{db}{profile}`. That response contains the database password - never copy it into a file, a log, or chat; `.env` already has it.",
        base = cloud.tier.api_base(),
        sub = cloud.subscription_id,
        db = cloud.database_id,
    )
}

fn skills_hint(facts: &SkillFacts) -> String {
    if facts.skills.is_empty() {
        "- The official redis/agent-skills were not available at init time; only this skill is installed.".to_string()
    } else {
        format!(
            "- Deep-dive skills installed alongside this one: {}. Use them when modeling data, writing queries, tuning connections, or securing the deployment.",
            facts.skills.join(", ")
        )
    }
}

fn pending_line(env_key: &str) -> String {
    format!(
        "- Setup is waiting for `{env_key}`. Replace each placeholder in `.env` yourself, then run `redisctl init --complete`. Never paste a key into agent chat.\n"
    )
}

const AGENT_MEMORY_SDK_HINT: &str = r#"- The SDK reads `__KEY__` from the environment itself, so no secret appears in code (`__PKG__` is installed):
  ```ts
  import { AgentMemory } from '__PKG__';
  const memory = new AgentMemory({ serverURL: process.env.__URL__, storeId: process.env.__ID__ });
  ```
"#;

const AGENT_MEMORY_REST_HINT: &str = r#"- There is no __RUNTIME__ SDK for the managed service yet, so call the REST API with a bearer token - two endpoints cover session memory:
  ```
  POST ${__URL__}/v1/stores/${__ID__}/session-memory/events   # append a turn
  GET  ${__URL__}/v1/stores/${__ID__}/session-memory/<sessionId>   # read it back
  Authorization: Bearer ${__KEY__}
  ```
  Body for an append: `{ sessionId, actorId, role: "USER" | "ASSISTANT", content: [{ text }], createdAt }`.
  Do not reach for PyPI's `agent-memory-client`: that is the client for the self-hosted Agent Memory Server, a different API.
"#;

const AGENT_MEMORY_BULLETS: &str = r#"- Session memory is the running conversation: append each turn with `addSessionEvent` (`sessionId` per conversation, `actorId` per end user, `role` USER or ASSISTANT), then read it back with `getSessionMemory` to rebuild context on the next turn.
- Long-term memory is the durable set of facts: write with the long-term-memory endpoints and retrieve by semantic search, not by key. Use it for what should outlive one conversation; use session memory for the turn-by-turn transcript.
- `sessionId` and `actorId` accept only letters, digits and hyphens: a dot or an underscore is rejected with 400 `invalid-data`, so slugify anything derived from a username or an email.
- Prefer one session per conversation and a stable `actorId` per user - that pairing is what makes memory compound across sessions rather than pile up.
"#;

const LANGCACHE_TS_HINT: &str = r#"  ```ts
  import { LangCache } from '__PKG__';
  const cache = new LangCache({ serverURL: process.env.__URL__, cacheId: process.env.__ID__, apiKey: process.env.__KEY__ });
  ```
"#;

const LANGCACHE_PY_HINT: &str = r#"  ```python
  from langcache import LangCache
  cache = LangCache(server_url=os.environ["__URL__"], cache_id=os.environ["__ID__"], api_key=os.environ["__KEY__"])
  ```
"#;

const LANGCACHE_BULLETS: &str = r#"- The pattern is search-before-generate: search the cache with the user's prompt, return the hit if one comes back above the similarity threshold, and only call the model on a miss - then store that answer.
- It matches on meaning, not on string equality, so tune `similarityThreshold` rather than normalizing prompts by hand. Attach `attributes` (tenant, model, language) when entries must not be shared across those boundaries.
- Cache model answers, never anything user-specific unless the attributes scope it to that user.
"#;

const CONTEXT_RETRIEVER_BULLETS: &str = r#"- A project MCP server named `context-retriever` bridges the remote tools into supported coding agents. Once the placeholder is filled, restart the agent so it reloads the server and discovers the generated tools.
- The agent key defines the surface and access tags. Treat the tool schemas as the business-data contract: inspect them before calling, prefer the narrow generated lookup or search tool, and do not bypass the surface with direct database queries.
"#;

const CONTEXT_RETRIEVER_PY_HINT: &str = r#"- The official client is installed for application-level MCP calls:
  ```python
  import os
  from context_surfaces import UnifiedClient
  context = UnifiedClient(mcp_url=os.environ["__URL__"])
  tools = await context.list_tools(os.environ["__KEY__"])
  ```
"#;

const CONTEXT_RETRIEVER_ANY_HINT: &str = r#"- Application code can use any MCP client with `__URL__` and an `X-API-Key` header read from `__KEY__`; the coding-agent path needs no product SDK.
"#;

const CONTEXT_RETRIEVER_CTL: &str = r#"- Provisioning stays outside the project secret boundary. Install the official CLI with `pipx install redis-context-retriever`, then use `ctxctl auth login`, `ctxctl surface ...`, and `ctxctl agent create ...`. Its login prompts hide credentials and its credential manager stores admin keys outside this repository.
"#;

fn fill_env(template: &str, product: &crate::WiredProduct, pkg: &str, runtime: Runtime) -> String {
    template
        .replace("__PKG__", pkg)
        .replace("__URL__", product.spec.env_url)
        .replace("__ID__", product.spec.env_id.unwrap_or_default())
        .replace("__KEY__", product.spec.env_key)
        .replace("__RUNTIME__", runtime.as_str())
}

fn product_section(facts: &SkillFacts, product: &crate::WiredProduct) -> String {
    let pkg = crate::install::product_package(product.spec.key, facts.runtime);
    let pending = product.pending_env().map(pending_line).unwrap_or_default();
    match product.spec.key {
        crate::ProductKey::AgentMemory => {
            let hint = match pkg {
                Some(pkg) => fill_env(AGENT_MEMORY_SDK_HINT, product, pkg, facts.runtime),
                None => fill_env(AGENT_MEMORY_REST_HINT, product, "", facts.runtime),
            };
            format!(
                "\n## Agent memory (Redis Iris)\n- Store `{id}` at `{url}`. Read all three from the environment - `{env_url}`, `{env_id}`, `{env_key}` are in `.env`; never hardcode the key or the store id.\n{pending}{hint}{AGENT_MEMORY_BULLETS}",
                id = product.id.as_deref().unwrap_or_default(),
                url = product.url(),
                env_url = product.spec.env_url,
                env_id = product.spec.env_id.unwrap_or_default(),
                env_key = product.spec.env_key,
            )
        }
        crate::ProductKey::LangCache => {
            let hint = if facts.runtime == Runtime::Python {
                fill_env(LANGCACHE_PY_HINT, product, "", facts.runtime)
            } else {
                fill_env(
                    LANGCACHE_TS_HINT,
                    product,
                    pkg.unwrap_or("@redis-ai/langcache"),
                    facts.runtime,
                )
            };
            format!(
                "\n## Semantic cache (LangCache)\n- Cache `{id}` at `{url}`. Read all three from the environment - `{env_url}`, `{env_id}`, `{env_key}` are in `.env`; never hardcode the key or the cache id.\n{pending}{hint}{LANGCACHE_BULLETS}",
                id = product.id.as_deref().unwrap_or_default(),
                url = product.url(),
                env_url = product.spec.env_url,
                env_id = product.spec.env_id.unwrap_or_default(),
                env_key = product.spec.env_key,
            )
        }
        crate::ProductKey::ContextRetriever => {
            let client = if pkg.is_some() {
                fill_env(CONTEXT_RETRIEVER_PY_HINT, product, "", facts.runtime)
            } else {
                fill_env(CONTEXT_RETRIEVER_ANY_HINT, product, "", facts.runtime)
            };
            format!(
                "\n## Governed business data (Context Retriever)\n- The scoped agent tools are exposed at `{url}`. Read `{env_url}` and `{env_key}` from `.env`; never use a Context Retriever admin key in application code or agent tooling.\n{pending}{CONTEXT_RETRIEVER_BULLETS}{client}{CONTEXT_RETRIEVER_CTL}",
                url = product.url(),
                env_url = product.spec.env_url,
                env_key = product.spec.env_key,
            )
        }
    }
}

fn product_sections(facts: &SkillFacts) -> String {
    facts
        .products
        .iter()
        .map(|product| product_section(facts, product))
        .collect()
}

/// Appended on every run, product or not: the recommendation policy is the point of
/// `--iris`, and configured projects still need the "do not add more" guardrail.
fn iris_recommendations(facts: &SkillFacts) -> String {
    let intro = if facts.products.is_empty() {
        "No Iris product is configured.".to_string()
    } else {
        format!(
            "Configured Iris products: {}.",
            facts
                .products
                .iter()
                .map(|p| p.label())
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let additional = if facts.products.is_empty() {
        "a product"
    } else {
        "another product"
    };
    format!(
        r#"
## Redis Iris product recommendations
- {intro} Do not add {additional}, dependency, credential, example, or MCP server unless the user asks for it or explicitly approves your recommendation.
- Recommend only from evidence in the project or the user's goal:
  - Agent Memory when an agent must retain conversation state, user preferences, or learned
    facts across turns or restarts.
  - LangCache when semantically repeated model or RAG requests create avoidable latency or
    model cost. Measure or identify that repeat path first.
  - Context Retriever when an agent needs governed, structured access to live business
    entities and relationships instead of direct database access or hand-built tool sprawl.
  - Redis Data Integration when the source of truth is relational and its Redis
    representation must stay current through snapshot plus change-data capture. This is
    infrastructure onboarding, not an application dependency.
- Before recommending a product, state the observed signal, expected benefit, and smallest
  additional project footprint. If the user approves, run only that product's focused
  `redisctl init` flow and preserve the human-only secret boundary.
- Focused flows after approval:
  - Agent Memory: `redisctl init --agent-memory <endpoint> --store <store-id>`.
  - LangCache: `redisctl init --langcache <endpoint> --cache <cache-id>`.
  - Context Retriever: `redisctl init --context-retriever <mcp-endpoint>` with only a scoped
    agent key placed privately in `.env`.
  - Redis Data Integration: guide its source, snapshot, and change-data-capture setup outside
    the application. Onboard only the resulting Redis target database with `--url` or
    `--cloud` when the application needs it directly.
- Never suggest installing the full Iris portfolio by default. Reassess after each focused
  integration because the next product may not be necessary.
"#
    )
}

/// What the frontmatter description names as this project's Redis surface.
fn subjects(facts: &SkillFacts) -> String {
    let mut subjects: Vec<String> = Vec::new();
    if facts.database {
        subjects.push("the database".to_string());
    }
    subjects.extend(facts.products.iter().map(|p| p.label().to_lowercase()));
    if subjects.is_empty() {
        "Iris product discovery".to_string()
    } else {
        subjects.join(", ")
    }
}

fn database_sections(facts: &SkillFacts) -> String {
    if !facts.database {
        return String::new();
    }
    format!(
        r#"
## Connection
- The connection string is `REDIS_URL` in `.env` (placeholder in `.env.example`). Read it from the environment; never hardcode it and never commit `.env`.
{client}
{db}

## Tooling
- An MCP server named `redis` is registered in the project's agent configs (it reads `REDIS_URL` from `.env` at launch). Prefer its tools to inspect keys, search, and manipulate data instead of shelling out.{control_plane}
{skills}

## Conventions
- Key naming: `<app>:<entity>:<id>` (for example `shop:product:42`), with `:` as the namespace separator.
- Every cache key gets a TTL (`SET key value EX 60`); no unbounded cache writes.
- Use `SCAN` for key iteration in application code, never `KEYS`.

## Handy commands
{handy}
"#,
        client = client_hint(facts),
        db = db_hint(facts),
        control_plane = control_plane_hint(facts),
        skills = skills_hint(facts),
        handy = handy_commands(facts),
    )
}

pub(crate) fn content(facts: &SkillFacts) -> String {
    format!(
        r#"---
name: redis-project-setup
description: >-
  Use for any Redis-related work in this project - caching, sessions, queues,
  streams, search, or anything touching a data store or Redis service: where
  credentials live, which services this project uses ({subjects}), client
  setup, conventions, and available tooling.
---

# Redis setup in this project

Set up by `redisctl init`; re-running it is safe and never clobbers existing content.
Every credential lives in `.env` and nowhere else - not in code, not in a committed
file.
{database}{products}{iris}"#,
        subjects = subjects(facts),
        database = database_sections(facts),
        products = product_sections(facts),
        iris = iris_recommendations(facts),
    )
}

/// The plan-time line: the content depends on apply outcomes (which skills landed,
/// whether the client installed), so the preview reports only what happens to the
/// file.
pub(crate) fn preview(cwd: &Path) -> Change {
    let rel = format!("{SKILLS_DIR}/{NAME}/SKILL.md");
    let status = if cwd.join(&rel).exists() {
        Status::Updated
    } else {
        Status::Created
    };
    Change::new(rel, status, NOTE)
}

/// Write the skill and, for Claude Code, mirror the skills CLI's own layout with a
/// per-skill `.claude/skills` symlink.
pub(crate) fn generate(
    cwd: &Path,
    facts: &SkillFacts,
    link_for_claude: bool,
    also_link: &[String],
) -> Result<Vec<Change>, InitError> {
    let rel = format!("{SKILLS_DIR}/{NAME}/SKILL.md");
    let content = content(facts);
    let action = match read_for_planning(cwd, &rel)? {
        Some(existing) if existing == content => FileAction::Unchanged { rel },
        existing => FileAction::Write {
            status: if existing.is_none() {
                Status::Created
            } else {
                Status::Updated
            },
            rel,
            content,
            note: NOTE.to_string(),
        },
    };
    let mut changes = vec![action.perform(cwd)?];
    if link_for_claude {
        changes.extend(link_claude_skill(cwd, NAME)?);
        for name in also_link {
            changes.extend(link_claude_skill(cwd, name)?);
        }
    }
    Ok(changes)
}

/// Claude Code reads `.claude/skills`; the skills CLI symlinks its installs there per
/// skill. Mirror that layout for skills the CLI did not place. No entry is reported
/// when the whole directory is already a symlink - it exposes everything by itself.
#[cfg(unix)]
fn link_claude_skill(cwd: &Path, name: &str) -> Result<Option<Change>, InitError> {
    let parent = cwd.join(".claude/skills");
    if parent.is_symlink() {
        return Ok(None);
    }
    let subject = format!(".claude/skills/{name}");
    let link = parent.join(name);
    let target = std::path::PathBuf::from("../..")
        .join(SKILLS_DIR)
        .join(name);
    match std::fs::symlink_metadata(&link) {
        Ok(meta) if meta.is_symlink() => {
            if std::fs::read_link(&link).ok().as_deref() == Some(&target) {
                return Ok(Some(Change::new(subject, Status::Unchanged, "")));
            }
            std::fs::remove_file(&link).map_err(|e| InitError::WriteFailed {
                rel: subject.clone(),
                message: e.to_string(),
            })?;
            std::os::unix::fs::symlink(&target, &link).map_err(|e| InitError::WriteFailed {
                rel: subject.clone(),
                message: e.to_string(),
            })?;
            Ok(Some(Change::new(
                subject,
                Status::Updated,
                format!("symlink to {SKILLS_DIR}/{name}"),
            )))
        }
        Ok(_) => Ok(Some(Change::new(
            subject,
            Status::Kept,
            "existing entry left untouched",
        ))),
        Err(_) => {
            std::fs::create_dir_all(&parent).map_err(|e| InitError::WriteFailed {
                rel: subject.clone(),
                message: e.to_string(),
            })?;
            std::os::unix::fs::symlink(&target, &link).map_err(|e| InitError::WriteFailed {
                rel: subject.clone(),
                message: e.to_string(),
            })?;
            Ok(Some(Change::new(
                subject,
                Status::Created,
                format!("symlink to {SKILLS_DIR}/{name}"),
            )))
        }
    }
}

#[cfg(not(unix))]
fn link_claude_skill(_cwd: &Path, name: &str) -> Result<Option<Change>, InitError> {
    Ok(Some(Change::new(
        format!(".claude/skills/{name}"),
        Status::Skipped,
        "symlinks are unix-only; Claude Code reads .agents/skills via its own discovery",
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts<'a>(container: Option<&'a str>, skills: &'a [String]) -> SkillFacts<'a> {
        SkillFacts {
            runtime: Runtime::Node,
            name: None,
            cloud: None,
            database: true,
            container,
            products: &[],
            skills,
            client_installed: true,
            cli_available: true,
            docker: true,
        }
    }

    fn cloud_facts(tier: crate::CloudTier, profile: Option<&str>) -> crate::CloudFacts {
        crate::CloudFacts {
            name: "cloud-db".to_string(),
            subscription_id: "1".to_string(),
            database_id: "9".to_string(),
            tier,
            profile: profile.map(str::to_string),
            created: false,
        }
    }

    #[test]
    fn cloud_facts_carry_ids_and_the_control_plane_command() {
        let cloud = cloud_facts(crate::CloudTier::Essentials, None);
        let mut f = facts(None, &[]);
        f.cloud = Some(&cloud);
        let text = content(&f);
        assert!(
            text.contains("`cloud-db` on Redis Cloud: Essentials subscription `1`, database `9`"),
            "{text}"
        );
        assert!(
            text.contains("redisctl api cloud get /fixed/subscriptions/1/databases/9`"),
            "{text}"
        );
        assert!(text.contains("never copy it into a file"), "{text}");
        // Credential-free invariant holds on the cloud path too.
        assert!(!text.contains("redis://"), "{text}");
    }

    #[test]
    fn flexible_databases_use_the_pro_api_path_and_name_the_profile() {
        let cloud = cloud_facts(crate::CloudTier::Flexible, Some("qa"));
        let mut f = facts(None, &[]);
        f.cloud = Some(&cloud);
        let text = content(&f);
        assert!(text.contains("Flexible subscription `1`"), "{text}");
        assert!(
            text.contains("redisctl api cloud get /subscriptions/1/databases/9 -p qa`"),
            "{text}"
        );
    }

    #[test]
    fn content_adapts_to_the_container_and_skills() {
        let skills = vec!["redis-core".to_string(), "redis-search".to_string()];
        let text = content(&facts(Some("redis-init-demo"), &skills));
        assert!(text.contains("docker start redis-init-demo"), "{text}");
        // The connection string lives in .env only; the committed skill never
        // carries a URL.
        assert!(!text.contains("redis://"), "{text}");
        assert!(text.contains("redis-core, redis-search"), "{text}");
        assert!(text.contains("createClient({ url: process.env.REDIS_URL })"));
        assert!(text.contains("redis-cli -u \"$REDIS_URL\" PING"));
    }

    #[test]
    fn content_names_an_external_database_and_missing_skills() {
        let mut f = facts(None, &[]);
        f.name = Some("my-cloud-db");
        f.cli_available = false;
        f.docker = false;
        let text = content(&f);
        assert!(
            text.contains("`my-cloud-db` (external, e.g. Redis Cloud)"),
            "{text}"
        );
        assert!(text.contains("were not available at init time"), "{text}");
        assert!(text.contains("redis-cli is not installed ("), "{text}");
    }

    #[test]
    fn container_without_native_cli_offers_docker_exec() {
        let mut f = facts(Some("redis-init-x"), &[]);
        f.cli_available = false;
        let text = content(&f);
        assert!(
            text.contains("docker exec -it redis-init-x redis-cli PING"),
            "{text}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn generate_writes_the_skill_links_claude_and_reruns_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let changes = generate(dir.path(), &facts(None, &[]), true, &[]).unwrap();
        let statuses: Vec<_> = changes
            .iter()
            .map(|c| (c.subject.as_str(), c.status))
            .collect();
        assert_eq!(
            statuses,
            vec![
                (
                    ".agents/skills/redis-project-setup/SKILL.md",
                    Status::Created
                ),
                (".claude/skills/redis-project-setup", Status::Created),
            ]
        );
        let link = dir.path().join(".claude/skills/redis-project-setup");
        assert!(link.is_symlink());
        assert!(link.join("SKILL.md").exists(), "symlink resolves");

        let rerun = generate(dir.path(), &facts(None, &[]), true, &[]).unwrap();
        assert!(rerun.iter().all(|c| c.status == Status::Unchanged));
    }

    #[cfg(unix)]
    #[test]
    fn a_real_directory_in_claude_skills_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".claude/skills/redis-project-setup")).unwrap();
        let changes = generate(dir.path(), &facts(None, &[]), true, &[]).unwrap();
        let link_change = &changes[1];
        assert_eq!(link_change.status, Status::Kept);
    }

    #[cfg(unix)]
    #[test]
    fn a_whole_dir_symlink_reports_no_link_entry() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
        std::fs::create_dir_all(dir.path().join(".agents/skills")).unwrap();
        std::os::unix::fs::symlink("../.agents/skills", dir.path().join(".claude/skills")).unwrap();
        let changes = generate(dir.path(), &facts(None, &[]), true, &[]).unwrap();
        assert_eq!(changes.len(), 1, "{changes:?}");
    }
}
