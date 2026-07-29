# Contributing to redisctl

Thank you for your interest in contributing to redisctl! This guide will help you get started.

## Code of Conduct

Please be respectful and constructive in all interactions.

## How to Contribute

### Finding Good First Issues

New to the project? Look for issues labeled [`good first issue`](https://github.com/redis/redisctl/labels/good%20first%20issue).

These issues are:
- Well-defined with clear acceptance criteria
- Include implementation hints and code pointers
- Have existing patterns you can follow
- Are great for learning the codebase

**Current good first issues:**
- **#138** - Add pager support for Windows (cross-platform development)
- **#426** - Add payment-method commands (CLI wrapper for existing API)

Each issue has detailed guidance in the comments. Don't hesitate to ask questions!

### Reporting Issues

- Check existing issues first to avoid duplicates
- Use issue templates when available
- Include steps to reproduce for bugs
- Provide context and use cases for feature requests

### Development Setup

1. **Fork and clone the repository**
   ```bash
   git clone https://github.com/YOUR_USERNAME/redisctl.git
   cd redisctl
   ```

2. **Install Rust toolchain (1.90+)**
   ```bash
   rustup update stable
   ```

3. **Run tests**
   ```bash
   cargo test --workspace
   cargo clippy --all-targets --all-features -- -D warnings
   cargo fmt --all -- --check
   ```

4. **Start local Redis Enterprise for testing**
   ```bash
   docker compose up -d
   ```

### Making Changes

1. **Create a feature branch**
   ```bash
   git checkout -b feat/your-feature-name
   # or for fixes:
   git checkout -b fix/issue-description
   ```

2. **Make your changes**
   - Follow existing code style and patterns
   - Add tests for new functionality
   - Update documentation as needed
   - Keep commits focused and atomic

3. **Use conventional commits**
   ```bash
   git commit -m "feat: add new command for X"
   git commit -m "fix: resolve issue with Y"
   git commit -m "docs: update README with Z"
   ```

   Commit types:
   - `feat`: New feature
   - `fix`: Bug fix
   - `docs`: Documentation only
   - `style`: Code style changes (formatting)
   - `refactor`: Code refactoring
   - `test`: Test additions or fixes
   - `chore`: Maintenance tasks

4. **Ensure all checks pass**
   ```bash
   cargo fmt --all
   cargo clippy --all-targets --all-features -- -D warnings
   cargo test --workspace
   ```

### Submitting Pull Requests

1. **Push your branch**
   ```bash
   git push origin feat/your-feature-name
   ```

2. **Create a Pull Request**
   - Use a clear, descriptive title
   - Reference any related issues
   - Describe what changes you made and why
   - Include test results if relevant

3. **Review Process**
   - Address review feedback
   - Keep PR up to date with main branch
   - Be patient - maintainers are volunteers

## Development Guidelines

### Code Style

- Run `cargo fmt` before committing
- Follow Rust naming conventions
- Write clear, self-documenting code
- Add comments for complex logic

### Testing

- Write unit tests for new functions
- Add integration tests for new commands
- Test both success and error cases
- Mock external API calls

### Documentation

- Update README.md for user-facing changes
- Add doc comments for public APIs
- Include examples in documentation
- Update CLI help text

### API Design

- Keep interfaces consistent and ergonomic
- Prefer typed request/response models; use untyped `serde_json::Value` only for
  intentionally opaque or multipart-style payloads (e.g., module uploads) or
  highly heterogeneous aggregates
- Use builder patterns for complex configurations
- Handle errors gracefully with context (`anyhow::Context`, `thiserror`)
- Where multiple API versions exist (v1/v2), expose explicit versioned sub‑handlers,
  for example `actions.v1()` and `actions.v2()`

### Working Style (PR Flow)

1. Create a feature/fix branch (e.g., `feat/...`, `fix/...`).
2. Open a Draft PR early. Push incremental, focused commits (use Conventional Commits).
3. Keep the PR body up to date with scope and a short checklist.
4. Ensure CI passes locally (`fmt`, `clippy -D warnings`, `test`) and in GitHub.
5. When complete and green, mark the PR “Ready for review”.
6. Prefer “Squash and merge” to produce a clean, single commit on `main`.
7. Reference and close related issues in the PR description.

## Release Process

We use a manual release workflow with semantic versioning:

1. **Trigger Release Workflow**
   - Go to Actions → "Manual Release" workflow
   - Click "Run workflow"
   - Select version bump type (patch/minor/major)
   - Optionally run in dry-run mode first

2. **Automated Steps**
   - Updates all crate versions in workspace
   - Creates git commit and tag
   - Publishes to crates.io
   - Triggers binary builds via cargo-dist
   - Publishes Docker images

3. **Version Management**
   - All workspace crates are versioned in lockstep
   - Tags follow format: `v{major}.{minor}.{patch}`
   - Conventional commits guide version selection

## Getting Help

- Open a [Discussion](https://github.com/redis/redisctl/discussions) for questions
- Check existing [Issues](https://github.com/redis/redisctl/issues)
- Review [Documentation](https://docs.rs/redisctl)

## Recognition

Contributors are recognized in:
- GitHub contributors graph
- Release notes
- Project documentation

Thank you for contributing to redisctl!
