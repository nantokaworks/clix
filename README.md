# ghx

`ghx` is a thin wrapper around [`gh`](https://cli.github.com/) for people who use multiple GitHub accounts locally.

It looks at the current repository's `origin` remote, infers the GitHub owner, resolves the matching account from `gh`'s `hosts.yml`, fetches a token for that account via `gh auth token -u <user>`, and then runs `gh` with `GH_TOKEN` set for the selected user.

The goal is simple: keep normal `gh` behavior, but automatically use the right account for the current repository whenever possible.

## Why

If you work across personal and organizational repositories, `gh` account switching can be tedious. `ghx` lets you stay on one shell command while selecting the account from repository context.

## インストール

### Homebrew (macOS / Linux)

```bash
brew install ichi0g0y/tap/ghx
```

### シェルスクリプト (macOS / Linux)

```bash
curl -fsSL https://raw.githubusercontent.com/ichi0g0y/ghx/main/install.sh | sh
```

### Cargo (全プラットフォーム)

```bash
cargo install --git https://github.com/ichi0g0y/ghx
```

### バイナリダウンロード

macOS / Linux / Windows 向けのビルド済みバイナリを [Releases](https://github.com/ichi0g0y/ghx/releases) ページからダウンロードできます。

## How It Works

For repository-aware commands, `ghx` does the following:

1. Runs `git remote get-url origin`
2. Extracts the GitHub owner from the remote URL
3. Reads `gh` config from:
   - `$GH_CONFIG_DIR`
   - `$XDG_CONFIG_HOME/gh`
   - `%APPDATA%/GitHub CLI` on Windows
   - `~/.config/gh`
4. Loads `hosts.yml`
5. Uses the owner if it matches a configured user under `github.com.users`
6. Falls back to the active `github.com.user` if there is no direct match
7. Runs `gh auth token -u <resolved-user>`
8. Executes `gh` with `GH_TOKEN` set to that token

For bootstrap commands such as `help` and `auth ...`, `ghx` passes through directly so it does not block basic `gh` usage when a repository or config is unavailable.

For `ghx version` and `ghx --version`, `ghx` prints its own version first and then forwards to `gh` so you can see both versions in one place.

## Requirements

- [`gh`](https://cli.github.com/) installed and available on `PATH`
- At least one authenticated GitHub account in `gh auth login`

## Build

```bash
cargo build --release
```

Or with the included task:

```bash
task build
```

## Usage

Run `ghx` the same way you would run `gh`:

```bash
ghx pr status
ghx issue list
ghx repo view
```

Bootstrap commands are forwarded without repository-based account resolution:

```bash
ghx help
ghx auth status
ghx auth login
```

Version commands show both `ghx` and `gh`:

```bash
ghx version
ghx --version
```

## Configuration Expectations

`ghx` currently assumes:

- The repository remote is GitHub
- The remote is configured as `origin`
- The owner can be parsed from either:
  - `git@github.com:owner/repo.git`
  - `https://github.com/owner/repo.git`
  - `https://github.com/owner/repo`

If the owner matches one of the entries under `github.com.users` in `hosts.yml`, that user is selected. Otherwise, the active `github.com.user` is used.

## Example

If your local `gh` config contains accounts for `alice` and `acme-inc`, then:

- In a repo with `origin = git@github.com:alice/tooling.git`, `ghx` uses `alice`
- In a repo with `origin = git@github.com:acme-inc/backend.git`, `ghx` uses `acme-inc`

## Limitations

- Only `origin` is inspected
- Only GitHub remotes are supported
- Owner resolution is based on the remote URL, not on deeper repository metadata
- Account resolution is intentionally simple: exact user match first, then active user fallback

## Troubleshooting

If `gh` is not installed or not available on `PATH`, `ghx` prints a direct message with the next commands to run:

```bash
$ ghx pr status
ghx: gh が見つかりません
  確認: gh --version
  インストール後に次を実行してください: gh auth login
  https://cli.github.com/
```

When `gh` is installed, version output includes both tools:

```bash
$ ghx version
ghx 0.1.0
gh version 2.x.y
...
```

If `gh` is installed but `hosts.yml` does not exist yet, `ghx` asks you to log in with `gh` first:

```bash
$ ghx pr status
ghx: gh の設定が見つかりません: ~/.config/gh/hosts.yml
  次を実行してください: gh auth login
```

## Development

Run tests:

```bash
cargo test
```

Run in development mode:

```bash
task dev -- pr status
```
