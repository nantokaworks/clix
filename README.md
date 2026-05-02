# clix

[![CI](https://github.com/ichi0g0y/clix/actions/workflows/ci.yml/badge.svg)](https://github.com/ichi0g0y/clix/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**The clix toolkit — directory-aware account switchers for popular dev CLIs.**

`clix` is a Cargo workspace that hosts a small family of CLI wrappers. Each member detects which account to use from your current directory (git remote, project config) and forwards the command to the upstream CLI with the right credentials injected. No more manual `auth switch` between personal and work accounts.

## Member tools

| Tool | Wraps | Switches by | Status |
|---|---|---|---|
| [ghx](crates/ghx/) | [`gh`](https://cli.github.com/) | git remote owner | shipped |
| wranglerx | [`wrangler`](https://developers.cloudflare.com/workers/wrangler/) | `wrangler.toml` `account_id` | planned |
| flyx | [`fly`](https://fly.io/docs/flyctl/) | `fly.toml` `app` | planned |

Each tool is released independently. Tags follow the `<tool>-v<semver>` convention (e.g. `ghx-v0.4.0`).

## Workspace layout

```
crates/
├── core/        # clix-core: shared git remote parser, exec, banner, update check
├── ghx/         # gh wrapper
├── wranglerx/   # wrangler wrapper (planned)
└── flyx/        # fly wrapper (planned)
```

## Install

Each member tool has its own install instructions in its directory:

- [`crates/ghx/README.md`](crates/ghx/README.md)

The `brew install ichi0g0y/tap/<tool>` formulae and the `cargo install --git` paths are tool-specific.

## Build

```bash
cargo build --release --workspace
```

## License

MIT
