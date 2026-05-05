/// Args we extract from the CLI before handing the rest to `wrangler`.
///
/// `--profile` and `--account-id` are wranglerx-only and are consumed (removed
/// from `raw`). `--account-id` is replayed as `CLOUDFLARE_ACCOUNT_ID` on the
/// child env. Other flags pass through untouched.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ParsedArgs {
    pub profile_override: Option<String>,
    pub account_id_override: Option<String>,
    pub raw: Vec<String>,
}

pub fn parse(args: &[String]) -> ParsedArgs {
    let mut out = ParsedArgs::default();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];

        if let Some(value) = strip_flag(a, &["--profile"]) {
            if !value.is_empty() {
                out.profile_override = Some(value.to_string());
                i += 1;
                continue;
            }
            if let Some(next) = args.get(i + 1) {
                out.profile_override = Some(next.clone());
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }

        if let Some(value) = strip_flag(a, &["--account-id"]) {
            if !value.is_empty() {
                out.account_id_override = Some(value.to_string());
                i += 1;
                continue;
            }
            if let Some(next) = args.get(i + 1) {
                out.account_id_override = Some(next.clone());
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }

        out.raw.push(a.clone());
        i += 1;
    }
    out
}

/// If `arg` matches one of `flags`, return the value portion:
/// - exact match (`--profile`) → `Some("")`, caller looks at next arg
/// - `--profile=foo` form → `Some("foo")`
/// Returns `None` if `arg` is unrelated.
fn strip_flag<'a>(arg: &'a str, flags: &[&str]) -> Option<&'a str> {
    for &flag in flags {
        if arg == flag {
            return Some("");
        }
        if let Some(rest) = arg.strip_prefix(flag) {
            if let Some(value) = rest.strip_prefix('=') {
                return Some(value);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn empty_args_parses_clean() {
        let p = parse(&args(&[]));
        assert_eq!(p, ParsedArgs::default());
    }

    #[test]
    fn no_flags_passes_everything_through() {
        let p = parse(&args(&["deploy", "--remote-only"]));
        assert!(p.profile_override.is_none());
        assert!(p.account_id_override.is_none());
        assert_eq!(p.raw, args(&["deploy", "--remote-only"]));
    }

    #[test]
    fn profile_long_form_is_consumed() {
        let p = parse(&args(&["--profile", "work", "deploy"]));
        assert_eq!(p.profile_override.as_deref(), Some("work"));
        assert_eq!(p.raw, args(&["deploy"]));
    }

    #[test]
    fn profile_equals_form_is_consumed() {
        let p = parse(&args(&["--profile=work", "deploy"]));
        assert_eq!(p.profile_override.as_deref(), Some("work"));
        assert_eq!(p.raw, args(&["deploy"]));
    }

    #[test]
    fn account_id_long_form_is_consumed() {
        let p = parse(&args(&["--account-id", "abc123", "deploy"]));
        assert_eq!(p.account_id_override.as_deref(), Some("abc123"));
        assert_eq!(p.raw, args(&["deploy"]));
    }

    #[test]
    fn account_id_equals_form_is_consumed() {
        let p = parse(&args(&["--account-id=abc123", "deploy"]));
        assert_eq!(p.account_id_override.as_deref(), Some("abc123"));
        assert_eq!(p.raw, args(&["deploy"]));
    }

    #[test]
    fn profile_and_account_id_combined() {
        let p = parse(&args(&[
            "--profile",
            "work",
            "deploy",
            "--account-id",
            "abc123",
        ]));
        assert_eq!(p.profile_override.as_deref(), Some("work"));
        assert_eq!(p.account_id_override.as_deref(), Some("abc123"));
        assert_eq!(p.raw, args(&["deploy"]));
    }

    #[test]
    fn lone_profile_with_no_value_is_dropped() {
        let p = parse(&args(&["deploy", "--profile"]));
        assert_eq!(p.profile_override, None);
        assert_eq!(p.raw, args(&["deploy"]));
    }
}
