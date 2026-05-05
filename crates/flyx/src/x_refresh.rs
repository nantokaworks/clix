use crate::config;
use crate::error::Error;
use crate::fly_cli;

pub(crate) fn pick_primary(slugs: &[String]) -> Option<String> {
    match slugs {
        [single] => Some(single.clone()),
        many => many.iter().find(|s| s.as_str() == "personal").cloned(),
    }
}

pub(crate) fn refresh(profile_name: Option<&str>) -> Result<(), Error> {
    let mut cfg = config::read_config()?;

    let targets: Vec<String> = match profile_name {
        Some(name) => {
            if !cfg.profiles.contains_key(name) {
                return Err(Error::ProfileNotFound {
                    profile: name.to_string(),
                });
            }
            vec![name.to_string()]
        }
        None => cfg.profiles.keys().cloned().collect(),
    };

    if targets.is_empty() {
        eprintln!("flyx: no profiles to refresh");
        return Ok(());
    }

    let mut last_err: Option<Error> = None;
    let mut failures = 0usize;

    for name in &targets {
        let token = match cfg.profiles.get(name) {
            Some(p) => p.access_token.clone(),
            None => continue,
        };

        let email = match fly_cli::auth_whoami(&token) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("flyx: warning: refresh \"{name}\" failed: {e}");
                failures += 1;
                last_err = Some(e);
                continue;
            }
        };

        let org_slugs: Vec<String> = match fly_cli::orgs_list(&token) {
            Ok(map) => map.into_keys().collect(),
            Err(e) => {
                eprintln!("flyx: warning: refresh \"{name}\" failed: {e}");
                failures += 1;
                last_err = Some(e);
                continue;
            }
        };

        let apps = match fly_cli::apps_list(&token) {
            Ok(list) => list,
            Err(e) => {
                eprintln!(
                    "flyx: warning: refresh \"{name}\" — could not list apps ({e}); skipping app→profile cache"
                );
                Vec::new()
            }
        };

        let new_primary = pick_primary(&org_slugs);

        let profile = cfg
            .profiles
            .get_mut(name)
            .expect("profile existence checked above");
        profile.email = email.clone();
        profile.org_slugs = org_slugs.clone();
        // Preserve a user-set org_slug as long as it's still in the new list.
        // Replace it when it's None or has fallen out of the visible orgs.
        let keep = profile
            .org_slug
            .as_deref()
            .map(|cur| org_slugs.iter().any(|s| s == cur))
            .unwrap_or(false);
        if !keep {
            profile.org_slug = new_primary.clone();
        }

        for slug in &org_slugs {
            cfg.mappings
                .entry(slug.clone())
                .or_insert_with(|| name.clone());
        }
        for app in &apps {
            cfg.mappings
                .entry(app.name.clone())
                .or_insert_with(|| name.clone());
        }

        let primary_label = profile.org_slug.as_deref().unwrap_or("(unbound)");
        let email_label = email.as_deref().unwrap_or("-");
        eprintln!(
            "flyx: refreshed \"{name}\": email={email_label} org_slug={primary_label} orgs=[{}] apps={}",
            org_slugs.join(", "),
            apps.len()
        );
    }

    config::write_config(&cfg)?;

    if failures > 0 {
        if profile_name.is_some() {
            return Err(last_err.expect("failure recorded but error not captured"));
        }
        eprintln!("flyx: refresh completed with {failures} failure(s)");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::pick_primary;

    fn s(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn pick_primary_returns_single_slug() {
        assert_eq!(
            pick_primary(&s(&["nantokaworks"])),
            Some("nantokaworks".to_string())
        );
    }

    #[test]
    fn pick_primary_prefers_personal_among_many() {
        assert_eq!(
            pick_primary(&s(&["nantokaworks", "personal", "acme"])),
            Some("personal".to_string())
        );
    }

    #[test]
    fn pick_primary_none_when_empty() {
        assert_eq!(pick_primary(&[]), None);
    }

    #[test]
    fn pick_primary_none_when_many_without_personal() {
        // Conservative: only auto-pick when there's exactly one or "personal" is present.
        assert_eq!(pick_primary(&s(&["acme", "nantokaworks"])), None);
    }
}
