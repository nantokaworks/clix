mod cloudflare_api;
mod config;
mod error;
mod oauth;
mod x_cmd;

use std::env;
use std::process::{self, Command};

use clix_core::banner;
use clix_core::exec::{ExecError, exec_replace};
use clix_core::git;
use clix_core::update;
use colored::Colorize;

use config::wrangler_toml::{ProjectConfigKind, find_project_account_id};
use config::{TriggerSource, trigger_source_label};

fn run() -> Result<(), error::Error> {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut cmd = Command::new("wrangler");
    cmd.args(&args);

    if is_version_command(&args) {
        return print_wranglerx_banner();
    }

    if args.is_empty() {
        print_wranglerx_banner()?;
        x_cmd::print_bare_hint();
        return run_wrangler(cmd);
    }

    if is_top_level_help(&args) {
        return run_wrangler_then(cmd, x_cmd::print_extras_section);
    }

    if should_passthrough(&args) {
        return run_wrangler(cmd);
    }

    if let [first, rest @ ..] = args.as_slice() {
        if first == "x" {
            return x_cmd::run(rest);
        }
    }

    if is_dry_run(&args) {
        return print_dry_run();
    }

    if env::var_os("CLOUDFLARE_ACCOUNT_ID").is_some() {
        return run_wrangler(cmd);
    }

    let (trigger, source) = resolve_trigger()?;
    let profile = config::resolve_profile(&trigger, &source)?;
    cmd.env("CLOUDFLARE_API_TOKEN", &profile.access_token);
    cmd.env("CLOUDFLARE_ACCOUNT_ID", &profile.account_id);

    run_wrangler(cmd)
}

fn run_wrangler(cmd: Command) -> Result<(), error::Error> {
    exec_replace(cmd).map_err(|e| match e {
        ExecError::NotFound => error::Error::WranglerNotFound,
        ExecError::Failed(msg) => error::Error::ExecFailed(msg),
    })
}

fn run_wrangler_then(mut cmd: Command, trailer: fn()) -> Result<(), error::Error> {
    let status = cmd.status().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => error::Error::WranglerNotFound,
        _ => error::Error::ExecFailed(e.to_string()),
    })?;
    trailer();
    process::exit(status.code().unwrap_or(1));
}

fn is_top_level_help(args: &[String]) -> bool {
    matches!(args, [first] if matches!(first.as_str(), "--help" | "-h" | "help"))
}

fn resolve_trigger() -> Result<(String, TriggerSource), error::Error> {
    if let Some(project) = find_project_account_id()? {
        let source = match project.kind {
            ProjectConfigKind::Toml => TriggerSource::WranglerToml(project.path),
            ProjectConfigKind::Jsonc => TriggerSource::WranglerJsonc(project.path),
        };
        return Ok((project.account_id, source));
    }

    match git::get_remote_owner() {
        Ok(owner) => Ok((owner, TriggerSource::GitRemote)),
        Err(_) => Ok((String::new(), TriggerSource::Default)),
    }
}

fn print_dry_run() -> Result<(), error::Error> {
    if let Ok(account_id) = env::var("CLOUDFLARE_ACCOUNT_ID") {
        eprintln!("wranglerx dry-run:");
        eprintln!("  mode: pass-through");
        eprintln!("  trigger source: env:CLOUDFLARE_ACCOUNT_ID");
        eprintln!("  account_id: {account_id}");
        return Ok(());
    }

    let (trigger, source) = resolve_trigger()?;
    let profile = config::resolve_profile(&trigger, &source)?;
    eprintln!("wranglerx dry-run:");
    eprintln!("  profile: {}", profile.name);
    eprintln!("  account_id: {}", profile.account_id);
    eprintln!("  trigger source: {}", trigger_source_label(&source));
    if profile.refreshed {
        eprintln!("  oauth: refreshed");
    }
    Ok(())
}

fn is_version_command(args: &[String]) -> bool {
    matches!(args, [first, ..] if matches!(first.as_str(), "--version" | "version"))
}

fn is_dry_run(args: &[String]) -> bool {
    matches!(args, [first, ..] if first == "--dry-run")
}

fn should_passthrough(args: &[String]) -> bool {
    match args {
        [first, ..] if matches!(first.as_str(), "--help" | "-h" | "help") => true,
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
    if let Ok(account_id) = env::var("CLOUDFLARE_ACCOUNT_ID") {
        context_lines.push(format!(
            "{} {}",
            "account_id:".dimmed(),
            account_id.yellow()
        ));
    } else if let Ok((trigger, source)) = resolve_trigger() {
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
    use super::{is_dry_run, is_top_level_help, is_version_command, should_passthrough};

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

    #[test]
    fn detects_top_level_help_only() {
        for args in [
            vec!["--help".to_string()],
            vec!["-h".to_string()],
            vec!["help".to_string()],
        ] {
            assert!(is_top_level_help(&args), "{args:?}");
        }
        for args in [
            vec![],
            vec!["help".to_string(), "deploy".to_string()],
            vec!["deploy".to_string(), "--help".to_string()],
        ] {
            assert!(!is_top_level_help(&args), "{args:?}");
        }
    }
}
