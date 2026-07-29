//! Color-mode detection per Doc 9 §12.5.

use clap::ValueEnum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum ColorMode {
    #[default]
    Auto,
    Always,
    Never,
}

impl ColorMode {
    /// Resolve to a concrete boolean: whether to emit ANSI color codes.
    pub fn should_color(self) -> bool {
        match self {
            ColorMode::Always => true,
            ColorMode::Never => false,
            ColorMode::Auto => {
                // Respect NO_COLOR convention (https://no-color.org/).
                if std::env::var("NO_COLOR").is_ok() {
                    return false;
                }
                // Only color when stderr is a real terminal.
                is_terminal()
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn is_terminal() -> bool {
    // Simplified: check Windows Terminal / TERM env var as a proxy.
    std::env::var("WT_SESSION").is_ok() || std::env::var("TERM").is_ok()
}

#[cfg(not(target_os = "windows"))]
fn is_terminal() -> bool {
    use std::os::unix::io::AsRawFd;
    let fd = std::io::stderr().as_raw_fd();
    libc_isatty(fd)
}

#[cfg(not(target_os = "windows"))]
fn libc_isatty(fd: i32) -> bool {
    extern "C" {
        fn isatty(fd: i32) -> i32;
    }
    unsafe { isatty(fd) != 0 }
}
