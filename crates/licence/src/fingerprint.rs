//! The machine fingerprint: derived on this machine, never sent anywhere.
//!
//! SHA-256 of a fixed domain tag and the platform's own stable id — on macOS
//! the `IOPlatformUUID` from `ioreg -rd1 -c IOPlatformExpertDevice`, on Linux
//! `/etc/machine-id`. When neither is available the answer is
//! [`Fingerprint::Unavailable`], said as such; there is no fallback guess,
//! because a guessed fingerprint is a licence that works on Tuesday and not on
//! Wednesday for no reason a person could find.
//!
//! Running `ioreg` spawns a local process; it opens no socket.

use sha2::{Digest, Sha256};

/// The domain tag, so this hash cannot be mistaken for any other.
const TAG: &[u8] = b"thruline-licence-machine-v1\0";

/// What this machine is, for a licence tied to one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fingerprint {
    /// 64 lowercase hex characters.
    Available(String),
    /// No stable platform id could be read.
    Unavailable,
}

/// The fingerprint of a platform id. Public so a test can derive the expected
/// value without reading this machine.
pub fn of_platform_id(id: &str) -> Fingerprint {
    let id = id.trim();
    if id.is_empty() {
        return Fingerprint::Unavailable;
    }
    let mut h = Sha256::new();
    h.update(TAG);
    h.update(id.as_bytes());
    Fingerprint::Available(crate::to_hex(&h.finalize()))
}

/// This machine's fingerprint.
pub fn local() -> Fingerprint {
    platform_id().map_or(Fingerprint::Unavailable, |id| of_platform_id(&id))
}

#[cfg(target_os = "macos")]
fn platform_id() -> Option<String> {
    let out = std::process::Command::new("/usr/sbin/ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    ioreg_uuid(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(target_os = "linux")]
fn platform_id() -> Option<String> {
    let id = std::fs::read_to_string("/etc/machine-id").ok()?;
    let id = id.trim();
    (!id.is_empty()).then(|| id.to_string())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn platform_id() -> Option<String> {
    None
}

/// The `IOPlatformUUID` value out of `ioreg`'s text.
pub fn ioreg_uuid(text: &str) -> Option<String> {
    let line = text.lines().find(|l| l.contains("\"IOPlatformUUID\""))?;
    let value = line.split('=').nth(1)?.trim().trim_matches('"').trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_uuid_is_read_out_of_ioreg_text() {
        let text = "+-o Mac  <class IOPlatformExpertDevice>\n    {\n      \"IOPlatformUUID\" = \"cccccccc-1111-2222-3333-444444444444\"\n    }\n";
        assert_eq!(
            ioreg_uuid(text).as_deref(),
            Some("cccccccc-1111-2222-3333-444444444444")
        );
        assert_eq!(ioreg_uuid("no such line"), None);
    }

    #[test]
    fn an_empty_id_is_unavailable_never_a_hash_of_nothing() {
        assert_eq!(of_platform_id("  "), Fingerprint::Unavailable);
        match of_platform_id("abc") {
            Fingerprint::Available(h) => assert_eq!(h.len(), 64),
            Fingerprint::Unavailable => panic!("a real id must hash"),
        }
    }
}
