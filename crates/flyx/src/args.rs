/// Args we extract from the CLI before handing the rest to `fly`.
///
/// `--profile` is flyx-only and is consumed (removed from `raw`).
/// `-a` / `--app` and `-o` / `--org` are observed but **kept in `raw`** so
/// `fly` still receives them — flyx only peeks at them as routing hints.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ParsedArgs {
    pub profile_override: Option<String>,
    pub explicit_app: Option<String>,
    pub explicit_org: Option<String>,
    pub raw: Vec<String>,
}

pub fn parse(args: &[String]) -> ParsedArgs {
    let mut out = ParsedArgs::default();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        // --profile is flyx-only — consume it.
        if let Some(value) = strip_flag(a, &["--profile"]) {
            if !value.is_empty() {
                out.profile_override = Some(value.to_string());
                i += 1;
                continue;
            }
            // bare "--profile <name>"
            if let Some(next) = args.get(i + 1) {
                out.profile_override = Some(next.clone());
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }

        // -a / --app and -o / --org are passed through to fly, but recorded.
        if let Some(value) = strip_flag(a, &["-a", "--app"]) {
            if !value.is_empty() {
                out.explicit_app = Some(value.to_string());
                out.raw.push(a.clone());
                i += 1;
                continue;
            }
            if let Some(next) = args.get(i + 1) {
                out.explicit_app = Some(next.clone());
                out.raw.push(a.clone());
                out.raw.push(next.clone());
                i += 2;
                continue;
            }
            out.raw.push(a.clone());
            i += 1;
            continue;
        }

        if let Some(value) = strip_flag(a, &["-o", "--org"]) {
            if !value.is_empty() {
                out.explicit_org = Some(value.to_string());
                out.raw.push(a.clone());
                i += 1;
                continue;
            }
            if let Some(next) = args.get(i + 1) {
                out.explicit_org = Some(next.clone());
                out.raw.push(a.clone());
                out.raw.push(next.clone());
                i += 2;
                continue;
            }
            out.raw.push(a.clone());
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
        assert!(p.explicit_app.is_none());
        assert!(p.explicit_org.is_none());
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
    fn dash_a_short_form_records_and_passes_through() {
        let p = parse(&args(&["logs", "-a", "myapp"]));
        assert_eq!(p.explicit_app.as_deref(), Some("myapp"));
        assert_eq!(p.raw, args(&["logs", "-a", "myapp"]));
    }

    #[test]
    fn dash_a_equals_form() {
        let p = parse(&args(&["status", "-a=myapp"]));
        assert_eq!(p.explicit_app.as_deref(), Some("myapp"));
        assert_eq!(p.raw, args(&["status", "-a=myapp"]));
    }

    #[test]
    fn long_app_form() {
        let p = parse(&args(&["logs", "--app", "myapp"]));
        assert_eq!(p.explicit_app.as_deref(), Some("myapp"));
        assert_eq!(p.raw, args(&["logs", "--app", "myapp"]));
    }

    #[test]
    fn dash_o_records_and_passes_through() {
        let p = parse(&args(&["apps", "create", "-o", "personal", "newthing"]));
        assert_eq!(p.explicit_org.as_deref(), Some("personal"));
        assert_eq!(
            p.raw,
            args(&["apps", "create", "-o", "personal", "newthing"])
        );
    }

    #[test]
    fn long_org_form_equals() {
        let p = parse(&args(&["apps", "create", "--org=work"]));
        assert_eq!(p.explicit_org.as_deref(), Some("work"));
        assert_eq!(p.raw, args(&["apps", "create", "--org=work"]));
    }

    #[test]
    fn profile_and_app_and_org_combined() {
        let p = parse(&args(&[
            "--profile",
            "work",
            "logs",
            "-a",
            "myapp",
            "-o",
            "personal",
        ]));
        assert_eq!(p.profile_override.as_deref(), Some("work"));
        assert_eq!(p.explicit_app.as_deref(), Some("myapp"));
        assert_eq!(p.explicit_org.as_deref(), Some("personal"));
        assert_eq!(p.raw, args(&["logs", "-a", "myapp", "-o", "personal"]));
    }

    #[test]
    fn lone_dash_a_with_no_value_is_left_alone() {
        let p = parse(&args(&["logs", "-a"]));
        assert_eq!(p.explicit_app, None);
        assert_eq!(p.raw, args(&["logs", "-a"]));
    }
}
