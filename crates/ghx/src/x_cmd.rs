use std::collections::BTreeMap;

use clix_core::git;

use crate::config;
use crate::error::Error;

const USAGE: &str = "usage: ghx x list\n\
                     \x20      ghx x bind <user> <owner>\n\
                     \x20      ghx x unbind <owner>\n\
                     \x20      ghx x whoami";

pub fn print_usage() {
    println!("{USAGE}");
}

pub fn print_extras_section() {
    println!();
    println!("ghx extras (wrapper-specific subcommands):");
    println!("  ghx x list                  list registered gh accounts and owner mappings");
    println!("  ghx x bind <user> <owner>   map a GitHub owner to a gh account");
    println!("  ghx x unbind <owner>        remove an owner mapping");
    println!("  ghx x whoami                show the resolved gh user for the current repo");
    println!("  ghx x --help                show this list");
}

pub fn print_bare_hint() {
    println!();
    println!("Tip: run `ghx x` for ghx-specific subcommands (list, bind, unbind, whoami).");
}

pub fn run(args: &[String]) -> Result<(), Error> {
    if args.is_empty() || is_help_arg(args) {
        print_usage();
        return Ok(());
    }
    match args {
        [cmd] if cmd == "list" => list(),
        [cmd, user, owner] if cmd == "bind" => bind(user, owner),
        [cmd, owner] if cmd == "unbind" => unbind(owner),
        [cmd] if cmd == "whoami" => whoami(),
        _ => Err(Error::InvalidXCommand(USAGE.to_string())),
    }
}

fn is_help_arg(args: &[String]) -> bool {
    matches!(args, [first] if matches!(first.as_str(), "--help" | "-h"))
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
