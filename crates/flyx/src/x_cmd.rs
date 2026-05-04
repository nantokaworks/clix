use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

use crate::auto_import;
use crate::config::{self, Profile};
use crate::error::Error;
use crate::fly_api;

const USAGE: &str = "usage: flyx x list\n\
                     \x20      flyx x bind <profile> <trigger>\n\
                     \x20      flyx x unbind <trigger>\n\
                     \x20      flyx x use <profile>\n\
                     \x20      flyx x save <profile>\n\
                     \x20      flyx x remove <profile>\n\
                     \x20      flyx x import\n\
                     \x20      flyx x whoami [<profile>]";

pub fn print_usage() {
    println!("{USAGE}");
}

pub fn print_extras_section() {
    println!();
    println!("flyx extras (wrapper-specific subcommands):");
    println!("  flyx x list                       list registered profiles and trigger mappings");
    println!("  flyx x bind <profile> <trigger>   map a trigger (org / app) to a profile");
    println!("  flyx x unbind <trigger>           remove a trigger mapping");
    println!("  flyx x use <profile>              set the default profile");
    println!("  flyx x save <profile>             snapshot ~/.fly/config.yml into a profile");
    println!("  flyx x remove <profile>           delete a profile");
    println!("  flyx x import                     auto-import profiles from ~/.fly/");
    println!("  flyx x whoami [<profile>]         show profile details");
    println!("  flyx x --help                     show this list");
}

pub fn print_bare_hint() {
    println!();
    println!("Tip: run `flyx x` for flyx-specific subcommands (profile / mapping management).");
}

pub fn run(args: &[String]) -> Result<(), Error> {
    if args.is_empty() || is_help_arg(args) {
        print_usage();
        return Ok(());
    }
    match args {
        [cmd] if cmd == "list" => list(),
        [cmd, name, trigger] if cmd == "bind" => bind(name, trigger),
        [cmd, trigger] if cmd == "unbind" => unbind(trigger),
        [cmd, name] if cmd == "use" => use_default(name),
        [cmd, name] if cmd == "save" => save(name),
        [cmd, name] if cmd == "remove" => remove(name),
        [cmd] if cmd == "import" => import(),
        [cmd] if cmd == "whoami" => whoami(None),
        [cmd, name] if cmd == "whoami" => whoami(Some(name)),
        _ => Err(Error::InvalidAuthCommand(USAGE.to_string())),
    }
}

fn is_help_arg(args: &[String]) -> bool {
    matches!(args, [first] if matches!(first.as_str(), "--help" | "-h"))
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

#[derive(Deserialize)]
struct FlyConfigYml {
    #[serde(default)]
    access_token: Option<String>,
}

fn read_fly_access_token() -> Result<(PathBuf, String), Error> {
    let candidates = candidate_fly_config_paths()?;
    for path in &candidates {
        if path.exists() {
            let content = fs::read_to_string(path).map_err(|e| Error::FlyConfigParse {
                path: path.clone(),
                msg: e.to_string(),
            })?;
            let parsed: FlyConfigYml =
                serde_yml::from_str(&content).map_err(|e| Error::FlyConfigParse {
                    path: path.clone(),
                    msg: e.to_string(),
                })?;
            let token = parsed
                .access_token
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .ok_or_else(|| Error::FlyTokenMissing { path: path.clone() })?;
            return Ok((path.clone(), token));
        }
    }
    Err(Error::FlyConfigMissing {
        searched: candidates,
    })
}

fn candidate_fly_config_paths() -> Result<Vec<PathBuf>, Error> {
    let home = dirs::home_dir().ok_or(Error::ConfigDirUnavailable)?;
    Ok(vec![home.join(".fly").join("config.yml")])
}

fn save(profile_name: &str) -> Result<(), Error> {
    let (cfg_path, token) = read_fly_access_token()?;

    let viewer = match fly_api::fetch_viewer(&token) {
        Ok(v) => v,
        Err(e) => {
            eprintln!(
                "flyx: warning: could not fetch viewer info ({e}); saving without org details"
            );
            fly_api::ViewerInfo {
                email: None,
                org_slugs: Vec::new(),
            }
        }
    };

    let mut cfg = config::read_config()?;

    let primary_org = match viewer.org_slugs.as_slice() {
        [single] => Some(single.clone()),
        many => many.iter().find(|s| s.as_str() == "personal").cloned(),
    };

    let profile = Profile {
        access_token: token,
        email: viewer.email.clone(),
        org_slug: primary_org.clone(),
        org_slugs: viewer.org_slugs.clone(),
    };

    cfg.profiles.insert(profile_name.to_string(), profile);

    if cfg.default.is_none() {
        cfg.default = Some(profile_name.to_string());
    }

    if let Some(slug) = primary_org.as_deref() {
        cfg.mappings
            .entry(slug.to_string())
            .or_insert_with(|| profile_name.to_string());
    }

    config::write_config(&cfg)?;

    eprintln!(
        "flyx: saved profile \"{profile_name}\" from {} to {}",
        cfg_path.display(),
        config::config_path()?.display()
    );
    if let Some(email) = viewer.email.as_deref() {
        eprintln!("flyx: viewer email: {email}");
    }
    match viewer.org_slugs.as_slice() {
        [] => eprintln!(
            "flyx: no orgs probed; bind manually with `flyx x bind {profile_name} <trigger>`"
        ),
        [single] => eprintln!("flyx: bound org_slug={single}"),
        many => {
            eprintln!(
                "flyx: token has access to {} orgs (primary={}); override with `flyx x bind {profile_name} <trigger>`",
                many.len(),
                primary_org.as_deref().unwrap_or("(none)")
            );
            for slug in many {
                eprintln!("    {slug}");
            }
        }
    }
    Ok(())
}

fn list() -> Result<(), Error> {
    let cfg = config::read_config()?;
    if cfg.profiles.is_empty() {
        println!("No profiles registered.");
        println!("Run `fly auth login` then `flyx x save <profile>` to register the first one.");
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
    eprintln!("flyx: default profile set to \"{profile_name}\"");
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

    config::write_config(&cfg)?;
    eprintln!("flyx: bound {trigger} -> {profile_name}");
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
    eprintln!("flyx: unbound {trigger}");
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
    println!("email: {}", profile.email.as_deref().unwrap_or("-"));
    println!(
        "org_slug: {}",
        profile.org_slug.as_deref().unwrap_or("(unbound)")
    );
    if !profile.org_slugs.is_empty() {
        println!("accessible_orgs: {}", profile.org_slugs.join(", "));
    }
    println!("token: {}", mask_token(&profile.access_token));
    Ok(())
}

fn mask_token(token: &str) -> String {
    if token.len() <= 12 {
        return "*".repeat(token.len());
    }
    let head = &token[..6];
    let tail = &token[token.len() - 4..];
    format!("{head}…{tail}")
}
