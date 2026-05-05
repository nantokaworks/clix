mod args;
mod cloudflare_api;
mod config;
mod error;
mod help;
mod oauth;
mod resolve;
#[cfg(test)]
mod test_support;
mod wrangler_cli;
mod x_cmd;

use std::env;
use std::process::{self, Command};

use clix_core::banner;
use clix_core::exec::{self, ExecError, exec_replace};
use clix_core::update;
use colored::Colorize;

use config::trigger_source_label;

fn run() -> Result<(), error::Error> {
    let raw: Vec<String> = env::args().skip(1).collect();

    if is_version_command(&raw) {
        return print_wranglerx_banner();
    }

    let parsed = args::parse(&raw);
    let mut cmd = Command::new("wrangler");
    cmd.args(&parsed.raw);

    if parsed.raw.is_empty() {
        print_wranglerx_banner()?;
        exec::write_or_exit_on_pipe_close(help::BARE_HINT);
        return run_wrangler(cmd);
    }

    if help::is_top_level_help(&parsed.raw) {
        return run_wrangler_with_extras(cmd);
    }

    if should_passthrough(&parsed.raw) {
        return run_wrangler(cmd);
    }

    if let [first, rest @ ..] = parsed.raw.as_slice() {
        if first == "x" {
            return x_cmd::run(rest);
        }
    }

    if is_whoami(&parsed.raw) {
        return run_whoami(&parsed);
    }

    if is_dry_run(&parsed.raw) {
        return resolve::print_dry_run(&parsed);
    }

    // Layer 7: explicit token in env — user signaled "don't manage this for me".
    // `--account-id` flag is still honored: replay it as CLOUDFLARE_ACCOUNT_ID
    // so the explicit account routes through wrangler.
    if resolve::has_cloudflare_env_token() {
        if let Some(id) = parsed.account_id_override.as_deref() {
            cmd.env("CLOUDFLARE_ACCOUNT_ID", id);
        }
        return run_wrangler(cmd);
    }

    // Layer 8: --profile <name> — inject the (refresh-aware) access_token.
    // Account_id precedence: explicit `--account-id` flag > the profile's
    // primary account_id (when unambiguous) > whatever wrangler resolves on
    // its own (wrangler.toml / outer-shell CLOUDFLARE_ACCOUNT_ID env / etc).
    if let Some(profile_name) = parsed.profile_override.as_deref() {
        let lookup = resolve::lookup_profile(profile_name)?;
        cmd.env("CLOUDFLARE_API_TOKEN", &lookup.access_token);
        let account_id = parsed
            .account_id_override
            .clone()
            .or(lookup.primary_account_id);
        if let Some(id) = account_id {
            cmd.env("CLOUDFLARE_ACCOUNT_ID", id);
        }
        return run_wrangler(cmd);
    }

    // Backward-compat: bare CLOUDFLARE_ACCOUNT_ID env (without --account-id flag
    // or --profile) means the user is managing creds themselves outside
    // wranglerx. Pass through verbatim.
    if env::var_os("CLOUDFLARE_ACCOUNT_ID").is_some() && parsed.account_id_override.is_none() {
        return run_wrangler(cmd);
    }

    // Layer 9 (default): 5-layer routing inside resolve_trigger.
    let (trigger, source) = resolve::resolve_trigger(&parsed)?;
    let resolved = resolve::resolve_profile(&trigger, &source)?;
    cmd.env("CLOUDFLARE_API_TOKEN", &resolved.access_token);
    cmd.env("CLOUDFLARE_ACCOUNT_ID", &resolved.account_id);

    run_wrangler(cmd)
}

fn run_wrangler(cmd: Command) -> Result<(), error::Error> {
    exec_replace(cmd).map_err(map_exec_err)
}

fn run_wrangler_with_extras(cmd: Command) -> Result<(), error::Error> {
    exec::run_with_trailer(cmd, help::EXTRAS_SECTION).map_err(map_exec_err)
}

fn map_exec_err(e: ExecError) -> error::Error {
    match e {
        ExecError::NotFound => error::Error::WranglerNotFound,
        ExecError::Failed(msg) => error::Error::ExecFailed(msg),
    }
}

fn run_whoami(parsed: &args::ParsedArgs) -> Result<(), error::Error> {
    // Resolve the token the same way an ordinary command would, so `wranglerx
    // whoami` answers "who am I according to *this* routing context?".
    let token = if resolve::has_cloudflare_env_token() {
        None
    } else if let Some(name) = parsed.profile_override.as_deref() {
        Some(resolve::lookup_profile(name)?.access_token)
    } else if env::var_os("CLOUDFLARE_ACCOUNT_ID").is_some()
        && parsed.account_id_override.is_none()
    {
        None
    } else {
        let (trigger, source) = resolve::resolve_trigger(parsed)?;
        let resolved = resolve::resolve_profile(&trigger, &source)?;
        Some(resolved.access_token)
    };

    let out = wrangler_cli::whoami(token.as_deref())?;
    wrangler_cli::print_whoami(&out);
    Ok(())
}

fn is_version_command(args: &[String]) -> bool {
    matches!(args, [first, ..] if matches!(first.as_str(), "--version" | "version"))
}

fn is_dry_run(args: &[String]) -> bool {
    matches!(args, [first, ..] if first == "--dry-run")
}

fn is_whoami(args: &[String]) -> bool {
    matches!(args, [first] if first == "whoami")
}

fn should_passthrough(args: &[String]) -> bool {
    match args {
        [first, ..] if matches!(first.as_str(), "--help" | "-h" | "help") => true,
        // `login` is passthrough until Phase 2 wires `auth::login`.
        [first, ..] if matches!(first.as_str(), "login" | "logout") => true,
        _ => false,
    }
}

fn print_wranglerx_banner() -> Result<(), error::Error> {
    let ascii_art = [
        "██╗    ██╗██████╗ ██╗  ██╗",
        "██║    ██║██╔══██╗╚██╗██╔╝",
        "██║ █╗ ██║██████╔╝ ╚███╔╝",
        "██║███╗██║██╔══██╗ ██╔██╗",
        "╚███╔███╔╝██║  ██║██╔╝ ██╗",
        " ╚══╝╚══╝ ╚═╝  ╚═╝╚═╝  ╚═╝",
    ];

    let mut context_lines: Vec<String> = Vec::new();
    if let Some((env_name, _)) = resolve::cloudflare_env_token() {
        context_lines.push(format!(
            "{} {}",
            format!("{env_name}:").dimmed(),
            "(env override)".yellow()
        ));
    } else if let Ok(account_id) = env::var("CLOUDFLARE_ACCOUNT_ID") {
        context_lines.push(format!(
            "{} {}",
            "account_id:".dimmed(),
            account_id.yellow()
        ));
    } else if let Ok((trigger, source)) = resolve::resolve_trigger(&args::ParsedArgs::default()) {
        let trigger_label = if trigger.is_empty() {
            "(default)".to_string()
        } else {
            trigger
        };
        context_lines.push(format!(
            "{} {}",
            "trigger:".dimmed(),
            trigger_label.yellow()
        ));
        context_lines.push(format!(
            "{} {}",
            "source:".dimmed(),
            trigger_source_label(&source).dimmed()
        ));
    }

    let update_request = update::CheckRequest {
        repo_slug: "nantokaworks/clix",
        tool_name: "wranglerx",
        current_version: env!("CARGO_PKG_VERSION"),
        disable_env_var: "WRANGLERX_NO_UPDATE_CHECK",
    };
    let update = update::check_for_update(&update_request).map(|info| banner::UpdateNotice {
        current: env!("CARGO_PKG_VERSION").to_string(),
        latest: info.latest,
        command: info.upgrade_cmd,
    });

    let banner = banner::Banner {
        ascii_art: &ascii_art,
        description: env!("CARGO_PKG_DESCRIPTION"),
        version: env!("CARGO_PKG_VERSION"),
        build_date: env!("WRANGLERX_BUILD_DATE"),
        repository: env!("CARGO_PKG_REPOSITORY"),
        context_lines,
        update,
    };

    banner::print(&banner).map_err(|e| error::Error::ExecFailed(e.to_string()))
}

fn main() {
    if let Err(e) = run() {
        eprintln!("wranglerx: {e}");
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::{is_dry_run, is_version_command, is_whoami, should_passthrough};

    #[test]
    fn detects_version_paths() {
        for args in [
            vec!["--version".to_string()],
            vec!["version".to_string()],
            vec!["version".to_string(), "--help".to_string()],
        ] {
            assert!(is_version_command(&args));
        }
    }

    #[test]
    fn detects_dry_run() {
        assert!(is_dry_run(&["--dry-run".to_string()]));
        assert!(!is_dry_run(&["deploy".to_string()]));
    }

    #[test]
    fn detects_whoami() {
        assert!(is_whoami(&["whoami".to_string()]));
        assert!(!is_whoami(&[
            "whoami".to_string(),
            "--help".to_string()
        ]));
        assert!(!is_whoami(&["deploy".to_string()]));
    }

    #[test]
    fn passthrough_for_bootstrap_paths() {
        for args in [
            vec!["--help".to_string()],
            vec!["-h".to_string()],
            vec!["help".to_string()],
            vec!["login".to_string()],
            vec!["logout".to_string()],
        ] {
            assert!(should_passthrough(&args));
        }
    }

    #[test]
    fn intercepts_whoami_and_x() {
        for args in [vec!["whoami".to_string()], vec!["x".to_string()]] {
            assert!(!should_passthrough(&args));
        }
    }
}
