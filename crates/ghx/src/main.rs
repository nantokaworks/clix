mod config;
mod error;
mod x_cmd;

use std::env;
use std::process::{self, Command};

use clix_core::banner;
use clix_core::exec::{ExecError, exec_replace};
use clix_core::git;
use clix_core::update;
use colored::Colorize;

fn run() -> Result<(), error::Error> {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut cmd = Command::new("gh");
    cmd.args(&args);

    if is_version_command(&args) {
        return print_ghx_banner();
    }

    if args.is_empty() {
        print_ghx_banner()?;
        x_cmd::print_bare_hint();
        return run_gh(cmd);
    }

    if is_top_level_help(&args) {
        return run_gh_then(cmd, x_cmd::print_extras_section);
    }

    if should_passthrough(&args) {
        return run_gh(cmd);
    }

    if let [first, rest @ ..] = args.as_slice() {
        if first == "x" {
            return x_cmd::run(rest);
        }
    }

    let owner = git::get_remote_owner()?;
    let gh_user = config::resolve_gh_user(&owner)?;
    let token = config::get_token(&gh_user)?;

    cmd.env("GH_TOKEN", &token);

    run_gh(cmd)
}

fn run_gh(cmd: Command) -> Result<(), error::Error> {
    exec_replace(cmd).map_err(|e| match e {
        ExecError::NotFound => error::Error::GhNotFound,
        ExecError::Failed(msg) => error::Error::ExecFailed(msg),
    })
}

fn run_gh_then(mut cmd: Command, trailer: fn()) -> Result<(), error::Error> {
    let status = cmd.status().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => error::Error::GhNotFound,
        _ => error::Error::ExecFailed(e.to_string()),
    })?;
    trailer();
    process::exit(status.code().unwrap_or(1));
}

fn is_version_command(args: &[String]) -> bool {
    matches!(args, [first, ..] if matches!(first.as_str(), "--version" | "version"))
}

fn is_top_level_help(args: &[String]) -> bool {
    matches!(args, [first] if matches!(first.as_str(), "--help" | "-h" | "help"))
}

fn should_passthrough(args: &[String]) -> bool {
    match args {
        [first, ..] if matches!(first.as_str(), "--help" | "-h" | "help") => true,
        [first, ..] if first == "auth" => true,
        _ => false,
    }
}

fn print_ghx_banner() -> Result<(), error::Error> {
    let ascii_art = [
        " ██████╗ ██╗  ██╗██╗  ██╗",
        "██╔════╝ ██║  ██║╚██╗██╔╝",
        "██║  ███╗███████║ ╚███╔╝",
        "██║   ██║██╔══██║ ██╔██╗",
        "╚██████╔╝██║  ██║██╔╝ ██╗",
        " ╚═════╝ ╚═╝  ╚═╝╚═╝  ╚═╝",
    ];

    let mut context_lines: Vec<String> = Vec::new();
    if let Some(info) = config::get_account_info() {
        if let Ok(owner) = git::get_remote_owner() {
            if let Some(resolved) = config::resolve_gh_user_for_display(&owner) {
                if resolved == owner {
                    context_lines
                        .push(format!("{} {}", "using:".dimmed(), resolved.green().bold()));
                } else {
                    context_lines.push(format!(
                        "{} {} {}",
                        "using:".dimmed(),
                        resolved.green().bold(),
                        format!("({owner})").dimmed()
                    ));
                }
            } else {
                context_lines.push(format!("{} {}", "owner:".dimmed(), owner.yellow()));
            }
        }
        if let Some(active) = info.active.as_ref() {
            context_lines.push(format!("{} {}", "gh default:".dimmed(), active.yellow()));
        }
        if !info.users.is_empty() {
            context_lines.push(format!(
                "{} {}",
                "accounts:".dimmed(),
                info.users.join(", ").dimmed()
            ));
        }
    }

    let update_request = update::CheckRequest {
        repo_slug: "nantokaworks/clix",
        tool_name: "ghx",
        current_version: env!("CARGO_PKG_VERSION"),
        disable_env_var: "GHX_NO_UPDATE_CHECK",
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
        build_date: env!("GHX_BUILD_DATE"),
        repository: env!("CARGO_PKG_REPOSITORY"),
        context_lines,
        update,
    };

    banner::print(&banner).map_err(|e| error::Error::ExecFailed(e.to_string()))
}

fn main() {
    if let Err(e) = run() {
        eprintln!("ghx: {e}");
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::{is_top_level_help, is_version_command, should_passthrough};

    #[test]
    fn no_args_is_not_passthrough() {
        assert!(!should_passthrough(&[]));
    }

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
    fn non_version_paths_are_not_detected() {
        for args in [
            vec!["help".to_string()],
            vec!["auth".to_string(), "status".to_string()],
            vec!["pr".to_string(), "status".to_string()],
        ] {
            assert!(!is_version_command(&args));
        }
    }

    #[test]
    fn passthrough_for_bootstrap_paths() {
        for args in [
            vec!["--help".to_string()],
            vec!["-h".to_string()],
            vec!["help".to_string()],
            vec!["auth".to_string(), "login".to_string()],
            vec!["auth".to_string(), "status".to_string()],
        ] {
            assert!(should_passthrough(&args));
        }
    }

    #[test]
    fn does_not_passthrough_repo_commands() {
        for args in [
            vec!["pr".to_string(), "status".to_string()],
            vec!["issue".to_string(), "list".to_string()],
            vec!["repo".to_string(), "view".to_string()],
        ] {
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
            vec!["help".to_string(), "repo".to_string()],
            vec!["pr".to_string(), "--help".to_string()],
        ] {
            assert!(!is_top_level_help(&args), "{args:?}");
        }
    }
}
