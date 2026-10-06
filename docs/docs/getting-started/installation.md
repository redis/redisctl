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

The examples below install **redisctl 0.12.1** from its
[CLI release](https://github.com/redis/redisctl/releases/tag/redisctl-v0.12.1).
For another version, choose its `redisctl-v<VERSION>` release and matching assets.
Avoid the repository-wide "latest" link: this workspace releases the CLI and MCP
server separately, so "latest" may point to the other application.

Archives contain a top-level platform directory. These commands extract it into
a temporary directory before installing the binary.

=== "Linux (x86_64)"

    ``` bash
    install_dir="$(mktemp -d)"
    curl -fL https://github.com/redis/redisctl/releases/download/redisctl-v0.12.1/redisctl-x86_64-unknown-linux-gnu.tar.xz -o "$install_dir/redisctl.tar.xz"
    tar -xJf "$install_dir/redisctl.tar.xz" --strip-components=1 -C "$install_dir"
    sudo install -m 755 "$install_dir/redisctl" /usr/local/bin/redisctl
    ```

=== "Linux (ARM64)"

    Version 0.12.1 does not provide a pre-built Linux ARM64 archive. Build from
    source with [Cargo](#cargo-from-source), or use the Linux ARM64 image described
    in the [Docker guide](docker.md).

=== "macOS (Intel)"

    ``` bash
    install_dir="$(mktemp -d)"
    curl -fL https://github.com/redis/redisctl/releases/download/redisctl-v0.12.1/redisctl-x86_64-apple-darwin.tar.xz -o "$install_dir/redisctl.tar.xz"
    tar -xJf "$install_dir/redisctl.tar.xz" --strip-components=1 -C "$install_dir"
    sudo install -m 755 "$install_dir/redisctl" /usr/local/bin/redisctl
    ```

=== "macOS (Apple Silicon)"

    ``` bash
    install_dir="$(mktemp -d)"
    curl -fL https://github.com/redis/redisctl/releases/download/redisctl-v0.12.1/redisctl-aarch64-apple-darwin.tar.xz -o "$install_dir/redisctl.tar.xz"
    tar -xJf "$install_dir/redisctl.tar.xz" --strip-components=1 -C "$install_dir"
    sudo install -m 755 "$install_dir/redisctl" /usr/local/bin/redisctl
    ```

=== "Windows"

    Download the [Windows x86_64 archive](https://github.com/redis/redisctl/releases/download/redisctl-v0.12.1/redisctl-x86_64-pc-windows-msvc.zip),
    extract it, and add the directory containing `redisctl.exe` to your PATH.

## Verify Installation

```bash
redisctl --version
```

For the 0.12.1 binary download examples above, the expected output is:

```
redisctl 0.12.1
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
