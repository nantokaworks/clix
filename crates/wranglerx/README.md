# wranglerx

[![CI](https://github.com/nantokaworks/clix/actions/workflows/ci.yml/badge.svg)](https://github.com/nantokaworks/clix/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/nantokaworks/clix?filter=wranglerx-*&label=wranglerx)](https://github.com/nantokaworks/clix/releases?q=wranglerx-)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**Automatically switch Cloudflare Wrangler accounts based on the directory you're in.**

> **Prerequisite:** `wranglerx` is a wrapper around [`wrangler`](https://developers.cloudflare.com/workers/wrangler/). Install Wrangler first and register each Cloudflare token with `wranglerx auth add`.

If you work across personal, work, or client Cloudflare accounts, `wranglerx` lets each project carry the account context. It detects `account_id` from `wrangler.toml` or `wrangler.jsonc`, falls back to the GitHub remote owner for explicit mappings, and runs `wrangler` with the matching `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID`.

## Installation

### Homebrew (macOS / Linux)

```bash
brew install nantokaworks/tap/wranglerx
```

### Cargo (all platforms)

```bash
cargo install --git https://github.com/nantokaworks/clix wranglerx
```

### Binary download

Pre-built binaries for macOS, Linux, and Windows are available on the [Releases](https://github.com/nantokaworks/clix/releases) page.

## Usage

Run `wranglerx` the same way you would run `wrangler`:

```bash
wranglerx deploy
wranglerx dev
wranglerx whoami
```

Bootstrap commands are forwarded without account resolution:

```bash
wranglerx help
wranglerx login
wranglerx logout
```

Version commands print the `wranglerx` banner:

```bash
wranglerx version
wranglerx --version
```

Use `--dry-run` to inspect the selected account without running `wrangler`:

```bash
wranglerx --dry-run deploy
```

## Config

Register accounts in `~/.config/wranglerx/accounts.yml` with `wranglerx auth`:

```bash
wranglerx auth add personal '${WRANGLERX_TOKEN_PERSONAL}' --account-id 1234abcd
wranglerx auth add acme '${WRANGLERX_TOKEN_ACME}'
wranglerx auth list
wranglerx auth remove acme
```

The config file shape is:

```yaml
accounts:
  personal:
    api_token: ${WRANGLERX_TOKEN_PERSONAL}
    account_id: 1234abcd
  acme:
    api_token: ${WRANGLERX_TOKEN_ACME}
mappings:
  1234abcd: personal
  acme-org: acme
```

Resolution order:

1. If `CLOUDFLARE_ACCOUNT_ID` is already set, `wranglerx` passes through to `wrangler` unchanged.
2. Otherwise, `wranglerx` walks up from the current directory and reads top-level `account_id` from the nearest `wrangler.toml` or `wrangler.jsonc`.
3. If no project account id is found, `wranglerx` falls back to the GitHub owner from `git remote get-url origin`.
4. The trigger key is matched against `mappings`.
5. For account-id triggers without a mapping, each registered token is checked against Cloudflare's accounts API.

## Env Vars

`api_token` may be stored as plain text or as a strict whole-value environment reference such as `${WRANGLERX_TOKEN_PERSONAL}`. Missing environment variables are reported when the selected account is used.

To disable update checks in the version banner:

```bash
export WRANGLERX_NO_UPDATE_CHECK=1
```

## Requirements

- [`wrangler`](https://developers.cloudflare.com/workers/wrangler/) installed and available on `PATH`
- At least one registered Cloudflare API token in `~/.config/wranglerx/accounts.yml`

## Build

```bash
cargo build --release
```

Or with the included task:

```bash
task build
```

## Release

GitHub Release is triggered by pushing a `wranglerx-v<version>` tag. `task release:wranglerx` fetches `origin/main`, reads the version from `crates/wranglerx/Cargo.toml` on that branch, creates a temporary worktree at `origin/main`, runs `cargo test -p wranglerx` there, then creates and pushes the tag.

```bash
task release:wranglerx
```

## Development

Run tests:

```bash
task test
```

Run in development mode:

```bash
task dev:wranglerx -- --dry-run deploy
```
