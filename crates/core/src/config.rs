//! The config file, shared by every screensaver.
//!
//! The launcher passes no arguments of its own, so settings live in
//! `~/.config/omarchy-screensavers/config`. The old poincare path,
//! `~/.config/omarchy-poincare/config`, is read as a fallback so existing
//! installs keep working. One option per line, `#` for comments, and a value
//! is the rest of its line -- so `--text MY NAME` needs no quoting.
//! Command-line arguments are appended after these and later options win, so a
//! flag typed by hand overrides the file.

use std::path::PathBuf;

/// Which config file to read: the new canonical path, falling back to the old
/// poincare one. Explicit env vars win over both.
pub fn config_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("OMARCHY_SCREENSAVERS_CONFIG") {
        return Some(PathBuf::from(p));
    }
    if let Ok(p) = std::env::var("OMARCHY_POINCARE_CONFIG") {
        return Some(PathBuf::from(p));
    }
    match std::env::var("HOME") {
        Ok(h) => {
            let new = PathBuf::from(&h).join(".config/omarchy-screensavers/config");
            if new.is_file() {
                return Some(new);
            }
            let old = PathBuf::from(&h).join(".config/omarchy-poincare/config");
            if old.is_file() {
                return Some(old);
            }
            // Neither exists: point at the new path so a read just finds
            // nothing, rather than erroring.
            Some(new)
        }
        Err(_) => None,
    }
}

/// Options read from the config file, which is how the screensaver gets
/// configured: the launcher passes no arguments of its own.
pub fn config_args() -> Vec<String> {
    match config_path() {
        Some(path) => match std::fs::read_to_string(&path) {
            Ok(text) => parse_config(&text),
            Err(_) => Vec::new(),
        },
        None => Vec::new(),
    }
}

/// Splits config text into arguments. Separated from the file handling so it
/// can be tested directly.
pub fn parse_config(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        // A '#' preceded by whitespace starts a trailing comment. Requiring the
        // whitespace keeps '#' usable inside a value.
        let line = match line
            .char_indices()
            .find(|&(i, c)| c == '#' && (i == 0 || line[..i].ends_with(char::is_whitespace)))
        {
            Some((i, _)) => &line[..i],
            None => line,
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match line.split_once(char::is_whitespace) {
            Some((flag, value)) => {
                out.push(flag.to_string());
                let value = value.trim();
                if !value.is_empty() {
                    out.push(value.to_string());
                }
            }
            None => out.push(line.to_string()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_is_the_rest_of_the_line_so_text_needs_no_quoting() {
        assert_eq!(parse_config("--text MY NAME\n"), ["--text", "MY NAME"]);
    }

    /// This one bit in practice: `--sim 10   # seconds` parsed the comment as
    /// part of the value.
    #[test]
    fn a_trailing_comment_is_stripped() {
        assert_eq!(parse_config("--sim 10   # seconds of orbit\n"), ["--sim", "10"]);
        assert_eq!(parse_config("--once  # just one\n"), ["--once"]);
    }

    /// ...but only when the # is preceded by whitespace, so it stays usable
    /// inside a value.
    #[test]
    fn a_hash_inside_a_value_survives() {
        assert_eq!(parse_config("--text A#B\n"), ["--text", "A#B"]);
    }

    #[test]
    fn blank_lines_and_whole_line_comments_are_ignored() {
        assert!(parse_config("\n   \n# a comment\n\t# another\n").is_empty());
    }
}
