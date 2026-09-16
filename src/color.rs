//! Color policy and ANSI painting for the terminal report.

use std::io::IsTerminal;

/// When to color the terminal report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode {
    /// Color when stdout is a terminal; honor `NO_COLOR` and `CLICOLOR_FORCE`.
    Auto,
    /// Always color.
    Always,
    /// Never color.
    Never,
}

/// Applies ANSI colors, or nothing, depending on the resolved policy.
#[derive(Clone, Copy, Debug)]
pub struct Paint {
    on: bool,
}

fn env_set(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|v| !v.is_empty())
}

impl Paint {
    /// Never colors.
    pub fn off() -> Self {
        Paint { on: false }
    }

    /// Resolve a policy: auto is on when stdout is a terminal, unless `NO_COLOR` is set or `TERM`
    /// is `dumb`; `CLICOLOR_FORCE` turns it on for pipes; `NO_COLOR` wins over `CLICOLOR_FORCE`.
    pub fn resolve(mode: ColorMode) -> Self {
        let on = match mode {
            ColorMode::Always => true,
            ColorMode::Never => false,
            ColorMode::Auto => {
                if env_set("NO_COLOR") {
                    false
                } else if env_set("CLICOLOR_FORCE") {
                    true
                } else if std::env::var("TERM").is_ok_and(|t| t == "dumb") {
                    false
                } else {
                    std::io::stdout().is_terminal()
                }
            }
        };
        Paint { on }
    }

    /// Whether colors are applied.
    pub fn is_on(&self) -> bool {
        self.on
    }

    /// Wrap `s` in the SGR `code` when on.
    pub fn c(&self, code: &str, s: &str) -> String {
        if self.on {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    /// Bold.
    pub fn bold(&self, s: &str) -> String {
        self.c("1", s)
    }
    /// Dim.
    pub fn dim(&self, s: &str) -> String {
        self.c("2", s)
    }
    /// Red.
    pub fn red(&self, s: &str) -> String {
        self.c("31", s)
    }
    /// Green.
    pub fn green(&self, s: &str) -> String {
        self.c("32", s)
    }
    /// Yellow.
    pub fn yellow(&self, s: &str) -> String {
        self.c("33", s)
    }
    /// Blue.
    pub fn blue(&self, s: &str) -> String {
        self.c("34", s)
    }
    /// Magenta.
    pub fn magenta(&self, s: &str) -> String {
        self.c("35", s)
    }
    /// Cyan.
    pub fn cyan(&self, s: &str) -> String {
        self.c("36", s)
    }
    /// Bold red.
    pub fn bred(&self, s: &str) -> String {
        self.c("1;31", s)
    }
    /// Bold green.
    pub fn bgreen(&self, s: &str) -> String {
        self.c("1;32", s)
    }
    /// Bold yellow.
    pub fn byellow(&self, s: &str) -> String {
        self.c("1;33", s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_and_always() {
        assert_eq!(Paint::off().red("x"), "x");
        let on = Paint::resolve(ColorMode::Always);
        assert!(on.is_on());
        assert_eq!(on.red("x"), "\x1b[31mx\x1b[0m");
        assert!(!Paint::resolve(ColorMode::Never).is_on());
    }
}
