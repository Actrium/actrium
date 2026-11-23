use crate::{Result, SecurityError};

/// An RTC control frame cannot be decoded as a business fragment: its
/// fragment count (bytes 6–7) is zero. Only the reliable lane carries it.
pub const RTC_AUTH_FRAME_PREFIX: &[u8] = b"ACTR\0\0\0\0";

/// Return the single SHA-256 fingerprint shared by the SDP's DTLS media
/// sections. Reject ambiguity so authentication covers both SCTP and SRTP.
pub fn dtls_fingerprint(sdp: &str) -> Result<String> {
    let mut fingerprint = None;
    let mut session_fingerprint = false;
    let mut media_fingerprint = None;
    for line in sdp.lines() {
        if line.starts_with("m=") {
            if media_fingerprint == Some(false) && !session_fingerprint {
                return Err(SecurityError(
                    "WebRTC media section has no DTLS fingerprint",
                ));
            }
            let active = line.split_whitespace().nth(1) != Some("0");
            media_fingerprint = Some(!active);
            continue;
        }
        let Some(value) = line.strip_prefix("a=fingerprint:") else {
            continue;
        };
        let mut fields = value.split_whitespace();
        if !fields
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case("sha-256"))
        {
            return Err(SecurityError(
                "unsupported WebRTC DTLS fingerprint algorithm",
            ));
        }
        let value = fields
            .next()
            .ok_or(SecurityError("missing WebRTC DTLS fingerprint"))?;
        if fields.next().is_some()
            || value.len() != 95
            || !value
                .split(':')
                .all(|byte| byte.len() == 2 && byte.bytes().all(|digit| digit.is_ascii_hexdigit()))
        {
            return Err(SecurityError("invalid WebRTC DTLS fingerprint"));
        }
        let value = value.to_ascii_lowercase();
        if fingerprint
            .as_ref()
            .is_some_and(|existing| existing != &value)
        {
            return Err(SecurityError("ambiguous WebRTC DTLS fingerprint"));
        }
        fingerprint = Some(value);
        if let Some(media) = media_fingerprint.as_mut() {
            *media = true;
        } else {
            session_fingerprint = true;
        }
    }
    if media_fingerprint == Some(false) && !session_fingerprint {
        return Err(SecurityError(
            "WebRTC media section has no DTLS fingerprint",
        ));
    }
    fingerprint.ok_or(SecurityError("missing WebRTC DTLS fingerprint"))
}
