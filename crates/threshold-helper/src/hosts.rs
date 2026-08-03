//! Editing the hosts file without ever being able to break browsing.
//!
//! Two rules make that true:
//!   * Threshold only ever owns the text between its markers. Everything
//!     outside them is copied through untouched, so an existing hosts file
//!     survives intact.
//!   * Writes are atomic — a temp file in the same directory, then a replace.
//!     A crash mid-write leaves the old file, never half of a new one.
//!
//! `0.0.0.0` rather than `127.0.0.1`: it fails the connection instantly instead
//! of waiting for a local timeout. Never a redirect to a local server either —
//! the social domains are HSTS-preloaded, so that produces certificate
//! interstitials rather than a clean stop.

use std::path::{Path, PathBuf};

pub const BEGIN: &str = "# THRESHOLD-BEGIN";
pub const END: &str = "# THRESHOLD-END";

pub fn system_hosts() -> PathBuf {
    PathBuf::from(r"C:\Windows\System32\drivers\etc\hosts")
}

/// The line ending the file already uses.
///
/// Windows hosts files are CRLF. Rewriting the whole file to LF would work, but
/// it changes bytes outside our block - which is exactly what this module
/// promises never to do.
pub fn line_ending(existing: &str) -> &'static str {
    if existing.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

/// Strip any existing Threshold block, returning the untouched remainder.
///
/// Self-healing on purpose: an unterminated block (from a crash, or a hand
/// edit) is dropped from the marker to the end rather than being left to
/// accumulate forever.
pub fn without_block(existing: &str) -> String {
    let eol = line_ending(existing);
    let mut out = String::with_capacity(existing.len());
    let mut inside = false;

    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed == BEGIN {
            inside = true;
            continue;
        }
        if trimmed == END {
            inside = false;
            continue;
        }
        if !inside {
            out.push_str(line);
            out.push_str(eol);
        }
    }

    // Collapse trailing blank lines so repeated writes do not grow the file.
    let doubled = format!("{eol}{eol}");
    while out.ends_with(&doubled) {
        out.truncate(out.len() - eol.len());
    }
    out
}

/// The file contents with the given hosts blocked. An empty list removes the
/// block entirely rather than leaving empty markers behind.
pub fn with_block(existing: &str, hosts: &[String]) -> String {
    let eol = line_ending(existing);
    let base = without_block(existing);

    if hosts.is_empty() {
        return base;
    }

    let mut out = base;
    if !out.is_empty() && !out.ends_with(eol) {
        out.push_str(eol);
    }
    out.push_str(BEGIN);
    out.push_str(eol);
    for host in hosts {
        out.push_str("0.0.0.0 ");
        out.push_str(host);
        out.push_str(eol);
    }
    out.push_str(END);
    out.push_str(eol);
    out
}

pub fn read(path: &Path) -> Result<String, String> {
    match std::fs::read_to_string(path) {
        Ok(contents) => Ok(contents),
        // A missing hosts file is unusual but not fatal — treat it as empty.
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(err) => Err(format!("could not read {}: {err}", path.display())),
    }
}

/// Write atomically: temp file beside the target, then replace.
pub fn write_atomic(path: &Path, contents: &str) -> Result<(), String> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let temp = dir.join("hosts.threshold.tmp");

    std::fs::write(&temp, contents)
        .map_err(|err| format!("could not stage hosts file: {err} (Defender may block this)"))?;

    std::fs::rename(&temp, path).map_err(|err| {
        let _ = std::fs::remove_file(&temp);
        format!("could not replace hosts file: {err}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXISTING: &str = "127.0.0.1 localhost\n::1 localhost\n";

    fn hosts(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn preserves_entries_outside_the_markers() {
        let result = with_block(EXISTING, &hosts(&["example.com"]));
        assert!(result.contains("127.0.0.1 localhost"));
        assert!(result.contains("::1 localhost"));
    }

    #[test]
    fn blocks_with_a_null_route_not_loopback() {
        let result = with_block(EXISTING, &hosts(&["example.com"]));
        assert!(result.contains("0.0.0.0 example.com"));
        assert!(!result.contains("127.0.0.1 example.com"));
    }

    #[test]
    fn rewriting_replaces_rather_than_appends() {
        let once = with_block(EXISTING, &hosts(&["a.com"]));
        let twice = with_block(&once, &hosts(&["b.com"]));
        assert_eq!(twice.matches(BEGIN).count(), 1);
        assert!(!twice.contains("a.com"));
        assert!(twice.contains("b.com"));
    }

    #[test]
    fn unblocking_leaves_the_original_file() {
        let blocked = with_block(EXISTING, &hosts(&["a.com"]));
        let cleared = with_block(&blocked, &[]);
        assert_eq!(cleared.trim_end(), EXISTING.trim_end());
        assert!(!cleared.contains(BEGIN));
    }

    #[test]
    fn an_unterminated_block_is_healed_rather_than_accumulating() {
        let damaged = format!("{EXISTING}{BEGIN}\n0.0.0.0 a.com\n");
        let cleared = without_block(&damaged);
        assert!(!cleared.contains("a.com"));
        assert!(cleared.contains("127.0.0.1 localhost"));
    }

    #[test]
    fn a_crlf_file_stays_crlf() {
        let windows_style = "127.0.0.1 localhost\r\n::1 localhost\r\n";
        let blocked = with_block(windows_style, &hosts(&["a.com"]));
        assert!(blocked.contains("\r\n"), "CRLF must be preserved");
        assert!(
            !blocked.replace("\r\n", "").contains('\n'),
            "no bare LF should be introduced"
        );
    }

    #[test]
    fn blocking_then_unblocking_restores_the_file_byte_for_byte() {
        // The whole promise of this module: bytes outside the markers are ours
        // to read and nobody's to rewrite.
        for original in [
            "127.0.0.1 localhost\r\n::1 localhost\r\n",
            "127.0.0.1 localhost\n::1 localhost\n",
        ] {
            let blocked = with_block(original, &hosts(&["a.com", "b.com"]));
            let restored = with_block(&blocked, &[]);
            assert_eq!(restored, original, "round trip must be byte-identical");
        }
    }

    #[test]
    fn repeated_writes_do_not_grow_the_file() {
        let mut current = EXISTING.to_string();
        for _ in 0..5 {
            current = with_block(&current, &hosts(&["a.com"]));
            current = with_block(&current, &[]);
        }
        assert_eq!(current.trim_end(), EXISTING.trim_end());
    }
}
