//! A small rolling log beside the database.
//!
//! The shipped build is a GUI binary with no console, so every decision the app
//! made was invisible: "the ritual did not appear" and "the ritual was
//! deliberately suppressed" looked identical from the outside, and diagnosing
//! them took a full evening of guesswork.
//!
//! Deliberately tiny. It records what the app decided and why — not activity,
//! not content. Intentions live in the database; nothing about what you were
//! doing goes here.

use std::io::Write;

const MAX_BYTES: u64 = 256 * 1024;

pub fn path() -> Option<std::path::PathBuf> {
    crate::db::data_dir().ok().map(|dir| dir.join("threshold.log"))
}

/// Append a line, and start over if the file has grown past its cap.
pub fn line(message: &str) {
    // Still print, so `--doctor` and a dev console keep working.
    println!("{message}");

    let Some(path) = path() else { return };

    if std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) > MAX_BYTES {
        let _ = std::fs::remove_file(&path);
    }

    let stamped = format!(
        "{} {message}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    );

    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = file.write_all(stamped.as_bytes());
    }
}

/// The last few lines, for `--log` and the settings panel.
pub fn tail(count: usize) -> Vec<String> {
    let Some(path) = path() else { return Vec::new() };
    let Ok(contents) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let lines: Vec<&str> = contents.lines().collect();
    lines
        .iter()
        .rev()
        .take(count)
        .rev()
        .map(|line| line.to_string())
        .collect()
}
