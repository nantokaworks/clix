use crate::cloudflare_api;
use crate::config::{self, Profile, ProfilesConfig};
use crate::error::Error;
use crate::help;
use crate::oauth;

pub fn run(args: &[String]) -> Result<(), Error> {
    if args.is_empty() || help::is_x_help_arg(args) {
        clix_core::exec::write_or_exit_on_pipe_close(help::X_USAGE);
        return Ok(());
    }
    match args {
        [cmd] if cmd == "list" => list(),
        [cmd, name, trigger] if cmd == "bind" => bind(name, trigger),
        [cmd, trigger] if cmd == "unbind" => unbind(trigger),
        [cmd, name] if cmd == "use" => use_default(name),
        [cmd, name] if cmd == "save" => save(name),
        [cmd, name] if cmd == "remove" => remove(name),
        [cmd, name] if cmd == "refresh" => refresh(name),
        [cmd] if cmd == "whoami" => whoami(None),
        [cmd, name] if cmd == "whoami" => whoami(Some(name)),
        _ => Err(Error::InvalidAuthCommand(help::X_USAGE.trim().to_string())),
    }
}

fn save(profile_name: &str) -> Result<(), Error> {
    let (cred_path, creds) = oauth::read_default_credentials()?;

    let probed = match cloudflare_api::list_account_ids(&creds.oauth_token) {
        Ok(ids) => ids,
        Err(e) => {
            eprintln!(
                "wranglerx: warning: could not list accounts ({e}); save without account_ids"
            );
            Vec::new()
        }
    };

    let mut cfg = config::read_config().or_else(|err| match err {
        Error::LegacyAccountsConfig { .. } => Ok(ProfilesConfig::default()),
        other => Err(other),
    })?;

    let primary_account_id = match probed.as_slice() {
        [single] => Some(single.clone()),
        _ => None,
    };

    let profile = Profile {
        access_token: creds.oauth_token,
        refresh_token: creds.refresh_token,
        expiration_time: creds.expiration_time,
        account_id: primary_account_id.clone(),
        scopes: creds.scopes,
        account_ids: probed.clone(),
    };

    cfg.profiles.insert(profile_name.to_string(), profile);

    if let Some(id) = primary_account_id.as_deref() {
        cfg.mappings.insert(id.to_string(), profile_name.to_string());
    }

    config::write_config(&cfg)?;

    eprintln!(
        "wranglerx: saved profile \"{profile_name}\" from {} to {}",
        cred_path.display(),
        config::config_path()?.display()
    );

    match probed.as_slice() {
        [] => eprintln!(
            "wranglerx: no accounts probed; bind manually with `wranglerx auth bind {profile_name} --account-id <id>`"
        ),
        [single] => eprintln!("wranglerx: bound account_id={single}"),
        many => {
            eprintln!(
                "wranglerx: token has access to {} accounts; pick one with `wranglerx auth bind {profile_name} --account-id <id>`",
                many.len()
            );
            for id in many {
                eprintln!("    {id}");
            }
        }
    }
    Ok(())
}

fn list() -> Result<(), Error> {
    let cfg = config::read_config()?;
    if cfg.profiles.is_empty() {
        println!("No profiles registered.");
        println!(
            "Run `wrangler login` then `wranglerx auth save <profile>` to register the first one."
        );
        return Ok(());
    }
    for (name, profile) in &cfg.profiles {
        let primary = profile.account_id.as_deref().unwrap_or("-");
        let access = profile
            .account_ids
            .iter()
            .filter(|id| Some(id.as_str()) != profile.account_id.as_deref())
            .cloned()
            .collect::<Vec<_>>()
            .join(",");
        let extra = if access.is_empty() {
            String::new()
        } else {
            format!("+[{access}]")
        };
        let marker = if cfg.default.as_deref() == Some(name.as_str()) {
            "*"
        } else {
            " "
        };
        println!(
            "{marker} {name}\taccount_id={primary}{extra}\texpires={}",
            profile.expiration_time
        );
    }
    if !cfg.mappings.is_empty() {
        println!("\nmappings:");
        for (trigger, profile) in &cfg.mappings {
            println!("    {trigger} -> {profile}");
        }
    }
    if let Some(default) = &cfg.default {
        println!("\ndefault: {default}");
    }
    Ok(())
}

fn use_default(profile_name: &str) -> Result<(), Error> {
    let mut cfg = config::read_config()?;
    if !cfg.profiles.contains_key(profile_name) {
        return Err(Error::ProfileNotFound {
            profile: profile_name.to_string(),
        });
    }
    cfg.default = Some(profile_name.to_string());
    config::write_config(&cfg)?;
    eprintln!("wranglerx: default profile set to \"{profile_name}\"");
    Ok(())
}

fn bind(profile_name: &str, trigger: &str) -> Result<(), Error> {
    let mut cfg = config::read_config()?;
    if !cfg.profiles.contains_key(profile_name) {
        return Err(Error::ProfileNotFound {
            profile: profile_name.to_string(),
        });
    }

    cfg.mappings
        .insert(trigger.to_string(), profile_name.to_string());

    if let Some(profile) = cfg.profiles.get_mut(profile_name) {
        if profile.account_id.is_none() {
            profile.account_id = Some(trigger.to_string());
        }
        if !profile.account_ids.iter().any(|id| id == trigger) {
            profile.account_ids.push(trigger.to_string());
        }
    }

    config::write_config(&cfg)?;
    eprintln!("wranglerx: bound {trigger} -> {profile_name}");
    Ok(())
}

fn unbind(trigger: &str) -> Result<(), Error> {
    let mut cfg = config::read_config()?;
    if cfg.mappings.remove(trigger).is_none() {
        return Err(Error::UnknownMapping {
            trigger: trigger.to_string(),
        });
    }
    config::write_config(&cfg)?;
    eprintln!("wranglerx: unbound {trigger}");
    Ok(())
}

fn remove(profile_name: &str) -> Result<(), Error> {
    let mut cfg = config::read_config()?;
    if cfg.profiles.remove(profile_name).is_none() {
        return Err(Error::ProfileNotFound {
            profile: profile_name.to_string(),
        });
    }
    cfg.mappings.retain(|_, name| name != profile_name);
    if cfg.default.as_deref() == Some(profile_name) {
        cfg.default = None;
    }
    config::write_config(&cfg)?;
    eprintln!("wranglerx: removed profile \"{profile_name}\"");
    Ok(())
}

fn refresh(profile_name: &str) -> Result<(), Error> {
    let mut cfg = config::read_config()?;
    let token = cfg
        .profiles
        .get(profile_name)
        .map(|p| p.refresh_token.clone())
        .ok_or_else(|| Error::ProfileNotFound {
            profile: profile_name.to_string(),
        })?;
    let new = oauth::refresh(&token)?;
    if let Some(profile) = cfg.profiles.get_mut(profile_name) {
        profile.access_token = new.access_token;
        profile.refresh_token = new.refresh_token;
        profile.expiration_time = new.expiration_time.clone();
        if let Some(scopes) = new.scopes {
            if !scopes.is_empty() {
                profile.scopes = scopes;
            }
        }
    }
    config::write_config(&cfg)?;
    eprintln!(
        "wranglerx: refreshed profile \"{profile_name}\" (expires {})",
        new.expiration_time
    );
    Ok(())
}

fn whoami(profile_name: Option<&str>) -> Result<(), Error> {
    let cfg = config::read_config()?;
    let name = match profile_name {
        Some(n) => n.to_string(),
        None => cfg.default.clone().ok_or(Error::NoDefaultProfile)?,
    };
    let profile = cfg
        .profiles
        .get(&name)
        .ok_or_else(|| Error::ProfileNotFound {
            profile: name.clone(),
        })?;
    println!("profile: {name}");
    println!(
        "account_id: {}",
        profile.account_id.as_deref().unwrap_or("(unbound)")
    );
    println!("expires: {}", profile.expiration_time);
    if !profile.account_ids.is_empty() {
        println!("accessible_accounts: {}", profile.account_ids.join(", "));
    }
    if !profile.scopes.is_empty() {
        println!("scopes: {}", profile.scopes.join(", "));
    }
    Ok(())
}
