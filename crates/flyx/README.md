# flyx

[![CI](https://github.com/nantokaworks/clix/actions/workflows/ci.yml/badge.svg)](https://github.com/nantokaworks/clix/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**Automatically switch Fly.io accounts based on the directory you're in.**

> **Prerequisite:** `flyx` is a wrapper around [`fly`](https://fly.io/docs/flyctl/). Install flyctl first, sign in with `fly auth login` for each Fly.io account, and snapshot each session into a flyx profile with `flyx x save <profile>` (or just let auto-import pick up your existing `~/.fly/config*.yml` files on first run).

If you work across personal, work, or client Fly.io organizations, `flyx` lets each project carry the account context. It snapshots the macaroon access token from `~/.fly/config.yml`, detects the `app` from `fly.toml`, looks up its owning organization through Fly's GraphQL API on first use (caching the result), and runs `fly` with the matching `FLY_API_TOKEN`.

Unlike Cloudflare Wrangler, Fly.io does not use OAuth refresh tokens — its tokens are long-lived macaroons. When a token expires, run `fly auth login` again and re-snapshot with `flyx x save <profile>`.

## Usage

Run `flyx` the same way you would run `fly`:

```bash
flyx deploy
flyx status
flyx logs
```

Bootstrap commands are forwarded without account resolution:

```bash
flyx help
flyx auth login
flyx auth logout
flyx auth signup
```

Other `fly auth ...` subcommands (`docker`, `whoami`) flow through normal token injection so they operate against the resolved profile.

Version commands print the `flyx` banner:

```bash
flyx version
flyx --version
```

Use `--dry-run` to inspect the selected profile without running `fly`:

```bash
flyx --dry-run deploy
```

## Profile Management

### Auto-import from existing fly configs (zero-config path)

If you already swap `~/.fly/config.yml` manually (or keep nicknamed siblings like `~/.fly/config.work.yml`), `flyx` discovers and imports them on first invocation:

- `~/.fly/config.yml` → profile **default**
- `~/.fly/config.<nickname>.yml` → profile **<nickname>**

The import runs lazily on the first `flyx <command>` when `~/.config/flyx/profiles.yml` is empty. Each token is hit against Fly's GraphQL API to fetch the email and accessible orgs.

You can also trigger a re-import explicitly (it skips profiles that already exist):

```bash
flyx x import
```

### Manual snapshots

Sign in with each Fly.io account using vanilla `fly auth login`, then snapshot the resulting access token into a named flyx profile:

```bash
fly auth login                              # browser-based flow for account A
flyx x save personal                     # snapshot to "personal" profile

fly auth logout
fly auth login                              # browser-based flow for account B
flyx x save work                         # snapshot to "work" profile
```

Subcommands:

```bash
flyx x list                              # show profiles, orgs, default
flyx x use <profile>                     # set the default fallback profile
flyx x bind <profile> --app <app>        # manual app->profile mapping
flyx x bind <profile> --org <slug>       # manual org->profile mapping
flyx x remove <profile>                  # delete a profile
flyx x whoami [<profile>]                # show profile details
flyx x import                            # re-scan ~/.fly/config*.yml
```

Snapshots live in `~/.config/flyx/profiles.yml`:

```yaml
default: personal
profiles:
  personal:
    access_token: fm2_lJPECAA...,fm2_lJPECAA...
    email: you@example.com
    org_slug: personal
    org_slugs: [personal, nantokaworks]
  work:
    access_token: fm2_lJPECAA...
    email: you@work.example.com
    org_slug: acme
    org_slugs: [acme]
mappings:
  my-app: personal      # fly.toml app name (cached on first lookup)
  acme: work            # org slug or git remote owner
```

`flyx auth login` / `flyx auth logout` / `flyx auth signup` continue to pass through to vanilla `fly auth ...` without touching the profile store, so the login flow remains untouched. Other `fly auth ...` subcommands (`docker`, `whoami`) flow through normal token injection so they operate against the resolved profile.

## Resolution Order

1. If `FLY_API_TOKEN` or `FLY_ACCESS_TOKEN` is already set in the environment, `flyx` passes through to `fly` unchanged (CI workflows).
2. Otherwise, `flyx` walks up from the current directory and reads top-level `app` from the nearest `fly.toml`.
3. If no project app is found, `flyx` tries the GitHub owner from `git remote get-url origin`.
4. If neither source yields a hit, `flyx` falls back to the `default` profile (set via `flyx x use <profile>`).
5. The trigger key is matched against `mappings` first. On miss for a `fly.toml` app, `flyx` queries Fly's GraphQL API via each saved profile to find the owning org and caches the result back into `mappings`.

## Env Vars

To disable update checks in the version banner:

```bash
export FLYX_NO_UPDATE_CHECK=1
```

In CI, set `FLY_API_TOKEN` directly — `flyx` will pass it through to `fly` without touching the profile store.

## Requirements

- [`fly`](https://fly.io/docs/flyctl/install/) installed and available on `PATH`
- At least one profile saved with `fly auth login` + `flyx x save <profile>`

## Build

```bash
cargo build --release -p flyx
```

Or with the included task:

```bash
task dev:flyx -- --dry-run deploy
```
