mod config;
mod error;
mod git;

use std::env;
use std::io::{self, Write};
use std::process::{self, Command};

fn run() -> Result<(), error::Error> {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut cmd = Command::new("gh");
    cmd.args(&args);

    if is_version_command(&args) {
        print_ghx_version()?;
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
        [] => true,
        [first, ..] if matches!(first.as_str(), "--help" | "-h" | "help") => true,
        [first, ..] if first == "auth" => true,
        _ => false,
    }
}

fn print_ghx_version() -> Result<(), error::Error> {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "ghx {}", env!("CARGO_PKG_VERSION"))
        .map_err(|e| error::Error::ExecFailed(e.to_string()))?;
    stdout
        .flush()
        .map_err(|e| error::Error::ExecFailed(e.to_string()))
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
    fn passthrough_without_args() {
        assert!(should_passthrough(&[]));
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
