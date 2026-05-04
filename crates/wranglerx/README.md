# wranglerx

[![CI](https://github.com/nantokaworks/clix/actions/workflows/ci.yml/badge.svg)](https://github.com/nantokaworks/clix/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/nantokaworks/clix?filter=wranglerx-*&label=wranglerx)](https://github.com/nantokaworks/clix/releases?q=wranglerx-)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**Automatically switch Cloudflare Wrangler accounts based on the directory you're in.**

> **Prerequisite:** `wranglerx` is a wrapper around [`wrangler`](https://developers.cloudflare.com/workers/wrangler/). Install Wrangler first, sign in with `wrangler login` for each Cloudflare account, and snapshot each session into a wranglerx profile with `wranglerx x save <profile>`.

If you work across personal, work, or client Cloudflare accounts, `wranglerx` lets each project carry the account context. It snapshots the OAuth tokens from `wrangler login`, detects `account_id` from `wrangler.toml` / `wrangler.jsonc` (or falls back to the GitHub remote owner / a configured default profile), refreshes expired tokens automatically, and runs `wrangler` with the matching `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID`.

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

## Profile Management

Sign in with each Cloudflare account using vanilla `wrangler login`, then snapshot the resulting OAuth credentials into a named wranglerx profile:

```bash
wrangler login                              # browser-based OAuth flow for account A
wranglerx x save personal                # snapshot to "personal" profile

wrangler logout
wrangler login                              # browser-based OAuth flow for account B
wranglerx x save work                    # snapshot to "work" profile
```

Subcommands:

```bash
wranglerx x list                         # show profiles, account_ids, expirations
wranglerx x use <profile>                # set the default fallback profile
wranglerx x bind <profile> --account-id <id>   # manual mapping (multi-account tokens)
wranglerx x remove <profile>             # delete a profile
wranglerx x refresh <profile>            # force OAuth refresh
wranglerx x whoami [<profile>]           # show profile details
```

Snapshots live in `~/.config/wranglerx/profiles.yml`:

```yaml
default: personal
profiles:
  personal:
    access_token: <oauth-access-token>
    refresh_token: <oauth-refresh-token>
    expiration_time: 2026-05-03T13:34:56Z
    account_id: 1234abcd
    account_ids: [1234abcd]
    scopes: [account:read, workers:write, ...]
  work:
    access_token: ...
    ...
mappings:
  1234abcd: personal
  myorg: work
```

`wranglerx login` and `wranglerx logout` continue to pass through to vanilla `wrangler` without touching the profile store, so the OAuth flow remains untouched.

## Resolution Order

1. If `CLOUDFLARE_ACCOUNT_ID` is already set in the environment, `wranglerx` passes through to `wrangler` unchanged (CI workflows).
2. Otherwise, `wranglerx` walks up from the current directory and reads top-level `account_id` from the nearest `wrangler.toml` or `wrangler.jsonc`.
3. If no project account id is found, `wranglerx` tries the GitHub owner from `git remote get-url origin`.
4. If neither source yields a hit, `wranglerx` falls back to the `default` profile (set via `wranglerx x use <profile>`).
5. The trigger key is matched against `mappings`, then against each profile's `account_id` / `account_ids` for direct account-id triggers.

If the resolved profile's `expiration_time` is past (or within 60 seconds), `wranglerx` automatically refreshes the OAuth token using `refresh_token` and rewrites `profiles.yml` before invoking `wrangler`.

## Env Vars

To disable update checks in the version banner:

```bash
export WRANGLERX_NO_UPDATE_CHECK=1
```

In CI, set `CLOUDFLARE_ACCOUNT_ID` and `CLOUDFLARE_API_TOKEN` directly — `wranglerx` will pass them through to `wrangler` without touching the profile store.

## Requirements

- [`wrangler`](https://developers.cloudflare.com/workers/wrangler/) installed and available on `PATH`
- At least one profile saved with `wrangler login` + `wranglerx x save <profile>`

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
