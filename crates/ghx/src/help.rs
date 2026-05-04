pub const X_USAGE: &str = "usage: ghx x list\n\
                           \x20      ghx x bind <user> <owner>\n\
                           \x20      ghx x unbind <owner>\n\
                           \x20      ghx x whoami\n";

pub const BARE_HINT: &str =
    "\nTip: run `ghx x` for ghx-specific subcommands (list, bind, unbind, whoami).\n";

pub const EXTRAS_SECTION: &str = "\nghx extras (wrapper-specific subcommands):\n\
    \x20 ghx x list                  list registered gh accounts and owner mappings\n\
    \x20 ghx x bind <user> <owner>   map a GitHub owner to a gh account\n\
    \x20 ghx x unbind <owner>        remove an owner mapping\n\
    \x20 ghx x whoami                show the resolved gh user for the current repo\n\
    \x20 ghx x --help                show this list\n";

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
            vec!["help".to_string(), "repo".to_string()],
            vec!["pr".to_string(), "--help".to_string()],
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
