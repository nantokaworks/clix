mod args;
mod auth;
mod auto_import;
mod config;
mod error;
mod fly_cli;
mod help;
mod resolve;
mod x_cmd;
mod x_refresh;
mod x_token;

use std::env;
use std::process::{self, Command};

use clix_core::banner;
use clix_core::exec::{self, ExecError, exec_replace};
use clix_core::update;
use colored::Colorize;

use config::trigger_source_label;

const FLY_AUTH_PASSTHROUGH: &[&str] = &["logout"];

fn run() -> Result<(), error::Error> {
    let raw: Vec<String> = env::args().skip(1).collect();

    if is_version_command(&raw) {
        return print_flyx_banner();
    }

    let parsed = args::parse(&raw);
    let mut cmd = Command::new("fly");
    cmd.args(&parsed.raw);

    if parsed.raw.is_empty() {
        print_flyx_banner()?;
        exec::write_or_exit_on_pipe_close(help::BARE_HINT);
        return run_fly(cmd);
    }

    if help::is_top_level_help(&parsed.raw) {
        return run_fly_with_extras(cmd);
    }

    if should_passthrough(&parsed.raw) {
        return run_fly(cmd);
    }

    if let [first, rest @ ..] = parsed.raw.as_slice() {
        if first == "x" {
            return x_cmd::run(rest);
        }
    }

    if let [first, second, rest @ ..] = parsed.raw.as_slice() {
        if first == "auth" && (second == "login" || second == "signup") {
            return auth::login(second, rest);
        }
    }

    if is_dry_run(&parsed.raw) {
        return resolve::print_dry_run(&parsed);
    }

    if resolve::has_fly_env_token() {
        return run_fly(cmd);
    }

    if let Some(profile_name) = parsed.profile_override.as_deref() {
        let token = resolve::lookup_profile_token(profile_name)?;
        cmd.env("FLY_API_TOKEN", token);
        return run_fly(cmd);
    }

    let (trigger, source) = resolve::resolve_trigger(&parsed)?;
    let resolved = resolve::resolve_profile(&trigger, &source)?;
    cmd.env("FLY_API_TOKEN", &resolved.access_token);

    run_fly(cmd)
}

fn run_fly(cmd: Command) -> Result<(), error::Error> {
    exec_replace(cmd).map_err(map_exec_err)
}

fn run_fly_with_extras(cmd: Command) -> Result<(), error::Error> {
    exec::run_with_trailer(cmd, help::EXTRAS_SECTION).map_err(map_exec_err)
}

fn map_exec_err(e: ExecError) -> error::Error {
    match e {
        ExecError::NotFound => error::Error::FlyNotFound,
        ExecError::Failed(msg) => error::Error::ExecFailed(msg),
    }
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
        [first, second, ..]
            if first == "auth" && FLY_AUTH_PASSTHROUGH.iter().any(|c| c == second) =>
        {
            true
        }
        _ => false,
    }
}

fn print_flyx_banner() -> Result<(), error::Error> {
    let ascii_art = [
        "███████╗██╗  ██╗   ██╗██╗  ██╗",
        "██╔════╝██║  ╚██╗ ██╔╝╚██╗██╔╝",
        "█████╗  ██║   ╚████╔╝  ╚███╔╝",
        "██╔══╝  ██║    ╚██╔╝   ██╔██╗",
        "██║     ███████╗██║   ██╔╝ ██╗",
        "╚═╝     ╚══════╝╚═╝   ╚═╝  ╚═╝",
    ];

    let mut context_lines: Vec<String> = Vec::new();
    if let Some((env_name, _)) = resolve::fly_env_token() {
        context_lines.push(format!(
            "{} {}",
            format!("{env_name}:").dimmed(),
            "(env override)".yellow()
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
        tool_name: "flyx",
        current_version: env!("CARGO_PKG_VERSION"),
        disable_env_var: "FLYX_NO_UPDATE_CHECK",
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
        build_date: env!("FLYX_BUILD_DATE"),
        repository: env!("CARGO_PKG_REPOSITORY"),
        context_lines,
        update,
    };

    banner::print(&banner).map_err(|e| error::Error::ExecFailed(e.to_string()))
}

fn main() {
    if let Err(e) = run() {
        eprintln!("flyx: {e}");
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::{is_dry_run, is_version_command, should_passthrough};

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
            vec!["auth".to_string(), "logout".to_string()],
        ] {
            assert!(should_passthrough(&args), "{args:?}");
        }
    }

    #[test]
    fn auth_login_and_signup_do_not_passthrough() {
        // login / signup are intercepted by auth::login for auto-snapshot.
        for args in [
            vec!["auth".to_string(), "login".to_string()],
            vec!["auth".to_string(), "signup".to_string()],
        ] {
            assert!(!should_passthrough(&args), "{args:?}");
        }
    }

    #[test]
    fn does_not_passthrough_flyx_x_commands() {
        for args in [
            vec!["x".to_string(), "save".to_string(), "p".to_string()],
            vec!["x".to_string(), "list".to_string()],
            vec!["x".to_string(), "whoami".to_string()],
            vec!["deploy".to_string()],
        ] {
            assert!(!should_passthrough(&args), "{args:?}");
        }
    }
}
