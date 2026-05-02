use crate::config::{self, AccountConfig};
use crate::error::Error;

pub fn run(args: &[String]) -> Result<(), Error> {
    match args {
        [cmd, rest @ ..] if cmd == "add" => add(rest),
        [cmd, name] if cmd == "remove" => remove(name),
        [cmd] if cmd == "list" => list(),
        _ => Err(Error::InvalidAuthCommand(
            "usage: wranglerx auth add <name> <token> [--account-id <id>]\n       wranglerx auth remove <name>\n       wranglerx auth list".to_string(),
        )),
    }
}

fn add(args: &[String]) -> Result<(), Error> {
    if args.len() < 2 {
        return Err(Error::InvalidAuthCommand(
            "usage: wranglerx auth add <name> <token> [--account-id <id>]".to_string(),
        ));
    }

    let name = &args[0];
    let token = &args[1];
    let account_id = parse_account_id(&args[2..])?;
    let mut cfg = config::read_config()?;
    cfg.accounts.insert(
        name.clone(),
        AccountConfig {
            api_token: Some(token.clone()),
            account_id: account_id.clone(),
        },
    );
    if let Some(account_id) = account_id {
        cfg.mappings.insert(account_id, name.clone());
    }
    config::write_config(&cfg)?;
    eprintln!(
        "wranglerx: saved account \"{name}\" to {}",
        config::config_path()?.display()
    );
    Ok(())
}

fn parse_account_id(args: &[String]) -> Result<Option<String>, Error> {
    match args {
        [] => Ok(None),
        [flag, value] if flag == "--account-id" => Ok(Some(value.clone())),
        _ => Err(Error::InvalidAuthCommand(
            "usage: wranglerx auth add <name> <token> [--account-id <id>]".to_string(),
        )),
    }
}

fn remove(name: &str) -> Result<(), Error> {
    let mut cfg = config::read_config()?;
    if cfg.accounts.remove(name).is_none() {
        return Err(Error::AccountNotFound {
            account: name.to_string(),
        });
    }
    cfg.mappings.retain(|_, account_name| account_name != name);
    config::write_config(&cfg)?;
    eprintln!("wranglerx: removed account \"{name}\"");
    Ok(())
}

fn list() -> Result<(), Error> {
    let cfg = config::read_config()?;
    if cfg.accounts.is_empty() {
        println!("No accounts registered.");
        return Ok(());
    }

    for (name, account) in cfg.accounts {
        let account_id = account.account_id.as_deref().unwrap_or("-");
        let token = account
            .api_token
            .as_deref()
            .map(|token| token_label(&name, token))
            .unwrap_or_else(|| "-".to_string());
        println!("{name}\taccount_id={account_id}\ttoken={token}");
    }
    Ok(())
}

fn token_label(name: &str, token: &str) -> String {
    if token.starts_with("${") && token.ends_with('}') {
        token.to_string()
    } else {
        format!("accounts.{name}")
    }
}
