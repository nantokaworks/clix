# flyx

[![CI](https://github.com/nantokaworks/clix/actions/workflows/ci.yml/badge.svg)](https://github.com/nantokaworks/clix/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**Automatically switch Fly.io accounts based on the directory you're in.**

> **Prerequisite:** `flyx` is a wrapper around [`fly`](https://fly.io/docs/flyctl/). Install flyctl first; `flyx` shells out to it for every operation that talks to Fly.io.

If you work across personal, work, or client Fly.io organizations, `flyx` lets each project carry the account context. It snapshots your Fly tokens, reads the `app` from `fly.toml` (or the `-a` / `-o` flag), figures out which account owns it, and runs `fly` with the matching `FLY_API_TOKEN`.

## Quick start

```bash
flyx auth login                # OAuth in browser; flyx auto-snapshots the result
flyx auth login work           # same, saved as profile "work"
flyx deploy                    # picks the right token based on cwd's fly.toml
flyx --profile work logs       # one-off override
```

## Routing

`flyx` resolves which token to use in this order:

1. `FLY_API_TOKEN` / `FLY_ACCESS_TOKEN` already set in env → pass through unchanged (CI workflows).
2. `flyx --profile <name> <cmd>` → use that profile's token explicitly.
3. `-a <app>` / `--app <app>` → mappings cache → profile owning the app.
4. fly.toml `app` (walked up from cwd) → mappings cache → profile owning the app.
5. `-o <slug>` / `--org <slug>` → match against profile `org_slugs`.
6. git remote owner (from `git remote get-url origin`) → match against profile `org_slugs`.
7. default profile (set with `flyx x use <profile>`).

The `mappings` cache is populated automatically when you run `flyx auth login`, `flyx x refresh`, or `flyx x save-token` — `fly apps list --json` returns every app the token can see, and the result is cached for offline routing.

## Commands

```bash
# Login / signup — these REPLACE manual snapshot steps.
flyx auth login [<name>]              # OAuth + auto-snapshot
flyx auth signup [<name>]             # signup + auto-snapshot
flyx auth logout                      # passthrough; profile store untouched

# Profile management
flyx x list                           # list profiles + cached mappings (auto-syncs from ~/.fly/)
flyx x use <profile>                  # change the default profile
flyx x remove <profile>               # delete a profile
flyx x refresh [<profile>]            # re-probe orgs/apps via flyctl
flyx x save-token <name> <token>      # register a paste-in token (e.g. `fly tokens create org`)
flyx x import                         # explicit scan of ~/.fly/config*.yml
flyx x whoami [<profile>]             # show profile details
```

`flyx x list` and `flyx x whoami` automatically sync from `~/.fly/config*.yml` — if you ran `fly auth login` outside of flyx, the new token is detected and snapshotted on the next list/whoami invocation (matched by root macaroon, so rotated discharges don't double-import).

## Profile YAML

Snapshots live in `~/.config/flyx/profiles.yml`:

```yaml
default: ichi
profiles:
  ichi:
    access_token: fm2_lJPECAA...
    email: you@example.com
    org_slug: personal
    org_slugs: [personal, nantokaworks]
  work:
    access_token: fo1_xxx...
    email: you@work.example.com
    org_slug: acme
    org_slugs: [acme]
mappings:                       # auto-populated cache; do not hand-edit
  some-app: ichi
  acme: work
```

Hand-editing `mappings` is unnecessary — `refresh` rebuilds it. If you want to override routing for a single invocation, use `flyx --profile <name>`.

## Env vars

```bash
export FLYX_NO_UPDATE_CHECK=1   # silence the version banner update check
```

In CI, set `FLY_API_TOKEN` directly — `flyx` passes it through to `fly` without touching the profile store.

## Requirements

- [`fly`](https://fly.io/docs/flyctl/install/) installed and available on `PATH`. flyx delegates every Fly.io interaction to flyctl, so anything flyctl can do for a token, flyx can route correctly.
- At least one profile registered with `flyx auth login` (or `flyx x save-token`).

## Build

```bash
cargo build --release -p flyx
```

Or with the included task:

```bash
task dev:flyx -- --dry-run deploy
```
