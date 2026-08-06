//! The test that would have caught the bug that shipped.
//!
//! Blocking was fully implemented in the helper and fully absent from the app:
//! the ritual recorded a session and never asked anyone to block anything. Every
//! unit test passed, because every unit worked. Nothing checked that the pieces
//! were connected.
//!
//! These assert the connection itself, at the seams where it broke.

use threshold_protocol as proto;

/// The app and the helper must agree on the request format, and they now share
/// one definition of it. When each kept its own copy, nothing forced them to.
#[test]
fn a_request_the_app_builds_passes_the_helpers_validator() {
    let request = proto::Request::block(
        vec!["social".into(), "video".into()],
        vec!["pinterest.com".into()],
        1_900_000_000,
        1_899_999_000,
    );
    proto::validate(&request).expect("the app must not build a request the helper rejects");

    let encoded = serde_json::to_string(&request).expect("encode");
    let decoded: proto::Request = proto::parse_json(&encoded).expect("decode");
    proto::validate(&decoded).expect("survives a round trip through the file");
    assert_eq!(decoded.action, proto::Action::Block);
    assert_eq!(decoded.until, Some(1_900_000_000));
    // The field that would vanish in transit if either side forgot it, taking
    // the user's own sites with it and reporting success anyway.
    assert_eq!(decoded.custom_hosts, vec!["pinterest.com".to_string()]);
    assert_eq!(decoded.issued_at, Some(1_899_999_000));
}

#[test]
fn unblock_requests_are_valid_without_categories_or_an_end() {
    proto::validate(&proto::Request::unblock()).expect("unblock");
    proto::validate(&proto::Request::emergency_unblock()).expect("emergency");
}

/// A block with nothing to block, or no end, must be refused at the point it is
/// built rather than written to disk and silently discarded.
#[test]
fn an_incoherent_block_is_refused_before_it_is_ever_written() {
    let now = 1_899_999_000;
    let mut request = proto::Request::block(vec![], vec![], 1_900_000_000, now);
    assert!(proto::validate(&request).is_err(), "nothing to block");

    request = proto::Request::block(vec!["social".into()], vec![], 0, now);
    assert!(proto::validate(&request).is_err(), "no end time");

    request = proto::Request::block(vec!["../../windows".into()], vec![], 1_900_000_000, now);
    assert!(proto::validate(&request).is_err(), "smuggled path");

    request = proto::Request::block(vec![], vec!["../../windows".into()], 1_900_000_000, now);
    assert!(proto::validate(&request).is_err(), "smuggled site");
}

/// The confirmation the app performs is a real read of the real file, so a
/// helper that fails silently cannot be mistaken for one that worked. This is
/// the check that turns "we asked" into "it happened".
#[test]
fn block_detection_reads_the_actual_hosts_file() {
    let reported = proto::hosts_has_block();
    let actual = std::fs::read_to_string(proto::system_hosts())
        .map(|contents| contents.contains(proto::HOSTS_BEGIN))
        .unwrap_or(false);
    assert_eq!(
        reported, actual,
        "hosts_has_block must reflect the file on disk, not an assumption"
    );
}

/// "Not blocked" and "could not tell" must never be the same answer.
///
/// They were: the check swallowed a read error as `false`, so a momentarily
/// unreadable hosts file satisfied the unblock confirmation on its very first
/// poll and the app announced that the sites were open again.
#[test]
fn a_hosts_file_we_cannot_read_is_not_reported_as_clear() {
    if let proto::HostsState::Unreadable(_) = proto::hosts_state() {
        assert!(!proto::hosts_confirmed_clear());
    }
    // And the two questions stay distinguishable in principle.
    assert!(!matches!(
        proto::parse_hosts("127.0.0.1 localhost\n"),
        proto::HostsState::Unreadable(_)
    ));
    assert!(proto::hosts_confirmed_clear() != proto::hosts_has_block() || {
        matches!(proto::hosts_state(), proto::HostsState::Unreadable(_))
    });
}

/// Both binaries must resolve the same paths, or the app writes a request the
/// helper never reads — which is exactly what a duplicated constant would allow.
#[test]
fn both_sides_agree_on_where_the_request_and_lock_live() {
    assert!(proto::request_path().ends_with("request.json"));
    assert!(proto::lock_path().ends_with(r"state\lock.json"));
    assert!(proto::request_path().starts_with(proto::PROGRAM_DATA));
    assert!(proto::lock_path().starts_with(proto::PROGRAM_DATA));
}
