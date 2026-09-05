//! Shared terminal rendering, morph, palette and stage machinery for the
//! Omarchy screensavers.

pub mod canvas;
pub mod config;
pub mod morph;
pub mod palette;
pub mod rng;
pub mod stage;
pub mod target;

use std::io::Write;

/// Writes a diagnostic line to stderr, ignoring any failure to deliver it.
///
/// `eprintln!` panics when the write fails, and with `panic = "abort"` that
/// becomes a SIGABRT and a core dump. Our stderr is the terminal the
/// screensaver draws in, and the lock screen tears that down without warning;
/// writes to the dead pty then fail with EIO. A diagnostic we cannot deliver
/// is not worth crashing over.
pub fn warn(args: std::fmt::Arguments) {
    let _ = writeln!(std::io::stderr(), "{args}");
}

/// `print!`/`println!` for a process whose stdout may already be gone.
///
/// Same hazard as [`warn`]: the standard macros panic when the write fails,
/// and `panic = "abort"` turns that into a SIGABRT. These paths only run for
/// `--help`, `--list`, `--verify` and `--show-palette`, but there is no reason
/// for them to be the one way left to abort on a terminal that went away.
///
/// Fully qualified, so callers need no `use std::io::Write`.
#[macro_export]
macro_rules! say {
    ($($arg:tt)*) => {{
        let _ = std::io::Write::write_fmt(
            &mut std::io::stdout(),
            format_args!($($arg)*),
        );
    }};
}

/// [`say!`] with a trailing newline.
#[macro_export]
macro_rules! sayln {
    () => {{
        let _ = std::io::Write::write_all(&mut std::io::stdout(), b"\n");
    }};
    ($($arg:tt)*) => {{
        let _ = std::io::Write::write_fmt(
            &mut std::io::stdout(),
            format_args!("{}\n", format_args!($($arg)*)),
        );
    }};
}

/// True when a write failed because whatever we were drawing on has gone away.
/// Closing a pty gives EIO; closing a pipe gives EPIPE.
pub fn terminal_is_gone(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::BrokenPipe || e.raw_os_error() == Some(libc::EIO)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Losing the terminal is how the screensaver normally ends -- the lock
    /// screen takes the window away mid-draw -- so it must not be mistaken
    /// for a failure worth reporting. Reporting it is what used to abort.
    #[test]
    fn a_dead_terminal_is_recognised_as_a_normal_shutdown() {
        let eio = std::io::Error::from_raw_os_error(libc::EIO);
        assert!(terminal_is_gone(&eio), "a closed pty gives EIO");

        let epipe = std::io::Error::from_raw_os_error(libc::EPIPE);
        assert!(terminal_is_gone(&epipe), "a closed pipe gives EPIPE");
    }

    /// The risk in the fix above is swallowing genuine errors along with the
    /// dead terminal, leaving the screensaver failing silently.
    #[test]
    fn other_errors_are_not_mistaken_for_a_dead_terminal() {
        let empty = std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "the morph target is empty -- nothing to assemble into",
        );
        assert!(!terminal_is_gone(&empty));

        let missing = std::io::Error::from(std::io::ErrorKind::NotFound);
        assert!(!terminal_is_gone(&missing));

        let denied = std::io::Error::from_raw_os_error(libc::EACCES);
        assert!(!terminal_is_gone(&denied));
    }
}

/// Restores the terminal however we leave the render loop.
pub struct TermGuard {
    raw: bool,
}

impl TermGuard {
    /// Enter the alternate screen, hide the cursor, and (unless `managed`) put
    /// the terminal in raw mode. The launcher owns the keyboard in managed
    /// mode, so raw mode would only steal its input.
    ///
    /// Returns the guard alongside the buffered stdout: dropping the guard
    /// restores the terminal, so keep it alive for the whole render loop.
    pub fn enter(managed: bool) -> std::io::Result<(TermGuard, impl Write)> {
        let raw = !managed && crossterm::terminal::enable_raw_mode().is_ok();
        let guard = TermGuard { raw };
        let mut out = std::io::BufWriter::with_capacity(1 << 18, std::io::stdout());
        // Alternate screen and hide the cursor. The background is set per cycle,
        // from the theme.
        out.write_all(b"\x1b[?1049h\x1b[?25l\x1b[2J")?;
        out.flush()?;
        Ok((guard, out))
    }

    /// Whether entering the terminal succeeded in enabling raw mode. When it
    /// did not (or the launcher owns input), the stage machine polls for
    /// resizes only instead of reading keys.
    pub fn raw_enabled(&self) -> bool {
        self.raw
    }

    fn restore(&mut self) {
        let mut out = std::io::stdout();
        // Show cursor, reset colours, leave the alternate screen.
        let _ = out.write_all(b"\x1b[0m\x1b[?25h\x1b[?1049l");
        let _ = out.flush();
        if self.raw {
            let _ = crossterm::terminal::disable_raw_mode();
        }
    }
}

impl Drop for TermGuard {
    fn drop(&mut self) {
        self.restore();
    }
}
