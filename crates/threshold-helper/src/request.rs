//! The only input this binary trusts, and only after checking it.
//!
//! The helper runs from a "highest privileges" scheduled task, which means
//! anything able to trigger that task decides what it does. It therefore
//! accepts no command-line arguments at all and reads a single file at a fixed
//! path, validating the shape before acting.

use std::path::{Path, PathBuf};

// The wire types are shared with the app: they cross a process boundary that is
// also a privilege boundary, and two private copies cannot be kept in step.
pub use threshold_protocol::{Action, Request};

#[derive(Debug)]
pub enum RequestError {
    Missing(PathBuf),
    Unreadable(String),
    Malformed(String),
    Invalid(String),
}

impl std::fmt::Display for RequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestError::Missing(path) => write!(f, "no request at {}", path.display()),
            RequestError::Unreadable(err) => write!(f, "could not read request: {err}"),
            RequestError::Malformed(err) => write!(f, "request is not valid JSON: {err}"),
            RequestError::Invalid(err) => write!(f, "request rejected: {err}"),
        }
    }
}

pub fn program_data() -> PathBuf {
    threshold_protocol::program_data()
}

/// Admin-only subdirectory holding the lock.
///
/// The request file must stay writable by the ordinary user - that is how the
/// UI asks for anything. The lock must not: if you could edit it, you could
/// simply shorten your own commitment, and the whole mechanism would be
/// decorative.
pub fn state_dir() -> PathBuf {
    threshold_protocol::state_dir()
}

pub fn request_path() -> PathBuf {
    threshold_protocol::request_path()
}

pub fn load(path: &Path) -> Result<Request, RequestError> {
    if !path.exists() {
        return Err(RequestError::Missing(path.to_path_buf()));
    }

    let raw = std::fs::read_to_string(path).map_err(|err| RequestError::Unreadable(err.to_string()))?;
    // Several Windows tools prepend a UTF-8 BOM; JSON parsers reject it. Whose
    // editor wrote the file is not something this should care about.
    let raw = raw.trim_start_matches('\u{feff}');
    let request: Request =
        serde_json::from_str(raw).map_err(|err| RequestError::Malformed(err.to_string()))?;

    validate(&request)?;
    Ok(request)
}

/// Reject anything that does not describe a coherent action, rather than
/// guessing at intent while holding administrator rights.
///
/// The rules live in the shared protocol crate so the side that writes requests
/// is held to the same standard as the side that reads them. This still runs on
/// receipt regardless: the helper holds administrator rights and trusts nobody,
/// including a caller that claims to have validated already.
pub fn validate(request: &Request) -> Result<(), RequestError> {
    threshold_protocol::validate(request).map_err(RequestError::Invalid)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block() -> Request {
        Request {
            action: Action::Block,
            categories: vec!["social".into()],
            until: Some(1_800_000_000),
            dry_run: false,
        }
    }

    #[test]
    fn accepts_a_well_formed_block() {
        assert!(validate(&block()).is_ok());
    }

    #[test]
    fn rejects_a_block_with_no_end() {
        let mut request = block();
        request.until = None;
        assert!(validate(&request).is_err());
    }

    #[test]
    fn rejects_a_block_with_no_categories() {
        let mut request = block();
        request.categories.clear();
        assert!(validate(&request).is_err());
    }

    #[test]
    fn rejects_category_names_outside_the_expected_shape() {
        let mut request = block();
        request.categories = vec!["../../windows".into()];
        assert!(validate(&request).is_err());
    }

    #[test]
    fn unblock_needs_neither_end_nor_categories() {
        let request = Request {
            action: Action::Unblock,
            categories: vec![],
            until: None,
            dry_run: false,
        };
        assert!(validate(&request).is_ok());
    }

    #[test]
    fn parses_the_documented_wire_shape() {
        let raw = r#"{"action":"block","categories":["social","video"],"until":1800000000}"#;
        let request: Request = serde_json::from_str(raw).expect("parses");
        assert_eq!(request.action, Action::Block);
        assert_eq!(request.categories.len(), 2);
        assert!(!request.dry_run);
    }
}
