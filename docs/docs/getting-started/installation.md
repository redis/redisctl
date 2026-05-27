# Installation

Multiple ways to install redisctl depending on your platform and preferences.

## Homebrew (Recommended)

The easiest way to install on macOS or Linux:

```bash
brew install redis/homebrew-tap/redisctl
```

To upgrade:

```bash
brew upgrade redisctl
```

## Docker

Run without installing anything:

```bash
docker run ghcr.io/redis/redisctl --help
```

For frequent use, create an alias:

```bash
alias redisctl='docker run --rm -e REDIS_CLOUD_API_KEY -e REDIS_CLOUD_SECRET_KEY ghcr.io/redis/redisctl'
```

See the [Docker guide](docker.md) for more details.

## Cargo (From Source)

If you have Rust installed:

```bash
cargo install redisctl
```

With secure credential storage (OS keyring support):

```bash
cargo install redisctl --features secure-storage
```

## Binary Downloads

Download pre-built binaries from [GitHub Releases](https://github.com/redis/redisctl/releases/latest).

=== "Linux (x86_64)"

    ``` bash
    curl -L https://github.com/redis/redisctl/releases/latest/download/redisctl-x86_64-unknown-linux-gnu.tar.xz | tar xJ
    sudo mv redisctl /usr/local/bin/
    ```

=== "Linux (ARM64)"

    ``` bash
    curl -L https://github.com/redis/redisctl/releases/latest/download/redisctl-aarch64-unknown-linux-gnu.tar.xz | tar xJ
    sudo mv redisctl /usr/local/bin/
    ```

=== "macOS (Intel)"

    ``` bash
    curl -L https://github.com/redis/redisctl/releases/latest/download/redisctl-x86_64-apple-darwin.tar.xz | tar xJ
    sudo mv redisctl /usr/local/bin/
    ```

=== "macOS (Apple Silicon)"

    ``` bash
    curl -L https://github.com/redis/redisctl/releases/latest/download/redisctl-aarch64-apple-darwin.tar.xz | tar xJ
    sudo mv redisctl /usr/local/bin/
    ```

=== "Windows"

    Download the `.zip` file from releases and extract to a directory in your PATH.

## Verify Installation

```bash
redisctl --version
```

Expected output:

```
redisctl 0.7.7
```

## Shell Completions

Generate shell completions for tab completion:

=== "Bash"

    ``` bash
    redisctl completions bash > ~/.local/share/bash-completion/completions/redisctl
    ```

=== "Zsh"

    ``` bash
    redisctl completions zsh > ~/.zfunc/_redisctl
    ```

=== "Fish"

    ``` bash
    redisctl completions fish > ~/.config/fish/completions/redisctl.fish
    ```

=== "PowerShell"

    ``` powershell
    redisctl completions powershell >> $PROFILE
    ```

## Next Steps

- [Quick Start](quickstart.md) - Run your first commands
- [Authentication](authentication.md) - Set up credentials
