pub const X_USAGE: &str = "usage: flyx x list\n\
                           \x20      flyx x bind <profile> <trigger>\n\
                           \x20      flyx x unbind <trigger>\n\
                           \x20      flyx x use <profile>\n\
                           \x20      flyx x save <profile>\n\
                           \x20      flyx x remove <profile>\n\
                           \x20      flyx x import\n\
                           \x20      flyx x refresh [<profile>]\n\
                           \x20      flyx x whoami [<profile>]\n";

pub const BARE_HINT: &str =
    "\nTip: run `flyx x` for flyx-specific subcommands (profile / mapping management).\n";

pub const EXTRAS_SECTION: &str = "\nflyx extras (wrapper-specific subcommands):\n\
    \x20 flyx x list                       list registered profiles and trigger mappings\n\
    \x20 flyx x bind <profile> <trigger>   map a trigger (org / app) to a profile\n\
    \x20 flyx x unbind <trigger>           remove a trigger mapping\n\
    \x20 flyx x use <profile>              set the default profile\n\
    \x20 flyx x save <profile>             snapshot ~/.fly/config.yml into a profile\n\
    \x20 flyx x remove <profile>           delete a profile\n\
    \x20 flyx x import                     auto-import profiles from ~/.fly/\n\
    \x20 flyx x refresh [<profile>]        re-probe org info using saved tokens\n\
    \x20 flyx x whoami [<profile>]         show profile details\n\
    \x20 flyx x --help                     show this list\n";

pub fn is_top_level_help(args: &[String]) -> bool {
    matches!(args, [first] if matches!(first.as_str(), "--help" | "-h" | "help"))
}

pub fn is_x_help_arg(args: &[String]) -> bool {
    matches!(args, [first] if matches!(first.as_str(), "--help" | "-h"))
}

#[cfg(test)]
mod tests {
    use super::{is_top_level_help, is_x_help_arg};

    #[test]
    fn detects_top_level_help_only() {
        for args in [
            vec!["--help".to_string()],
            vec!["-h".to_string()],
            vec!["help".to_string()],
        ] {
            assert!(is_top_level_help(&args), "{args:?}");
        }
        for args in [
            vec![],
            vec!["help".to_string(), "deploy".to_string()],
            vec!["deploy".to_string(), "--help".to_string()],
        ] {
            assert!(!is_top_level_help(&args), "{args:?}");
        }
    }

    #[test]
    fn detects_x_help_arg() {
        for args in [vec!["--help".to_string()], vec!["-h".to_string()]] {
            assert!(is_x_help_arg(&args), "{args:?}");
        }
        for args in [
            vec![],
            vec!["help".to_string()],
            vec!["list".to_string(), "--help".to_string()],
        ] {
            assert!(!is_x_help_arg(&args), "{args:?}");
        }
    }
}
