# Contributing

How to contribute to redisctl.

## Getting Started

### Prerequisites

- Rust 1.90+ (edition 2024)
- Git

### Clone and Build

```bash
git clone https://github.com/redis/redisctl
cd redisctl
cargo build
```

### Run Tests

```bash
cargo test --workspace --all-features
```

### Run Lints

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

## Development Workflow

### 1. Create a Branch

```bash
git checkout -b feat/my-feature
# or
git checkout -b fix/my-fix
```

Branch prefixes:
- `feat/` - New features
- `fix/` - Bug fixes
- `docs/` - Documentation
- `refactor/` - Code refactoring
- `test/` - Test improvements

### 2. Make Changes

Follow the existing code style. Key patterns:

- Use `anyhow` for CLI errors, `thiserror` for library errors
- All commands must support `-o json` output
- Add tests for new functionality
- Destructive commands must use long-only `--force` to skip confirmation; do not assign `-f`

### 3. Test Locally

```bash
# Run all tests
cargo test --workspace --all-features

# Run specific test
cargo test test_database_list

# Run with output
cargo test -- --nocapture
```

### 4. Commit

Use conventional commits:

```bash
git commit -m "feat: add database backup command"
git commit -m "fix: handle empty response in subscription list"
git commit -m "docs: add VPC peering guide"
```

Types: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`

### 5. Push and Create PR

```bash
git push origin feat/my-feature
```

Then create a PR on GitHub.

## Project Structure

```
crates/
├── redisctl-core/      # Core library - config, workflows, and shared logic
├── redisctl/           # CLI
│   ├── src/commands/   # Command implementations
│   ├── src/workflows/  # Multi-step workflows
│   └── tests/          # CLI tests
└── redisctl-mcp/       # MCP server
```

**External dependencies** (separate repositories):
- [redis-cloud](https://github.com/redis-developer/redis-cloud-rs) - Cloud API client
- [redis-enterprise](https://github.com/redis-developer/redis-enterprise-rs) - Enterprise API client

## Adding a New Command

1. Add command enum in `crates/redisctl/src/cli/mod.rs` (top-level) or the relevant domain file: `src/cli/cloud.rs` (Cloud), `src/cli/enterprise.rs` (Enterprise)
2. Implement handler in `crates/redisctl/src/commands/` (subdirs: `cloud/`, `enterprise/`)
3. Add to command routing in `crates/redisctl/src/main.rs`
4. Add tests in `crates/redisctl/tests/`
5. Update documentation

When adding a destructive command, add its command path to the `DESTRUCTIVE_COMMANDS` inventory in
`crates/redisctl/src/main.rs`. The command-tree tests require every listed command to expose the
canonical long-only `--force` bypass and reject a canonical `--yes` flag. Do not add new
compatibility aliases; the hidden `--yes` and `-y` aliases exist only for commands that exposed them
before this convention was adopted.

## Testing

### Unit Tests

```rust
#[test]
fn test_parse_database_id() {
    let id = parse_database_id("123:456").unwrap();
    assert_eq!(id.subscription, 123);
    assert_eq!(id.database, 456);
}
```

### Integration Tests with Mocks

```rust
#[tokio::test]
async fn test_database_list() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/subscriptions/123/databases"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"databaseId": 1, "name": "test"}
        ])))
        .mount(&mock_server)
        .await;

    // Test against mock
}
```

## Documentation

- User docs: `docs/docs/`
- API docs: Inline rustdoc comments
- Build docs: `cargo doc --workspace`

## Questions?

- [GitHub Issues](https://github.com/redis/redisctl/issues)
- [GitHub Discussions](https://github.com/redis/redisctl/discussions)
