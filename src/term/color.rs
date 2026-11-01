//! Color policy and ANSI painting for the terminal report.

use std::io::IsTerminal;

use clap::ValueEnum;

/// When to color the terminal report. On the command line, each is its name in lower case.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum ColorMode {
    /// Color when the stream is a terminal; honor `NO_COLOR` and `CLICOLOR_FORCE`.
    Auto,
    /// Always color.
    Always,
    /// Never color.
    Never,
}

impl ColorMode {
    /// `Never` when `no_color` is true (the flag `--no-color`), else this mode.
    pub fn unless(self, no_color: bool) -> ColorMode {
        if no_color { ColorMode::Never } else { self }
    }
}

/// The stream that a text with color goes to. Each stream has its own color: with
/// `strata-preflight ... | less`, the progress lines on standard error can have color, and the
/// report on standard output has none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stream {
    /// Standard output: the report.
    Stdout,
    /// Standard error: the progress lines.
    Stderr,
}

impl Stream {
    fn is_terminal(self) -> bool {
        match self {
            Stream::Stdout => std::io::stdout().is_terminal(),
            Stream::Stderr => std::io::stderr().is_terminal(),
        }
    }
}

/// Applies ANSI colors, or nothing, depending on the resolved policy.
#[derive(Clone, Copy, Debug)]
pub struct Paint {
    on: bool,
}

fn env_set(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|v| !v.is_empty())
}

/// What decides the color under [`ColorMode::Auto`].
struct Environment {
    /// `NO_COLOR` has a value.
    no_color: bool,
    /// `CLICOLOR_FORCE` has a value.
    force: bool,
    /// `TERM` is `dumb`.
    dumb: bool,
    /// The stream is a terminal.
    terminal: bool,
}

impl Environment {
    fn read(stream: Stream) -> Environment {
        Environment {
            no_color: env_set("NO_COLOR"),
            force: env_set("CLICOLOR_FORCE"),
            dumb: std::env::var("TERM").is_ok_and(|t| t == "dumb"),
            terminal: stream.is_terminal(),
        }
    }

    /// `NO_COLOR` wins over `CLICOLOR_FORCE`, `CLICOLOR_FORCE` wins over `TERM=dumb`, and a
    /// terminal gets the color when none of them decides.
    fn colors(&self) -> bool {
        if self.no_color {
            false
        } else if self.force {
            true
        } else {
            !self.dumb && self.terminal
        }
    }
}

impl Paint {
    /// Never colors.
    pub fn off() -> Self {
        Paint { on: false }
    }

    /// Resolve a policy for `stream`. Auto is on when the stream is a terminal, unless
    /// `NO_COLOR` is set or `TERM` is `dumb`. `CLICOLOR_FORCE` turns it on for pipes, and
    /// `NO_COLOR` wins over `CLICOLOR_FORCE`.
    ///
    /// Both tools use this one rule. `strata` colors its report on standard output.
    /// `strata-preflight` colors its report on standard output and its progress lines on
    /// standard error.
    pub fn resolve(mode: ColorMode, stream: Stream) -> Self {
        let on = match mode {
            ColorMode::Always => true,
            ColorMode::Never => false,
            ColorMode::Auto => Environment::read(stream).colors(),
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
    /// Dim yellow: a detail of what yellow marks.
    pub fn dyellow(&self, s: &str) -> String {
        self.c("2;33", s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_and_always() {
        assert_eq!(Paint::off().red("x"), "x");
        let on = Paint::resolve(ColorMode::Always, Stream::Stdout);
        assert!(on.is_on());
        assert_eq!(on.red("x"), "\x1b[31mx\x1b[0m");
        assert_eq!(on.dyellow("x"), "\x1b[2;33mx\x1b[0m");
        assert!(!Paint::resolve(ColorMode::Never, Stream::Stderr).is_on());
    }

    fn colors(no_color: bool, force: bool, dumb: bool, terminal: bool) -> bool {
        Environment {
            no_color,
            force,
            dumb,
            terminal,
        }
        .colors()
    }

    #[test]
    fn auto_colors_a_terminal_and_colors_a_pipe_only_when_the_environment_forces_it() {
        assert!(colors(false, false, false, true), "a terminal");
        assert!(!colors(false, false, false, false), "a pipe");
        assert!(
            colors(false, true, false, false),
            "CLICOLOR_FORCE colors a pipe"
        );
        assert!(
            colors(false, true, true, false),
            "CLICOLOR_FORCE wins over TERM=dumb"
        );
        assert!(
            !colors(false, false, true, true),
            "TERM=dumb removes the color"
        );
        assert!(
            !colors(true, true, false, true),
            "NO_COLOR wins over CLICOLOR_FORCE"
        );
    }

    #[test]
    fn the_flag_no_color_turns_every_mode_to_never_and_no_other_flag_changes_a_mode() {
        for mode in [ColorMode::Auto, ColorMode::Always, ColorMode::Never] {
            assert_eq!(mode.unless(true), ColorMode::Never);
            assert_eq!(mode.unless(false), mode);
        }
    }
}
