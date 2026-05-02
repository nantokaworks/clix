# clix

[![CI](https://github.com/ichi0g0y/clix/actions/workflows/ci.yml/badge.svg)](https://github.com/ichi0g0y/clix/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**The clix toolkit — directory-aware account switchers for popular dev CLIs.**

`clix` is a Cargo workspace that hosts a small family of CLI wrappers. Each member detects which account to use from your current directory (git remote, project config) and forwards the command to the upstream CLI with the right credentials injected. No more manual `auth switch` between personal and work accounts.

![ghx demo](.github/demo.gif)

## What It Solves

Many developer CLIs keep one active account at a time. That works until you jump between personal, work, and client projects in different terminals or automation sessions. `clix` tools avoid global account switching: they inspect the current directory, pick the matching account, inject credentials into only the child process they are about to run, and then get out of the way.

Use the member tool that matches the upstream CLI:

- Use `ghx` when you want `gh` to select the right GitHub account from the repository owner.
- Use `wranglerx` when you want `wrangler` to select the right Cloudflare account from `wrangler.toml` or `wrangler.jsonc`.

## Member tools

| Tool | Wraps | Switches by | Status |
|---|---|---|---|
| [ghx](crates/ghx/) | [`gh`](https://cli.github.com/) | git remote owner | shipped |
| [wranglerx](crates/wranglerx/) | [`wrangler`](https://developers.cloudflare.com/workers/wrangler/) | `wrangler.toml` `account_id` | shipped |
| flyx | [`fly`](https://fly.io/docs/flyctl/) | `fly.toml` `app` | planned |

Each tool is released independently. Tags follow the `<tool>-v<semver>` convention (e.g. `ghx-v0.4.0`).

## Quick Examples

`ghx` reads the current repository's GitHub owner and runs `gh` with the matching token:

```bash
cd ~/src/work-api
ghx pr status
```

`wranglerx` reads the current project's Cloudflare `account_id` and runs `wrangler` with the matching API token:

```bash
cd ~/src/worker
wranglerx --dry-run deploy
wranglerx deploy
```

Each tool has its own detailed README with setup, config, and troubleshooting notes.

## Workspace layout

```
crates/
├── core/        # clix-core: shared git remote parser, exec, banner, update check
├── ghx/         # gh wrapper
├── wranglerx/   # wrangler wrapper
└── flyx/        # fly wrapper (planned)
```

## Install

Each member tool has its own install instructions in its directory:

- [`crates/ghx/README.md`](crates/ghx/README.md)
- [`crates/wranglerx/README.md`](crates/wranglerx/README.md)

The `brew install ichi0g0y/tap/<tool>` formulae and the `cargo install --git` paths are tool-specific.

## Build

```bash
cargo build --release --workspace
```

## License

MIT
