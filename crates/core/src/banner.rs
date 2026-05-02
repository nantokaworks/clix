use std::io::{self, Write};

use colored::Colorize;

pub struct Banner<'a> {
    pub ascii_art: &'a [&'a str],
    pub description: &'a str,
    pub version: &'a str,
    pub build_date: &'a str,
    pub repository: &'a str,
    /// 既に色付け済みの追加表示行（"using: alice" 等）。`│ ` で prefix されて出力される。
    pub context_lines: Vec<String>,
    pub update: Option<UpdateNotice>,
}

pub struct UpdateNotice {
    pub current: String,
    pub latest: String,
    pub command: String,
}

pub fn print(banner: &Banner<'_>) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    let bar = "│".dimmed();

    writeln!(stdout, "{}", "┌──────────────────────────────".dimmed())?;
    for line in banner.ascii_art {
        writeln!(stdout, "{bar} {line}")?;
    }
    writeln!(stdout, "{bar}")?;
    writeln!(stdout, "{bar} {}", banner.description.dimmed())?;
    writeln!(
        stdout,
        "{bar} {}",
        format!("version: {} ({})", banner.version, banner.build_date).dimmed()
    )?;
    writeln!(stdout, "{bar} {}", banner.repository.dimmed())?;

    if !banner.context_lines.is_empty() {
        writeln!(stdout, "{bar}")?;
        for line in &banner.context_lines {
            writeln!(stdout, "{bar} {line}")?;
        }
    }

    if let Some(update) = &banner.update {
        writeln!(stdout, "{bar}")?;
        writeln!(
            stdout,
            "{bar} {} {} → {}",
            "update available:".yellow().bold(),
            update.current.dimmed(),
            update.latest.green().bold()
        )?;
        writeln!(stdout, "{bar} {}", update.command.cyan())?;
    }

    writeln!(stdout, "{}", "└──────────────────────────────".dimmed())?;
    stdout.flush()
}
