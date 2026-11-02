//! Terminal output for `redisctl init`: colour helpers and the banner.
//!
//! Colours only when stdout is a terminal and NO_COLOR is unset or empty, Redis red at
//! the best depth the terminal offers (24-bit when COLORTERM says so, the nearest
//! 256-colour index otherwise).

use std::ffi::OsString;
use std::io::IsTerminal;

fn tty() -> bool {
    std::io::stdout().is_terminal()
}

fn colour_allowed(tty: bool, no_color: Option<OsString>) -> bool {
    tty && no_color.is_none_or(|value| value.is_empty())
}

/// Unit tests assert on plain text whether or not `cargo test` runs in a terminal.
fn colour() -> bool {
    !cfg!(test) && colour_allowed(tty(), std::env::var_os("NO_COLOR"))
}

fn paint(code: &str, s: &str) -> String {
    if colour() {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

pub fn bold(s: &str) -> String {
    paint("1", s)
}

pub fn dim(s: &str) -> String {
    paint("2", s)
}

pub fn yellow(s: &str) -> String {
    paint("33", s)
}

/// Success marker. Deliberately not red: the payoff must not share a hue with a failure.
pub fn ok(s: &str) -> String {
    paint("97;1", s)
}

pub fn red(s: &str) -> String {
    paint("31", s)
}

fn icon(status: crate::workflows::init::engine::Status) -> String {
    use crate::workflows::init::engine::Status;
    match status {
        Status::Created => brand_red("+"),
        Status::Updated => yellow("~"),
        Status::Unchanged => dim("="),
        Status::Kept => yellow("!"),
        Status::Skipped => dim("-"),
        Status::Planned => yellow("»"),
    }
}

/// One summary line: icon, status, subject, note.
pub fn change_line(change: &crate::workflows::init::engine::Change) -> String {
    let note = if change.note.is_empty() {
        String::new()
    } else {
        dim(&format!("  {}", change.note))
    };
    format!(
        "  {} {:<9} {}{}",
        icon(change.status),
        change.status.label(),
        change.subject,
        note
    )
}

/// A "doing X..." line that stays open until it is closed exactly once - with
/// " ready", " done", or nothing when an error message follows on its own line.
pub struct Progress {
    open: bool,
}

/// Status steps are rail entries, continuing the wizard's clack rail through the
/// apply phase instead of dropping to plain indented text.
fn progress_open_line(label: &str) -> String {
    format!("{}  {}", brand_red("◇"), dim(&format!("{label}...")))
}

/// A rail entry complete on one line, for steps that are not a "doing X..." wait.
pub fn step(label: &str) {
    println!("{}  {}", brand_red("◇"), dim(label));
}

/// An info line on the same rail column, so header facts between the questions
/// and the status steps keep the rail unbroken.
pub fn rail_line(s: &str) -> String {
    format!("{}  {}", brand_red("◇"), s)
}

/// A bare rail row: the connector that carries the line across gaps between
/// entries, clack-style.
pub fn rail_gap() -> String {
    brand_red("│")
}

/// The rail's closing corner, ending the step column before the summary.
pub fn rail_end(label: &str) -> String {
    format!("{}  {}", brand_red("└"), dim(label))
}

pub fn progress(label: &str) -> Progress {
    use std::io::Write;
    print!("{}", progress_open_line(label));
    let _ = std::io::stdout().flush();
    Progress { open: true }
}

impl Progress {
    pub fn done(&mut self, outcome: &str) {
        if !self.open {
            return;
        }
        self.open = false;
        if outcome.is_empty() {
            println!();
        } else {
            println!("{}", dim(outcome));
        }
    }
}

fn truecolor() -> bool {
    std::env::var("COLORTERM")
        .map(|v| v.contains("truecolor") || v.contains("24bit"))
        .unwrap_or(false)
}

fn brand(rgb: &str, idx: u8, s: &str) -> String {
    if truecolor() {
        paint(&format!("38;2;{rgb}"), s)
    } else {
        paint(&format!("38;5;{idx}"), s)
    }
}

fn brand_red(s: &str) -> String {
    brand("255;68;56", 203, s)
}

fn brand_edge(s: &str) -> String {
    brand("196;39;27", 160, s)
}

/// The wordmark: blocks filled in brand red, with the box-drawing characters that
/// trace their edge in the darker tone. TTY only, keeping piped output clean.
const WORDMARK: [&str; 6] = [
    "██████╗ ███████╗██████╗ ██╗███████╗",
    "██╔══██╗██╔════╝██╔══██╗██║██╔════╝",
    "██████╔╝█████╗  ██║  ██║██║███████╗",
    "██╔══██╗██╔══╝  ██║  ██║██║╚════██║",
    "██║  ██║███████╗██████╔╝██║███████║",
    "╚═╝  ╚═╝╚══════╝╚═════╝ ╚═╝╚══════╝",
];

/// How a wordmark character is coloured: the blocks get the fill tone, spaces stay
/// plain, everything else (the box-drawing edge) gets the darker tone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunKind {
    Block,
    Edge,
    Space,
}

fn classify(c: char) -> RunKind {
    match c {
        '█' | '▀' | '▄' | '▌' | '▐' => RunKind::Block,
        ' ' => RunKind::Space,
        _ => RunKind::Edge,
    }
}

/// Split a wordmark line into runs of like-classified characters, so colour is chosen
/// per run instead of per character.
fn split_runs(line: &str) -> Vec<(RunKind, String)> {
    let mut runs: Vec<(RunKind, String)> = Vec::new();
    for c in line.chars() {
        let kind = classify(c);
        match runs.last_mut() {
            Some((last, run)) if *last == kind => run.push(c),
            _ => runs.push((kind, c.to_string())),
        }
    }
    runs
}

pub fn banner() {
    if !tty() {
        return;
    }
    let art: Vec<String> = WORDMARK
        .iter()
        .map(|line| {
            split_runs(line)
                .into_iter()
                .map(|(kind, run)| match kind {
                    RunKind::Block => brand_red(&run),
                    RunKind::Edge => brand_edge(&run),
                    RunKind::Space => run,
                })
                .collect::<String>()
        })
        .collect();
    println!("\n{}\n", art.join("\n"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_runs_groups_blocks_and_edges() {
        assert_eq!(
            split_runs("██╔═██"),
            vec![
                (RunKind::Block, "██".to_string()),
                (RunKind::Edge, "╔═".to_string()),
                (RunKind::Block, "██".to_string()),
            ]
        );
    }

    #[test]
    fn rail_gap_and_end_close_the_line() {
        assert_eq!(rail_gap(), "│");
        assert_eq!(rail_end("done"), "└  done");
    }

    #[test]
    fn progress_lines_join_the_wizard_rail() {
        assert_eq!(
            progress_open_line("installing skills (npx skills add)"),
            "◇  installing skills (npx skills add)..."
        );
    }

    #[test]
    fn change_line_pads_the_status_and_appends_the_note() {
        let change = crate::workflows::init::engine::Change {
            subject: ".env".into(),
            status: crate::workflows::init::engine::Status::Created,
            note: "REDIS_URL".into(),
        };
        assert_eq!(change_line(&change), "  + created   .env  REDIS_URL");
    }

    #[test]
    fn change_line_without_a_note_has_no_trailing_spaces() {
        let change = crate::workflows::init::engine::Change {
            subject: ".gitignore".into(),
            status: crate::workflows::init::engine::Status::Unchanged,
            note: String::new(),
        };
        assert_eq!(change_line(&change), "  = unchanged .gitignore");
    }

    #[test]
    fn colour_needs_a_terminal_and_no_non_empty_no_color() {
        assert!(colour_allowed(true, None));
        assert!(colour_allowed(true, Some("".into())));
        assert!(!colour_allowed(true, Some("1".into())));
        assert!(!colour_allowed(false, None));
    }

    #[test]
    fn split_runs_keeps_spaces_uncoloured() {
        assert_eq!(
            split_runs("█ ╗"),
            vec![
                (RunKind::Block, "█".to_string()),
                (RunKind::Space, " ".to_string()),
                (RunKind::Edge, "╗".to_string()),
            ]
        );
    }
}
