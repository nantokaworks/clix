use crate::auto_import;
use crate::config;
use crate::error::Error;
use crate::fly_api;

pub(crate) fn pick_primary(slugs: &[String]) -> Option<String> {
    match slugs {
        [single] => Some(single.clone()),
        many => many.iter().find(|s| s.as_str() == "personal").cloned(),
    }
}

/// Best-effort: scan `~/.fly/config*.yml`, find one whose root macaroon
/// matches `token`'s root macaroon, and return the org slugs Fly cached under
/// `wire_guard_state`. First match wins. Errors collapse to an empty result —
/// this is a fallback path, not a failure-worthy operation.
///
/// Matches on the first comma-separated macaroon ("root") because Fly rotates
/// the trailing discharge tokens, so an identity-equal token can disagree
/// byte-for-byte with the snapshot we saved.
fn harvest_local_orgs_for_token(token: &str) -> Vec<String> {
    let target_root = root_macaroon(token);
    let files = match auto_import::discover_fly_config_files() {
        Ok(files) => files,
        Err(_) => return Vec::new(),
    };
    for path in files {
        let summary = match auto_import::read_fly_config_summary(&path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if summary.access_token.as_deref().map(root_macaroon) == Some(target_root) {
            return summary.wire_guard_orgs;
        }
    }
    Vec::new()
}

fn root_macaroon(token: &str) -> &str {
    token.split(',').next().unwrap_or(token)
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

        let mut viewer = match fly_api::fetch_viewer(&token) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("flyx: warning: refresh \"{name}\" failed: {e}");
                failures += 1;
                last_err = Some(e);
                continue;
            }
        };

        for slug in harvest_local_orgs_for_token(&token) {
            if !viewer.org_slugs.iter().any(|s| s == &slug) {
                viewer.org_slugs.push(slug);
            }
        }

        let new_primary = pick_primary(&viewer.org_slugs);

        let profile = cfg
            .profiles
            .get_mut(name)
            .expect("profile existence checked above");
        profile.email = viewer.email.clone();
        profile.org_slugs = viewer.org_slugs.clone();
        // Preserve a user-set org_slug as long as it's still in the new list.
        // Replace it when it's None or has fallen out of the visible orgs.
        let keep = profile
            .org_slug
            .as_deref()
            .map(|cur| viewer.org_slugs.iter().any(|s| s == cur))
            .unwrap_or(false);
        if !keep {
            profile.org_slug = new_primary.clone();
        }

        for slug in &viewer.org_slugs {
            cfg.mappings
                .entry(slug.clone())
                .or_insert_with(|| name.clone());
        }

        let primary_label = profile.org_slug.as_deref().unwrap_or("(unbound)");
        let email_label = viewer.email.as_deref().unwrap_or("-");
        eprintln!(
            "flyx: refreshed \"{name}\": email={email_label} org_slug={primary_label} orgs=[{}]",
            viewer.org_slugs.join(", ")
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
