pub const X_USAGE: &str = "usage: wranglerx x list\n\
                           \x20      wranglerx x bind <profile> <trigger>\n\
                           \x20      wranglerx x unbind <trigger>\n\
                           \x20      wranglerx x use <profile>\n\
                           \x20      wranglerx x save <profile>\n\
                           \x20      wranglerx x remove <profile>\n\
                           \x20      wranglerx x refresh <profile>\n\
                           \x20      wranglerx x whoami [<profile>]\n";

pub const BARE_HINT: &str =
    "\nTip: run `wranglerx x` for wranglerx-specific subcommands (profile / mapping management).\n";

pub const EXTRAS_SECTION: &str = "\nwranglerx extras (wrapper-specific subcommands):\n\
    \x20 wranglerx x list                       list registered profiles and trigger mappings\n\
    \x20 wranglerx x bind <profile> <trigger>   map a trigger (account_id) to a profile\n\
    \x20 wranglerx x unbind <trigger>           remove a trigger mapping\n\
    \x20 wranglerx x use <profile>              set the default profile\n\
    \x20 wranglerx x save <profile>             snapshot wrangler OAuth credentials into a profile\n\
    \x20 wranglerx x remove <profile>           delete a profile\n\
    \x20 wranglerx x refresh <profile>          refresh the profile's OAuth access token\n\
    \x20 wranglerx x whoami [<profile>]         show profile details\n\
    \x20 wranglerx x --help                     show this list\n";

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
