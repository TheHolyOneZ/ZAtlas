use std::io::IsTerminal;

use zatlas_core::findings::Severity;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    colour: bool,
}

impl Style {
    pub fn of_stdout() -> Self {
        let forced = std::env::var_os("CLICOLOR_FORCE").is_some_and(|v| v != "0");
        let suppressed = std::env::var_os("NO_COLOR").is_some()
            || std::env::var("TERM").is_ok_and(|t| t == "dumb");
        Self {
            colour: forced
                || (std::io::stdout().is_terminal() && !suppressed && console_understands_ansi()),
        }
    }

    pub fn plain() -> Self {
        Self { colour: false }
    }

    fn wrap(self, code: &str, text: &str) -> String {
        if self.colour {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_owned()
        }
    }

    pub fn dim(self, text: &str) -> String {
        self.wrap("2", text)
    }

    pub fn bold(self, text: &str) -> String {
        self.wrap("1", text)
    }

    pub fn accent(self, text: &str) -> String {
        self.wrap("36", text)
    }

    pub fn severity(self, severity: Severity, text: &str) -> String {
        self.wrap(
            match severity {
                Severity::Critical => "1;31",
                Severity::High => "31",
                Severity::Medium => "33",
                Severity::Low => "36",
                Severity::Info => "2",
            },
            text,
        )
    }
}

fn console_understands_ansi() -> bool {
    if !cfg!(windows) {
        return true;
    }

    ["WT_SESSION", "ConEmuANSI", "ANSICON", "TERM"]
        .iter()
        .any(|key| std::env::var_os(key).is_some())
}

pub fn severity_word(severity: Severity) -> &'static str {
    match severity {
        Severity::Critical => "crit",
        Severity::High => "high",
        Severity::Medium => "med ",
        Severity::Low => "low ",
        Severity::Info => "info",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_style_emits_no_escape_sequences() {
        let s = Style::plain();
        assert_eq!(s.bold("x"), "x");
        assert_eq!(s.severity(Severity::Critical, "x"), "x");
        assert!(!s.dim("x").contains('\x1b'));
    }

    #[test]
    fn severity_words_are_all_the_same_width_so_the_column_lines_up() {
        for severity in [
            Severity::Info,
            Severity::Low,
            Severity::Medium,
            Severity::High,
            Severity::Critical,
        ] {
            assert_eq!(severity_word(severity).len(), 4, "{severity:?}");
        }
    }
}
