//! Color policy and ANSI painting for the terminal report.

use std::io::IsTerminal;

use clap::ValueEnum;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum ColorMode {
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, Debug)]
pub struct Paint {
    on: bool,
}

fn env_set(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|v| !v.is_empty())
}

impl Paint {
    pub fn off() -> Self {
        Paint { on: false }
    }

    /// auto: on when stdout is a terminal, unless NO_COLOR is set or TERM is dumb;
    /// CLICOLOR_FORCE turns it on for pipes; NO_COLOR wins over CLICOLOR_FORCE.
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

    pub fn is_on(&self) -> bool {
        self.on
    }

    pub fn c(&self, code: &str, s: &str) -> String {
        if self.on {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    pub fn bold(&self, s: &str) -> String {
        self.c("1", s)
    }
    pub fn dim(&self, s: &str) -> String {
        self.c("2", s)
    }
    pub fn red(&self, s: &str) -> String {
        self.c("31", s)
    }
    pub fn green(&self, s: &str) -> String {
        self.c("32", s)
    }
    pub fn yellow(&self, s: &str) -> String {
        self.c("33", s)
    }
    pub fn blue(&self, s: &str) -> String {
        self.c("34", s)
    }
    pub fn magenta(&self, s: &str) -> String {
        self.c("35", s)
    }
    pub fn cyan(&self, s: &str) -> String {
        self.c("36", s)
    }
    pub fn bred(&self, s: &str) -> String {
        self.c("1;31", s)
    }
    pub fn bgreen(&self, s: &str) -> String {
        self.c("1;32", s)
    }
    pub fn byellow(&self, s: &str) -> String {
        self.c("1;33", s)
    }
}
