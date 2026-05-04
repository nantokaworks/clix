use std::collections::BTreeMap;

use clix_core::git;

use crate::config;
use crate::error::Error;
use crate::help;

pub fn run(args: &[String]) -> Result<(), Error> {
    if args.is_empty() || help::is_x_help_arg(args) {
        clix_core::exec::write_or_exit_on_pipe_close(help::X_USAGE);
        return Ok(());
    }
    match args {
        [cmd] if cmd == "list" => list(),
        [cmd, user, owner] if cmd == "bind" => bind(user, owner),
        [cmd, owner] if cmd == "unbind" => unbind(owner),
        [cmd] if cmd == "whoami" => whoami(),
        _ => Err(Error::InvalidXCommand(help::X_USAGE.trim().to_string())),
    }
}

fn list() -> Result<(), Error> {
    println!("gh accounts:");
    match config::get_account_info() {
        Some(info) => {
            if info.users.is_empty() {
                println!("  (no users registered — run `gh auth login`)");
            }
            for user in &info.users {
                let marker = if info.active.as_deref() == Some(user.as_str()) {
                    "*"
                } else {
                    " "
                };
                println!("{marker} {user}");
            }
        }
        None => {
            println!("  (gh hosts.yml not found — run `gh auth login`)");
        }
    }

    let mappings: BTreeMap<String, String> =
        config::read_owner_mappings().into_iter().collect();
    if !mappings.is_empty() {
        println!("\nowner -> gh user mappings:");
        for (owner, user) in &mappings {
            println!("    {owner} -> {user}");
        }
    }
    Ok(())
}

fn bind(user: &str, owner: &str) -> Result<(), Error> {
    config::bind_owner_mapping(owner, user)?;
    eprintln!("ghx: bound {owner} -> {user}");
    Ok(())
}

fn unbind(owner: &str) -> Result<(), Error> {
    config::remove_owner_mapping(owner)?;
    eprintln!("ghx: unbound {owner}");
    Ok(())
}

fn whoami() -> Result<(), Error> {
    let owner = git::get_remote_owner()?;
    let user = config::resolve_gh_user_for_display(&owner);
    println!("owner: {owner}");
    match user {
        Some(u) => println!("gh user: {u}"),
        None => println!("gh user: (unresolved — run `ghx x bind <user> {owner}`)"),
    }
    Ok(())
}
