use crate::auto_import;
use crate::config::{self, Profile};
use crate::error::Error;
use crate::fly_cli;
use crate::help;
use crate::x_refresh;

pub fn run(args: &[String]) -> Result<(), Error> {
    if args.is_empty() || help::is_x_help_arg(args) {
        clix_core::exec::write_or_exit_on_pipe_close(help::X_USAGE);
        return Ok(());
    }
    match args {
        [cmd] if cmd == "list" => list(),
        [cmd, name] if cmd == "use" => use_default(name),
        [cmd, name] if cmd == "remove" => remove(name),
        [cmd] if cmd == "import" => import(),
        [cmd] if cmd == "refresh" => x_refresh::refresh(None),
        [cmd, name] if cmd == "refresh" => x_refresh::refresh(Some(name)),
        [cmd, name, token] if cmd == "save-token" => save_token(name, token),
        [cmd] if cmd == "whoami" => whoami(None),
        [cmd, name] if cmd == "whoami" => whoami(Some(name)),
        _ => Err(Error::InvalidAuthCommand(help::X_USAGE.trim().to_string())),
    }
}

fn import() -> Result<(), Error> {
    let mut cfg = config::read_config()?;
    let result = auto_import::run(&mut cfg)?;
    config::write_config(&cfg)?;
    if result.imported.is_empty() && result.skipped_existing.is_empty() {
        eprintln!("flyx: no fly config files found under ~/.fly/");
    } else {
        if !result.imported.is_empty() {
            eprintln!(
                "flyx: imported {} profile(s): {}",
                result.imported.len(),
                result.imported.join(", ")
            );
        }
        if !result.skipped_existing.is_empty() {
            eprintln!(
                "flyx: skipped {} existing profile(s): {}",
                result.skipped_existing.len(),
                result.skipped_existing.join(", ")
            );
        }
    }
    Ok(())
}

/// Register a profile from a token pasted on the command line. Used for
/// deploy / org-scoped tokens (e.g. `fly tokens create org -o <slug>`) that
/// don't come through the interactive `fly auth login` flow.
fn save_token(profile_name: &str, token: &str) -> Result<(), Error> {
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err(Error::FlyCliError {
            msg: "empty token; pass the macaroon string as the second argument".to_string(),
        });
    }

    let email = fly_cli::auth_whoami(&token).unwrap_or(None);
    let org_slugs: Vec<String> = fly_cli::orgs_list(&token)
        .map(|m| m.into_keys().collect())
        .unwrap_or_default();
    let apps = fly_cli::apps_list(&token).unwrap_or_default();

    let mut cfg = config::read_config()?;

    let primary_org = x_refresh::pick_primary(&org_slugs);

    cfg.profiles.insert(
        profile_name.to_string(),
        Profile {
            access_token: token,
            email: email.clone(),
            org_slug: primary_org.clone(),
            org_slugs: org_slugs.clone(),
        },
    );

    if cfg.default.is_none() {
        cfg.default = Some(profile_name.to_string());
    }

    if let Some(slug) = primary_org.as_deref() {
        cfg.mappings
            .entry(slug.to_string())
            .or_insert_with(|| profile_name.to_string());
    }
    for slug in &org_slugs {
        cfg.mappings
            .entry(slug.clone())
            .or_insert_with(|| profile_name.to_string());
    }
    // App names are globally unique on Fly — a fresh save-token is the
    // source of truth for the apps it can see, so stale entries are
    // overwritten.
    for app in &apps {
        cfg.mappings.insert(app.name.clone(), profile_name.to_string());
    }

    config::write_config(&cfg)?;

    eprintln!("flyx: ✓ saved profile \"{profile_name}\"");
    if let Some(email) = email.as_deref() {
        eprintln!("flyx: viewer email: {email}");
    }
    eprintln!(
        "flyx: orgs=[{}] apps={}",
        org_slugs.join(", "),
        apps.len()
    );
    Ok(())
}

fn list() -> Result<(), Error> {
    let mut cfg = config::read_config()?;
    auto_import::sync_with_fly_dir(&mut cfg)?;

    if cfg.profiles.is_empty() {
        println!("No profiles registered.");
        println!("Run `flyx auth login` to register your first profile.");
        return Ok(());
    }
    for (name, profile) in &cfg.profiles {
        let primary = profile.org_slug.as_deref().unwrap_or("-");
        let extras = profile
            .org_slugs
            .iter()
            .filter(|s| Some(s.as_str()) != profile.org_slug.as_deref())
            .cloned()
            .collect::<Vec<_>>()
            .join(",");
        let extras_label = if extras.is_empty() {
            String::new()
        } else {
            format!("+[{extras}]")
        };
        let marker = if cfg.default.as_deref() == Some(name.as_str()) {
            "*"
        } else {
            " "
        };
        let email = profile.email.as_deref().unwrap_or("-");
        println!("{marker} {name}\torg_slug={primary}{extras_label}\temail={email}");
    }
    if !cfg.mappings.is_empty() {
        println!("\nmappings (auto-populated cache):");
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
    eprintln!("flyx: default profile set to \"{profile_name}\"");
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
    eprintln!("flyx: removed profile \"{profile_name}\"");
    Ok(())
}

fn whoami(profile_name: Option<&str>) -> Result<(), Error> {
    let mut cfg = config::read_config()?;
    auto_import::sync_with_fly_dir(&mut cfg)?;

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
    println!("email: {}", profile.email.as_deref().unwrap_or("-"));
    println!(
        "org_slug: {}",
        profile.org_slug.as_deref().unwrap_or("(unbound)")
    );
    if profile.org_slug.is_none() {
        println!("hint: run `flyx x refresh {name}` to re-probe");
    }
    if !profile.org_slugs.is_empty() {
        println!("accessible_orgs: {}", profile.org_slugs.join(", "));
    }
    println!(
        "token: {}",
        crate::x_token::mask_token(&profile.access_token)
    );
    Ok(())
}
