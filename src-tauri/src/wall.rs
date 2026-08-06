//! The wall a blocked site puts up, and what Threshold says at it.
//!
//! Blocked names resolve to `proto::SINK_IP`, a loopback address this app owns.
//! When a browser tries one, the connection lands here. We read the name it
//! asked for, close without answering, and show the user a line they chose.
//!
//! # Why this can exist at all
//!
//! The browser's own error page cannot be customised — neither Chrome nor Edge
//! has any policy for it, and putting our own page there would need a locally
//! trusted certificate authority, which is far too high a price for decorating a
//! failure. So the page stays the browser's. The *response* is ours, in our own
//! window, and it arrives at the only moment it could matter.
//!
//! # Never a handshake
//!
//! The name is read from the TLS ClientHello's SNI extension, which the client
//! sends first, before any certificate is exchanged. We then close. Nothing is
//! presented, so there is nothing for the browser to distrust: an HSTS-preloaded
//! domain gets a plain connection error rather than a certificate interstitial.
//! That distinction is the whole reason `hosts.rs` refused a local server for
//! years, and it is why this is not one.

use std::io::Read;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use threshold_protocol as proto;

/// How long a connection may take to say what it wants before we give up on it.
///
/// A browser sends its ClientHello immediately. Anything slower is not a page
/// load, and holding the thread for it would let a stuck socket wedge the sink.
const READ_TIMEOUT: Duration = Duration::from_millis(400);

/// The most of a ClientHello we will look at. Real ones are ~1-2 KB; this is
/// bounded so a hostile or broken client cannot make us buffer without limit.
const MAX_HELLO: usize = 8 * 1024;

/// How long after showing the wall before it may appear again.
///
/// One page load makes many connections — subresources, retries, prefetches,
/// favicons — and a window per connection would be a strobe light. This is not
/// a nicety: an interruption that repeats is the thing people uninstall an app
/// to escape.
const COOLDOWN: Duration = Duration::from_secs(90);

static LAST_SHOWN: Mutex<Option<Instant>> = Mutex::new(None);

/// Should the wall be shown for this attempt, or has it just been shown?
///
/// Pure but for the clock, and the clock is passed in so the rule is testable.
fn allowed_at(last: Option<Instant>, now: Instant, cooldown: Duration) -> bool {
    match last {
        None => true,
        Some(last) => now.duration_since(last) >= cooldown,
    }
}

/// Start listening on the sink address. Failure is not fatal.
///
/// If a port cannot be bound, blocking still works exactly as before — the name
/// resolves to an address with nothing on it and the connection is refused. The
/// only thing lost is the quote, so this must never take the app down with it.
pub fn listen(app: &tauri::AppHandle) {
    let Ok(ip) = proto::SINK_IP.parse::<Ipv4Addr>() else {
        crate::log::line("wall: the sink address is not a valid address");
        return;
    };

    for port in proto::SINK_PORTS {
        let addr = SocketAddr::V4(SocketAddrV4::new(ip, port));
        let listener = match TcpListener::bind(addr) {
            Ok(listener) => listener,
            Err(err) => {
                crate::log::line(&format!(
                    "wall: could not listen on {addr} ({err}); blocking still works, \
                     but no quote will be shown"
                ));
                continue;
            }
        };

        let app = app.clone();
        std::thread::spawn(move || {
            crate::log::line(&format!("wall: listening on {addr}"));
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => handle(&app, stream, port),
                    // One refused connection is not a reason to stop listening.
                    Err(err) => crate::log::line(&format!("wall: {err}")),
                }
            }
        });
    }
}

fn handle(app: &tauri::AppHandle, mut stream: TcpStream, port: u16) {
    // Claim the right to show before reading anything.
    //
    // A single page load opens many connections, and this loop takes them one at
    // a time: reading each one first would hold every other request for the read
    // timeout apiece. Claiming first means the burst closes instantly and only
    // the one connection that will actually be answered pays for a read.
    //
    // Checked and set under one lock because there is a thread per port, and two
    // walls for one page is the thing the cooldown exists to prevent.
    {
        let mut last = match LAST_SHOWN.lock() {
            Ok(last) => last,
            // A panicked holder must not silence the wall forever.
            Err(poisoned) => poisoned.into_inner(),
        };
        if !allowed_at(*last, Instant::now(), COOLDOWN) {
            return;
        }
        *last = Some(Instant::now());
    }

    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let mut buffer = vec![0u8; MAX_HELLO];
    let read = stream.read(&mut buffer).unwrap_or(0);
    // Closed before anything is written back. No certificate, no response, no
    // interstitial - just a connection that went nowhere.
    drop(stream);

    // The name is a nicety; the wall is shown either way. A client that connects
    // and says nothing still means somebody tried.
    let host = match (read, port) {
        (0, _) => None,
        (_, 443) => sni(&buffer[..read]),
        _ => http_host(&buffer[..read]),
    };

    crate::blocked::show(app, host.as_deref());
}

/// The name from a TLS ClientHello, if it carried one.
///
/// Every length here is checked against what is actually present. A hand-written
/// version of this walked off the end of the buffer on the first real capture
/// and returned a slice of the cipher list as a hostname, which is exactly the
/// shape of bug that ends up in a window title.
///
/// `None` is fine and common: a client connecting to a bare address sends no
/// SNI at all, and the wall can still be shown without naming the site.
fn sni(buf: &[u8]) -> Option<String> {
    // TLS record header: content type 0x16 (handshake), version, length.
    if buf.len() < 43 || buf[0] != 0x16 {
        return None;
    }
    // Handshake header (4) + client version (2) + random (32) = 38, after the
    // 5-byte record header.
    let mut i = 43usize;

    let session = *buf.get(i)? as usize;
    i = i.checked_add(1 + session)?;

    let suites = u16::from_be_bytes([*buf.get(i)?, *buf.get(i + 1)?]) as usize;
    i = i.checked_add(2 + suites)?;

    let compression = *buf.get(i)? as usize;
    i = i.checked_add(1 + compression)?;

    // Extensions block length, then the extensions themselves.
    let extensions = u16::from_be_bytes([*buf.get(i)?, *buf.get(i + 1)?]) as usize;
    i = i.checked_add(2)?;
    let end = i.checked_add(extensions)?.min(buf.len());

    while i + 4 <= end {
        let kind = u16::from_be_bytes([buf[i], buf[i + 1]]);
        let len = u16::from_be_bytes([buf[i + 2], buf[i + 3]]) as usize;
        i += 4;

        if kind == 0 {
            // server_name: list length (2), name type (1), name length (2), name.
            let name_len = u16::from_be_bytes([*buf.get(i + 3)?, *buf.get(i + 4)?]) as usize;
            let start = i + 5;
            let name = buf.get(start..start.checked_add(name_len)?)?;
            return std::str::from_utf8(name)
                .ok()
                .map(|host| host.trim().to_ascii_lowercase())
                .filter(|host| !host.is_empty());
        }
        i = i.checked_add(len)?;
    }

    None
}

/// The `Host:` header from a plain HTTP request.
fn http_host(buf: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(buf).ok()?;
    text.lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.trim()
                .eq_ignore_ascii_case("host")
                .then(|| value.trim().to_ascii_lowercase())
        })
        // A port in the header is not part of the name.
        .map(|host| host.split(':').next().unwrap_or_default().to_string())
        .filter(|host| !host.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal but real ClientHello carrying one server name.
    fn hello(server_name: &str) -> Vec<u8> {
        let name = server_name.as_bytes();
        let mut ext = Vec::new();
        ext.extend_from_slice(&0u16.to_be_bytes()); // server_name
        ext.extend_from_slice(&((name.len() + 5) as u16).to_be_bytes());
        ext.extend_from_slice(&((name.len() + 3) as u16).to_be_bytes()); // list
        ext.push(0); // host_name
        ext.extend_from_slice(&(name.len() as u16).to_be_bytes());
        ext.extend_from_slice(name);

        let mut body = Vec::new();
        body.extend_from_slice(&[0x03, 0x03]); // client version
        body.extend_from_slice(&[0u8; 32]); // random
        body.push(0); // session id length
        body.extend_from_slice(&2u16.to_be_bytes()); // cipher suites length
        body.extend_from_slice(&[0x13, 0x01]);
        body.push(1); // compression methods length
        body.push(0);
        body.extend_from_slice(&(ext.len() as u16).to_be_bytes());
        body.extend_from_slice(&ext);

        let mut handshake = vec![0x01]; // client_hello
        handshake.extend_from_slice(&(body.len() as u32).to_be_bytes()[1..]);
        handshake.extend_from_slice(&body);

        let mut record = vec![0x16, 0x03, 0x01];
        record.extend_from_slice(&(handshake.len() as u16).to_be_bytes());
        record.extend_from_slice(&handshake);
        record
    }

    #[test]
    fn reads_the_name_a_browser_asked_for() {
        assert_eq!(sni(&hello("x.com")), Some("x.com".to_string()));
        assert_eq!(
            sni(&hello("news.ycombinator.com")),
            Some("news.ycombinator.com".to_string())
        );
    }

    #[test]
    fn a_name_in_capitals_comes_back_lowercase() {
        assert_eq!(sni(&hello("WWW.X.COM")), Some("www.x.com".to_string()));
    }

    /// Every one of these walked off the end of the buffer in the first version.
    #[test]
    fn a_truncated_or_hostile_hello_returns_nothing_rather_than_rubbish() {
        let full = hello("x.com");
        for cut in 0..full.len() {
            let _ = sni(&full[..cut]);
        }
        assert_eq!(sni(&[]), None);
        assert_eq!(sni(&[0x16]), None);
        assert_eq!(sni(&[0xff; 200]), None);
        // A record claiming enormous lengths must not be believed.
        let mut lying = hello("x.com");
        lying[43] = 0xff;
        assert_eq!(sni(&lying), None);
    }

    #[test]
    fn a_connection_that_is_not_tls_is_not_a_name() {
        assert_eq!(sni(b"GET / HTTP/1.1\r\nHost: x.com\r\n\r\n"), None);
    }

    #[test]
    fn reads_the_host_header_from_plain_http() {
        assert_eq!(
            http_host(b"GET / HTTP/1.1\r\nHost: X.com\r\nAccept: */*\r\n\r\n"),
            Some("x.com".to_string())
        );
        assert_eq!(
            http_host(b"GET / HTTP/1.1\r\nHost: x.com:8080\r\n\r\n"),
            Some("x.com".to_string())
        );
        assert_eq!(http_host(b"GET / HTTP/1.1\r\n\r\n"), None);
    }

    #[test]
    fn the_wall_waits_before_showing_itself_again() {
        let now = Instant::now();
        assert!(allowed_at(None, now, COOLDOWN), "the first attempt shows");
        assert!(
            !allowed_at(Some(now), now, COOLDOWN),
            "a page's other requests must not each raise a window"
        );
        assert!(allowed_at(
            Some(now - COOLDOWN - Duration::from_secs(1)),
            now,
            COOLDOWN
        ));
    }
}
