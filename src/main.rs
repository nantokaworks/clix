mod config;
mod error;
mod git;
mod update;

use std::env;
use std::io::{self, Write};
use std::process::{self, Command};

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
        return exec_gh(cmd);
    }

    if should_passthrough(&args) {
        return exec_gh(cmd);
    }

    let owner = git::get_remote_owner()?;
    let gh_user = config::resolve_gh_user(&owner)?;
    let token = config::get_token(&gh_user)?;

    cmd.env("GH_TOKEN", &token);

    exec_gh(cmd)
}

fn is_version_command(args: &[String]) -> bool {
    matches!(args, [first, ..] if matches!(first.as_str(), "--version" | "version"))
}

fn should_passthrough(args: &[String]) -> bool {
    match args {
        [first, ..] if matches!(first.as_str(), "--help" | "-h" | "help") => true,
        [first, ..] if first == "auth" => true,
        _ => false,
    }
}

fn print_ghx_banner() -> Result<(), error::Error> {
    let w = |e: io::Error| error::Error::ExecFailed(e.to_string());
    let mut stdout = io::stdout().lock();

    let b = "│".dimmed();
    writeln!(stdout, "{}", "┌──────────────────────────────".dimmed()).map_err(w)?;
    writeln!(stdout, "{b}  ██████╗ ██╗  ██╗██╗  ██╗").map_err(w)?;
    writeln!(stdout, "{b} ██╔════╝ ██║  ██║╚██╗██╔╝").map_err(w)?;
    writeln!(stdout, "{b} ██║  ███╗███████║ ╚███╔╝").map_err(w)?;
    writeln!(stdout, "{b} ██║   ██║██╔══██║ ██╔██╗").map_err(w)?;
    writeln!(stdout, "{b} ╚██████╔╝██║  ██║██╔╝ ██╗").map_err(w)?;
    writeln!(stdout, "{b}  ╚═════╝ ╚═╝  ╚═╝╚═╝  ╚═╝").map_err(w)?;
    writeln!(stdout, "{b}").map_err(w)?;
    writeln!(stdout, "{b} {}", env!("CARGO_PKG_DESCRIPTION").dimmed()).map_err(w)?;
    writeln!(stdout, "{b} {}", format!("version: {} ({})", env!("CARGO_PKG_VERSION"), env!("GHX_BUILD_DATE")).dimmed()).map_err(w)?;
    writeln!(stdout, "{b} {}", env!("CARGO_PKG_REPOSITORY").dimmed()).map_err(w)?;
    if let Some(info) = config::get_account_info() {
        writeln!(stdout, "{b}").map_err(w)?;
        if let Some(ref active) = info.active {
            writeln!(stdout, "{b} {} {}", "active account:".dimmed(), active.green().bold()).map_err(w)?;
        }
        if !info.users.is_empty() {
            writeln!(stdout, "{b} {} {}", "accounts:".dimmed(), info.users.join(", ").yellow()).map_err(w)?;
        }
    }
    if let Some(info) = update::check_for_update() {
        writeln!(stdout, "{b}").map_err(w)?;
        writeln!(
            stdout, "{b} {} {} → {}",
            "update available:".yellow().bold(),
            env!("CARGO_PKG_VERSION").dimmed(),
            info.latest.green().bold()
        ).map_err(w)?;
        writeln!(stdout, "{b} {}", info.upgrade_cmd.cyan()).map_err(w)?;
    }
    writeln!(stdout, "{}", "└──────────────────────────────".dimmed()).map_err(w)?;
    stdout.flush().map_err(w)
}

/// Unix: exec でプロセスを置き換え（シグナル・stdout/stderr がそのまま透過）
#[cfg(unix)]
fn exec_gh(mut cmd: Command) -> Result<(), error::Error> {
    use std::os::unix::process::CommandExt;
    let err = cmd.exec();
    if err.kind() == std::io::ErrorKind::NotFound {
        Err(error::Error::GhNotFound)
    } else {
        Err(error::Error::ExecFailed(err.to_string()))
    }
}

/// Windows: spawn して終了コードを転送
#[cfg(windows)]
fn exec_gh(mut cmd: Command) -> Result<(), error::Error> {
    let status = cmd.status().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            error::Error::GhNotFound
        } else {
            error::Error::ExecFailed(e.to_string())
        }
    })?;
    process::exit(status.code().unwrap_or(1));
}

fn main() {
    if let Err(e) = run() {
        eprintln!("ghx: {e}");
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::{is_version_command, should_passthrough};

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
}
